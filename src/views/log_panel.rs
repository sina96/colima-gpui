//! Terminal-style live log panel for any `Receiver<LogLine>`: `colima start` output and
//! `docker logs --follow`. Blocking channel reads stay on the background executor.
//!
//! - **Following / Paused:** new output scrolls into view; scrolling up or selecting text
//!   pauses it, and the toggle or scrolling back to the bottom resumes it.
//! - **Wrap:** long lines wrap instead of scrolling sideways.
//! - **Selection:** drag, Shift-click, double-click (word), triple-click (line), ⌘A, ⌘C,
//!   Esc to clear, and a right-click menu. Copies the text as shown.
//! - **Copy / Save…:** copy the raw transcript, or write it to a file via the save dialog.
//!
//! Builders: `.running_label("Starting…")` for the status while the stream is open,
//! `.plain_stderr()` for container logs, where stderr is ordinary output rather than an error,
//! and `.save_name("demo-api")` for Save's default file name (`<name>-log-<date>-<time>.log`).
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use gpui::{
    App, ClipboardItem, Context, CursorStyle, EventEmitter, FocusHandle, Global, HighlightStyle,
    Hsla, KeyBinding, ListHorizontalSizingBehavior, MouseButton, MouseDownEvent, MouseMoveEvent,
    Pixels, Point, ScrollEvent, ScrollHandle, SharedString, Stateful, StyledText, Task, TextLayout,
    UniformListScrollHandle, Window, actions, anchored, deferred, div, point, prelude::*, px,
    uniform_list,
};

use super::log_selection::{Pos, Selection, word_at};
use crate::colima::commands::LogLine;
use crate::theme::{ActiveTheme, MONO_FONT, Theme};
use crate::ui::{Button, ButtonStyle, IconName};

const BATCH_SIZE: usize = 256;
const RECEIVE_TIMEOUT: Duration = Duration::from_millis(50);
/// Followed logs never end; keep the newest lines only.
const MAX_LINES: usize = 10_000;
/// Wrapped lines have varying heights, so wrap mode renders without virtualization;
/// cap it to the newest lines to stay fast.
const MAX_WRAPPED_LINES: usize = 2_000;
const KEY_CONTEXT: &str = "LogPanel";

actions!(log_panel, [SelectAll, CopySelection, ClearSelection]);

/// Subscribe before the next event-loop turn to refresh the profile on completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogFinished {
    /// None means the sender disappeared without reporting an exit status.
    pub exit_code: Option<i32>,
}

#[derive(Default)]
struct Transcript {
    lines: Vec<LogLine>,
    /// Lines dropped from the front so far; `lines[i]` is absolute line `dropped + i`.
    dropped: usize,
    finished: Option<LogFinished>,
}

impl Transcript {
    fn append(&mut self, lines: Vec<LogLine>, disconnected: bool) {
        for line in lines {
            if self.finished.is_some() {
                break;
            }
            if let LogLine::Exit(code) = &line {
                self.finished = Some(LogFinished {
                    exit_code: Some(*code),
                });
            }
            self.lines.push(line);
        }
        if self.lines.len() > MAX_LINES {
            let excess = self.lines.len() - MAX_LINES;
            self.lines.drain(..excess);
            self.dropped += excess;
        }
        if disconnected && self.finished.is_none() {
            self.lines.push(LogLine::Stderr(
                "Log stream closed without an exit status.".into(),
            ));
            self.finished = Some(LogFinished { exit_code: None });
        }
    }

    fn text(&self) -> String {
        self.lines
            .iter()
            .map(line_text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn line_text(line: &LogLine) -> String {
    match line {
        LogLine::Stdout(text) | LogLine::Stderr(text) => text.clone(),
        LogLine::Exit(0) => "Process exited successfully (code 0).".into(),
        LogLine::Exit(code) => format!("Process exited with code {code}."),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Normal,
    Warning,
    Error,
    Success,
}

/// Colima logs progress to stderr as logfmt (`time=… level=info msg="…"`), so
/// stderr alone doesn't mean failure. Shows the `msg` and grades by `level`.
fn display(line: &LogLine, plain_stderr: bool) -> (String, Severity) {
    match line {
        LogLine::Stdout(text) => (text.clone(), Severity::Normal),
        LogLine::Stderr(text) if plain_stderr => (text.clone(), Severity::Normal),
        LogLine::Stderr(text) => match parse_logfmt(text) {
            Some((level, msg)) => {
                let severity = match level.as_str() {
                    "warn" | "warning" => Severity::Warning,
                    "error" | "fatal" | "panic" => Severity::Error,
                    _ => Severity::Normal,
                };
                (msg, severity)
            }
            None => (text.clone(), Severity::Error),
        },
        LogLine::Exit(0) => (line_text(line), Severity::Success),
        LogLine::Exit(_) => (line_text(line), Severity::Error),
    }
}

/// Extracts `level` and `msg` from a logfmt line; `None` if it isn't one.
fn parse_logfmt(text: &str) -> Option<(String, String)> {
    let field = |key: &str| -> Option<String> {
        let start = text.find(&format!("{key}="))? + key.len() + 1;
        let rest = &text[start..];
        if let Some(quoted) = rest.strip_prefix('"') {
            let mut value = String::new();
            let mut chars = quoted.chars();
            while let Some(ch) = chars.next() {
                match ch {
                    '\\' => value.extend(chars.next()),
                    '"' => return Some(value),
                    _ => value.push(ch),
                }
            }
            None
        } else {
            Some(rest.split(' ').next().unwrap_or_default().to_owned())
        }
    };
    let level = field("level")?;
    let msg = field("msg")?;
    Some((level, msg))
}

/// Reads at most one bounded batch. The receiver returns to the UI task by
/// ownership, but is only ever read inside a background task.
fn receive_batch(receiver: &Receiver<LogLine>) -> (Vec<LogLine>, bool) {
    let mut lines = Vec::with_capacity(BATCH_SIZE);
    match receiver.recv_timeout(RECEIVE_TIMEOUT) {
        Ok(line) => lines.push(line),
        Err(RecvTimeoutError::Timeout) => return (lines, false),
        Err(RecvTimeoutError::Disconnected) => return (lines, true),
    }
    while lines.len() < BATCH_SIZE {
        match receiver.try_recv() {
            Ok(line) => lines.push(line),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => return (lines, true),
        }
    }
    (lines, false)
}

/// Byte index under `position` in a painted line. Positions above or below the text (the
/// row is taller than the text) count as on it, so a drag along a row picks columns.
fn hit(layout: &TextLayout, position: Point<Pixels>) -> usize {
    let bounds = layout.bounds();
    let y = if position.y < bounds.top() {
        bounds.top()
    } else if position.y >= bounds.bottom() {
        bounds.bottom() - px(1.)
    } else {
        position.y
    };
    match layout.index_for_position(point(position.x, y)) {
        Ok(index) | Err(index) => index,
    }
}

/// `2026-10-25-1023` in local time, for file names. `date` is part of macOS and Linux.
fn timestamp() -> String {
    std::process::Command::new("/bin/date")
        .arg("+%Y-%m-%d-%H%M")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|stamp| !stamp.is_empty())
        .unwrap_or_else(|| "log".into())
}

/// Save's suggested file name: `demo-api-log-2026-10-25-1023.log`, or `log-….log` without a
/// name. Characters a file name can't hold become `-`.
fn file_name(name: Option<&str>, stamp: &str) -> String {
    let name: String = name
        .unwrap_or_default()
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches(['-', '.']);
    if name.is_empty() {
        format!("log-{stamp}.log")
    } else {
        format!("{name}-log-{stamp}.log")
    }
}

fn save_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let downloads = home.join("Downloads");
    if downloads.is_dir() { downloads } else { home }
}

/// Registers the panel's key bindings once, the first time a panel is created.
struct KeysBound;
impl Global for KeysBound {}

fn bind_keys(cx: &mut App) {
    if cx.has_global::<KeysBound>() {
        return;
    }
    cx.set_global(KeysBound);
    cx.bind_keys([
        KeyBinding::new("cmd-a", SelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-c", CopySelection, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", ClearSelection, Some(KEY_CONTEXT)),
    ]);
}

pub struct LogPanel {
    transcript: Transcript,
    running_label: SharedString,
    plain_stderr: bool,
    save_name: Option<SharedString>,
    follow: bool,
    wrap: bool,
    scroll: UniformListScrollHandle,
    wrap_scroll: ScrollHandle,
    focus: FocusHandle,
    selection: Option<Selection>,
    /// The left button went down on a line and hasn't come up yet.
    dragging: bool,
    /// Right-click menu, open at this window position.
    menu: Option<Point<Pixels>>,
    /// Result of the last Save…, shown in the toolbar: (text, succeeded).
    notice: Option<(SharedString, bool)>,
    // Cancelling this task drops the receiver after at most RECEIVE_TIMEOUT.
    // It does not terminate the Colima process, which can outlive this view.
    _drain: Task<()>,
}

impl EventEmitter<LogFinished> for LogPanel {}

impl LogPanel {
    pub fn new(receiver: Receiver<LogLine>, cx: &mut Context<Self>) -> Self {
        bind_keys(cx);
        let drain = cx.spawn(async move |this, cx| {
            let mut receiver = receiver;
            loop {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let (lines, disconnected) = receive_batch(&receiver);
                        (receiver, lines, disconnected)
                    })
                    .await;
                let (returned_receiver, lines, disconnected) = result;
                receiver = returned_receiver;
                if lines.is_empty() && !disconnected {
                    // Also check whether the view disappeared during a quiet stream.
                    if this.upgrade().is_none() {
                        break;
                    }
                    continue;
                }
                let finished = this.update(cx, |this, cx| {
                    this.transcript.append(lines, disconnected);
                    this.selection = this
                        .selection
                        .and_then(|selection| selection.clamp_to(this.transcript.dropped));
                    if this.follow {
                        this.scroll_to_end();
                    }
                    if let Some(finished) = this.transcript.finished {
                        cx.emit(finished);
                    }
                    cx.notify();
                    this.transcript.finished.is_some()
                });
                if !matches!(finished, Ok(false)) {
                    break;
                }
                // Coalesce noisy streams so input and rendering keep getting time.
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            }
        });
        Self {
            transcript: Transcript::default(),
            running_label: "Running…".into(),
            plain_stderr: false,
            save_name: None,
            follow: true,
            wrap: false,
            scroll: UniformListScrollHandle::new(),
            wrap_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            selection: None,
            dragging: false,
            menu: None,
            notice: None,
            _drain: drain,
        }
    }

    /// Status text while the stream is open and following (default "Running…").
    pub fn running_label(mut self, label: impl Into<SharedString>) -> Self {
        self.running_label = label.into();
        self
    }

    /// Show stderr like stdout (container logs) instead of grading it as an error.
    pub fn plain_stderr(mut self) -> Self {
        self.plain_stderr = true;
        self
    }

    /// Start of Save's suggested file name, e.g. the container name.
    pub fn save_name(mut self, name: impl Into<SharedString>) -> Self {
        self.save_name = Some(name.into());
        self
    }

    fn scroll_to_end(&self) {
        if self.wrap {
            self.wrap_scroll.scroll_to_bottom();
        } else {
            self.scroll.scroll_to_bottom();
        }
    }

    fn at_end(&self) -> bool {
        if self.wrap {
            let (offset, max) = (self.wrap_scroll.offset(), self.wrap_scroll.max_offset());
            -offset.y >= max.y - px(4.)
        } else {
            self.scroll.is_scrolled_to_end().unwrap_or(true)
        }
    }

    /// Scrolling up pauses following; scrolling down while at the end resumes it.
    fn on_scroll(&mut self, event: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        let dy = event.delta.pixel_delta(window.line_height()).y;
        let follow = if dy > px(0.) {
            false
        } else {
            self.follow || self.at_end()
        };
        if follow != self.follow {
            self.follow = follow;
            cx.notify();
        }
    }

    pub fn text(&self) -> String {
        self.transcript.text()
    }
    pub fn finished(&self) -> Option<LogFinished> {
        self.transcript.finished
    }

    /// Displayed text of absolute line `line`, if it's still held.
    fn display_text(&self, line: usize) -> Option<String> {
        let index = line.checked_sub(self.transcript.dropped)?;
        let line = self.transcript.lines.get(index)?;
        Some(display(line, self.plain_stderr).0)
    }

    fn selected_text(&self) -> Option<String> {
        let selection = self.selection.filter(|selection| !selection.is_empty())?;
        let first = selection.start().line;
        let texts: Vec<String> = (first..=selection.end().line)
            .filter_map(|line| self.display_text(line))
            .collect();
        Some(selection.text(|line| {
            line.checked_sub(first)
                .and_then(|index| texts.get(index))
                .map(String::as_str)
        }))
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let Some(last) = self.transcript.lines.len().checked_sub(1) else {
            return;
        };
        let last = self.transcript.dropped + last;
        let end = self.display_text(last).map_or(0, |text| text.len());
        self.selection = Some(Selection::new(
            Pos::new(self.transcript.dropped, 0),
            Pos::new(last, end),
        ));
        self.menu = None;
        cx.notify();
    }

    fn copy_selection(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        self.menu = None;
        cx.notify();
    }

    /// Esc clears the selection or closes the menu; otherwise it reaches the page (Back).
    fn clear_selection(&mut self, _: &ClearSelection, _: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_none() && self.selection.is_none() {
            cx.propagate();
            return;
        }
        self.menu = None;
        self.selection = None;
        cx.notify();
    }

    fn mouse_down(
        &mut self,
        pos: Pos,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.menu = None;
        let text = self.display_text(pos.line).unwrap_or_default();
        self.selection = Some(match event.click_count {
            2 => {
                let word = word_at(&text, pos.col);
                Selection::new(Pos::new(pos.line, word.start), Pos::new(pos.line, word.end))
            }
            n if n >= 3 => Selection::new(Pos::new(pos.line, 0), Pos::new(pos.line, text.len())),
            _ => match self.selection {
                Some(selection) if event.modifiers.shift => Selection::new(selection.anchor, pos),
                _ => Selection::new(pos, pos),
            },
        });
        self.dragging = event.click_count < 2;
        if self
            .selection
            .is_some_and(|selection| !selection.is_empty())
        {
            self.follow = false;
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn drag_to(&mut self, pos: Pos, cx: &mut Context<Self>) {
        let Some(selection) = &mut self.selection else {
            return;
        };
        if selection.head != pos {
            selection.head = pos;
            self.follow = false;
            cx.notify();
        }
    }

    fn mouse_up(&mut self, cx: &mut Context<Self>) {
        self.dragging = false;
        if self.selection.is_some_and(|selection| selection.is_empty()) {
            self.selection = None;
            cx.notify();
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let text = self.text();
        let base = self.save_name.clone();
        self.menu = None;
        cx.spawn(async move |this, cx| {
            let stamp = cx.background_executor().spawn(async { timestamp() }).await;
            let name = file_name(base.as_deref(), &stamp);
            let dir = save_dir();
            let receiver = cx.update(|cx| cx.prompt_for_new_path(&dir, Some(&name)));
            let path = match receiver.await {
                Ok(Ok(Some(path))) => path,
                Ok(Err(error)) => {
                    this.update(cx, |this, cx| {
                        this.notice = Some((format!("Couldn't save: {error}").into(), false));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                _ => return,
            };
            let written = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { std::fs::write(&written, text + "\n") })
                .await;
            this.update(cx, |this, cx| {
                let file = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                this.notice = Some(match result {
                    Ok(()) => (format!("Saved {file}").into(), true),
                    Err(error) => (format!("Couldn't save: {error}").into(), false),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// One log line: highlighted where selected, and a drag target for selection.
    fn render_line(
        &self,
        line: usize,
        text: String,
        color: Hsla,
        selection_bg: Hsla,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let highlight = self
            .selection
            .and_then(|selection| selection.range_in_line(line, &text))
            .map(|range| {
                (
                    range,
                    HighlightStyle {
                        background_color: Some(selection_bg),
                        ..Default::default()
                    },
                )
            });
        let text = StyledText::new(text).with_highlights(highlight);
        let layout = text.layout().clone();
        let down_layout = layout.clone();
        div()
            .id(line)
            .px_3()
            .text_color(color)
            .child(text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let pos = Pos::new(line, hit(&down_layout, event.position));
                    this.mouse_down(pos, event, window, cx);
                }),
            )
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if this.dragging && event.pressed_button == Some(MouseButton::Left) {
                    this.drag_to(Pos::new(line, hit(&layout, event.position)), cx);
                }
            }))
    }

    fn render_menu(
        &self,
        position: Point<Pixels>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let has_selection = self
            .selection
            .is_some_and(|selection| !selection.is_empty());
        let has_lines = !self.transcript.lines.is_empty();
        let item = |id: &'static str, label: &'static str, enabled: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .h(px(26.))
                .px_2()
                .mx_1()
                .rounded_sm()
                .text_sm()
                .text_color(theme.text)
                .when(!enabled, |el| el.opacity(0.5))
                .when(enabled, |el| {
                    el.cursor_pointer().hover(|s| s.bg(theme.element_hover))
                })
                .child(label)
        };
        deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(8.))
                .child(
                    div()
                        .id("log-menu")
                        .occlude()
                        .min_w(px(160.))
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.elevated_surface_background)
                        .shadow_md()
                        .font_family(crate::theme::UI_FONT)
                        .child(item("log-menu-copy", "Copy", has_selection).when(
                            has_selection,
                            |el| {
                                el.on_click(cx.listener(|this, _, window, cx| {
                                    this.copy_selection(&CopySelection, window, cx)
                                }))
                            },
                        ))
                        .child(item("log-menu-all", "Select all", has_lines).when(
                            has_lines,
                            |el| {
                                el.on_click(cx.listener(|this, _, window, cx| {
                                    this.select_all(&SelectAll, window, cx)
                                }))
                            },
                        ))
                        .child(
                            item("log-menu-save", "Save…", has_lines).when(has_lines, |el| {
                                el.on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                            }),
                        )
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.menu = None;
                            cx.notify();
                        })),
                ),
        )
        .priority(3)
    }
}

impl Render for LogPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (label, color) = match self.transcript.finished {
            None if self.follow => (self.running_label.to_string(), theme.text_muted),
            None => ("Paused".to_owned(), theme.text_muted),
            Some(LogFinished { exit_code: Some(0) }) => {
                ("Finished · exit 0".to_owned(), theme.success)
            }
            Some(LogFinished {
                exit_code: Some(code),
            }) => (format!("Failed · exit {code}"), theme.error),
            Some(LogFinished { exit_code: None }) => {
                ("Log stream ended".to_owned(), theme.text_muted)
            }
        };
        let toggle_style = |on: bool| {
            if on {
                ButtonStyle::Selected
            } else {
                ButtonStyle::Ghost
            }
        };
        let plain_stderr = self.plain_stderr;
        let line_color = move |severity: Severity, theme: &Theme| match severity {
            Severity::Normal => theme.text,
            Severity::Warning => theme.warning,
            Severity::Error => theme.error,
            Severity::Success => theme.success,
        };
        let selection_bg = theme.accent.opacity(0.3);
        let empty = self.transcript.lines.is_empty();

        let toolbar = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap_2()
            .h_8()
            .px_2()
            .border_b_1()
            .border_color(theme.border_variant)
            .bg(theme.surface_background)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .text_xs()
                    .text_color(color)
                    .child(label)
                    .child(
                        div()
                            .text_color(theme.text_muted)
                            .child(format!("{} lines", self.transcript.lines.len())),
                    )
                    .when_some(self.notice.clone(), |el, (notice, ok)| {
                        el.child(
                            div()
                                .truncate()
                                .text_color(if ok { theme.success } else { theme.error })
                                .child(notice),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new(
                            "log-follow",
                            if self.follow { "Following" } else { "Paused" },
                        )
                        .icon(if self.follow {
                            IconName::Pause
                        } else {
                            IconName::Play
                        })
                        .style(toggle_style(self.follow))
                        .tooltip(if self.follow {
                            "Pause auto-scroll"
                        } else {
                            "Follow the newest output"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.follow = !this.follow;
                            if this.follow {
                                this.scroll_to_end();
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        Button::new("log-wrap", "Wrap")
                            .style(toggle_style(self.wrap))
                            .tooltip("Wrap long lines")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.wrap = !this.wrap;
                                if this.follow {
                                    this.scroll_to_end();
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("log-copy", "Copy")
                            .icon(IconName::Copy)
                            .tooltip("Copy the whole log")
                            .disabled(empty)
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(this.text()))
                            })),
                    )
                    .child(
                        Button::new("log-save", "Save…")
                            .icon(IconName::Download)
                            .tooltip("Save the log to a file")
                            .disabled(empty)
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            );

        let body = div()
            .id("log-body")
            .flex_1()
            .min_h_0()
            .py_1()
            .bg(theme.terminal_background)
            .font_family(MONO_FONT)
            .text_xs()
            .cursor(CursorStyle::IBeam)
            .on_scroll(cx.listener(Self::on_scroll))
            // Lines stop propagation, so this is a click on empty space below them.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus, cx);
                    this.menu = None;
                    this.selection = None;
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.menu = Some(event.position);
                    cx.notify();
                }),
            );
        let first_line = self.transcript.dropped;
        let body = if empty {
            body.child(
                div()
                    .px_3()
                    .text_color(theme.text_muted)
                    .child("Waiting for output…"),
            )
        } else if self.wrap {
            let start = self
                .transcript
                .lines
                .len()
                .saturating_sub(MAX_WRAPPED_LINES);
            let lines = self.transcript.lines[start..]
                .iter()
                .enumerate()
                .map(|(offset, line)| {
                    let (text, severity) = display(line, plain_stderr);
                    self.render_line(
                        first_line + start + offset,
                        text,
                        line_color(severity, &theme),
                        selection_bg,
                        cx,
                    )
                    .py(px(1.))
                })
                .collect::<Vec<_>>();
            body.child(
                div()
                    .id("log-wrapped")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.wrap_scroll)
                    .children(lines),
            )
        } else {
            body.child(
                uniform_list(
                    "log-lines",
                    self.transcript.lines.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        let theme = cx.theme().clone();
                        range
                            .map(|index| {
                                let (text, severity) =
                                    display(&this.transcript.lines[index], plain_stderr);
                                this.render_line(
                                    this.transcript.dropped + index,
                                    text,
                                    line_color(severity, &theme),
                                    selection_bg,
                                    cx,
                                )
                                .h(px(20.))
                                .flex()
                                .items_center()
                                .whitespace_nowrap()
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .track_scroll(&self.scroll)
                .size_full(),
            )
        };

        div()
            .track_focus(&self.focus)
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::clear_selection))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.mouse_up(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.mouse_up(cx)),
            )
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .child(toolbar)
            .child(body)
            .when_some(self.menu, |el, position| {
                el.child(self.render_menu(position, &theme, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn preserves_stream_order_and_terminal_status_in_copy() {
        let mut transcript = Transcript::default();
        transcript.append(
            vec![
                LogLine::Stdout("booting".into()),
                LogLine::Stderr("failed".into()),
                LogLine::Exit(7),
            ],
            true,
        );
        assert_eq!(
            transcript.finished,
            Some(LogFinished { exit_code: Some(7) })
        );
        assert_eq!(
            transcript.text(),
            "booting\nfailed\nProcess exited with code 7."
        );
        let mut success = Transcript::default();
        success.append(vec![LogLine::Exit(0)], false);
        assert_eq!(success.finished, Some(LogFinished { exit_code: Some(0) }));
    }

    #[test]
    fn grades_colima_logfmt_by_level() {
        let info = LogLine::Stderr(
            r#"time="2026-10-24T18:41:07+02:00" level=info msg="starting ..." context=docker"#
                .into(),
        );
        assert_eq!(
            display(&info, false),
            ("starting ...".into(), Severity::Normal)
        );
        let warn = LogLine::Stderr(r#"time="t" level=warning msg="disk \"x\" low""#.into());
        assert_eq!(
            display(&warn, false),
            (r#"disk "x" low"#.into(), Severity::Warning)
        );
        let fatal = LogLine::Stderr(r#"time="t" level=fatal msg=boom"#.into());
        assert_eq!(display(&fatal, false), ("boom".into(), Severity::Error));
        let plain = LogLine::Stderr("unexpected".into());
        assert_eq!(
            display(&plain, false),
            ("unexpected".into(), Severity::Error)
        );
    }

    #[test]
    fn plain_stderr_is_not_an_error() {
        let line = LogLine::Stderr("GET /health 200".into());
        assert_eq!(
            display(&line, true),
            ("GET /health 200".into(), Severity::Normal)
        );
        assert_eq!(display(&line, false).1, Severity::Error);
    }

    #[test]
    fn transcript_keeps_only_the_newest_lines() {
        let mut transcript = Transcript::default();
        let lines = (0..MAX_LINES + 5)
            .map(|i| LogLine::Stdout(i.to_string()))
            .collect();
        transcript.append(lines, false);
        assert_eq!(transcript.lines.len(), MAX_LINES);
        assert_eq!(transcript.lines[0], LogLine::Stdout("5".into()));
        assert_eq!(transcript.dropped, 5);
    }

    #[test]
    fn missing_exit_is_reported_without_fabricating_a_code() {
        let mut transcript = Transcript::default();
        transcript.append(vec![LogLine::Stdout("partial".into())], true);
        assert_eq!(transcript.finished, Some(LogFinished { exit_code: None }));
        assert!(transcript.text().contains("without an exit status"));
    }

    #[test]
    fn batches_are_bounded_and_drain_before_disconnect() {
        let (tx, rx) = channel();
        for index in 0..BATCH_SIZE + 1 {
            tx.send(LogLine::Stdout(index.to_string())).unwrap();
        }
        tx.send(LogLine::Exit(0)).unwrap();
        drop(tx);
        let (first, closed) = receive_batch(&rx);
        assert_eq!(first.len(), BATCH_SIZE);
        assert!(!closed);
        let (last, closed) = receive_batch(&rx);
        assert_eq!(
            last,
            vec![LogLine::Stdout(BATCH_SIZE.to_string()), LogLine::Exit(0)]
        );
        assert!(closed);
    }

    #[test]
    fn file_names_carry_the_container_name() {
        assert_eq!(
            file_name(Some("demo-ministack"), "2026-10-25-1023"),
            "demo-ministack-log-2026-10-25-1023.log"
        );
        assert_eq!(file_name(Some("/web: api"), "s"), "web--api-log-s.log");
        assert_eq!(file_name(None, "s"), "log-s.log");
        assert_eq!(file_name(Some(".."), "s"), "log-s.log");
    }

    #[test]
    fn timestamps_suit_file_names() {
        let stamp = timestamp();
        assert!(
            stamp == "log"
                || (stamp.len() == 15 && stamp.chars().all(|c| c.is_ascii_digit() || c == '-')),
            "{stamp}"
        );
    }
}
