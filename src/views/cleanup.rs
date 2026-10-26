//! Disk usage and explicitly confirmed cleanup for one visible Docker profile.
use crate::ui::ErrorNote;
use gpui::{
    Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Subscription, Task,
    Window, anchored, container_query, deferred, div, point, prelude::*, px,
};

use crate::{
    colima::{cache, cli, commands},
    docker::system::{self, Prune, Usage, VmDisk, format_size},
    theme::{ActiveTheme, CHART_BUILD_CACHE, CHART_CONTAINERS, CHART_IMAGES, CHART_VOLUMES},
    ui::{
        Button, ButtonStyle, Icon, IconName, InfoTip, Meter, Segment, StackedBar,
        dialog::DialogResult, text_input::TextInput,
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Prune(Prune),
    Trim,
    ColimaCache,
}

pub struct CleanupView {
    socket: String,
    profile: Option<String>,
    usage: Vec<Usage>,
    disk: Option<VmDisk>,
    unused_volumes: Option<usize>,
    volumes_error: Option<SharedString>,
    cache_size: Option<u64>,
    cache_error: Option<SharedString>,
    loaded: bool,
    refreshing: bool,
    refresh_again: bool,
    busy: Option<Action>,
    list_error: Option<SharedString>,
    disk_error: Option<SharedString>,
    error: Option<SharedString>,
    result: Option<SharedString>,
    confirm: Option<(Entity<CleanupDialog>, Subscription)>,
    _poll: Task<()>,
}

impl CleanupView {
    pub fn new(socket: String, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let Ok(interval) = this.update(cx, |this, cx| {
                    this.refresh(cx);
                    crate::store::poll_interval(cx)
                }) else {
                    break;
                };
                cx.background_executor().timer(interval).await;
            }
        });
        Self {
            profile: system::profile_from_socket(&socket),
            socket,
            usage: Vec::new(),
            disk: None,
            unused_volumes: None,
            volumes_error: None,
            cache_size: None,
            cache_error: None,
            loaded: false,
            refreshing: false,
            refresh_again: false,
            busy: None,
            list_error: None,
            disk_error: None,
            error: None,
            result: None,
            confirm: None,
            _poll: poll,
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        if self.refreshing {
            self.refresh_again = true;
            return;
        }
        self.refreshing = true;
        let socket = self.socket.clone();
        let profile = self.profile.clone();
        let task = cx.background_executor().spawn(async move {
            let usage = system::disk_usage(&socket);
            let disk = profile.map(|profile| commands::vm_disk(&profile));
            let volumes = unused_volumes(&socket);
            let cache = cache::size();
            (usage, disk, volumes, cache)
        });
        cx.spawn(async move |this, cx| {
            let (usage, disk, volumes, cache) = task.await;
            this.update(cx, |this, cx| {
                this.refreshing = false;
                this.loaded = true;
                match usage {
                    Ok(usage) => {
                        this.usage = usage;
                        this.list_error = None;
                    }
                    Err(error) => this.list_error = Some(error.to_string().into()),
                }
                this.unused_volumes = match volumes {
                    Ok(count) => {
                        this.volumes_error = None;
                        Some(count)
                    }
                    Err(error) => {
                        this.volumes_error = Some(error.to_string().into());
                        None
                    }
                };
                this.cache_size = match cache {
                    Ok(size) => {
                        this.cache_error = None;
                        Some(size)
                    }
                    Err(error) => {
                        this.cache_error = Some(error.to_string().into());
                        None
                    }
                };
                match disk {
                    Some(Ok(disk)) => {
                        this.disk = Some(disk);
                        this.disk_error = None;
                    }
                    Some(Err(error)) => this.disk_error = Some(error.to_string().into()),
                    None => {
                        this.disk_error =
                            Some("VM disk unavailable: unrecognized Colima socket path.".into())
                    }
                }
                if std::mem::take(&mut this.refresh_again) {
                    this.refresh(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn confirm(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.confirm.is_some() {
            return;
        }
        let profile = self.profile.clone().unwrap_or_else(|| self.socket.clone());
        let (title, message, typed) = match action {
            Action::Prune(prune) => (
                prune.label().to_owned(),
                format!(
                    "Profile: {profile}. {} Estimated reclaimable: up to {}. Estimates may include shared data and can change before cleanup. {}",
                    prune.description(),
                    format_size(prune.estimate(&self.usage)),
                    if crate::settings::get(cx).trim_after_prune {
                        "After pruning, fstrim will return unused disk blocks to this computer."
                    } else {
                        "Automatic return of free space to this computer is disabled in Settings."
                    }
                ),
                (prune == Prune::Everything).then_some(profile),
            ),
            Action::ColimaCache => (
                "Prune Colima download cache".into(),
                format!(
                    "Removes cached downloads shared by all Colima profiles on this computer ({}). Profiles and VM data are kept. Future starts may download these assets again. Estimated size: {}.",
                    cache::cache_home()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "path unavailable".into()),
                    self.cache_size
                        .map(format_size)
                        .unwrap_or_else(|| "unavailable".into())
                ),
                None,
            ),
            Action::Trim => (
                "Return free space".into(),
                format!(
                    "Profile: {profile}. Can return approximately {}. Nothing is deleted: fstrim releases unused VM filesystem blocks from the host disk file. Actual returned space depends on the host filesystem.",
                    self.disk
                        .as_ref()
                        .and_then(VmDisk::returnable)
                        .map(format_size)
                        .unwrap_or_else(|| "an unknown amount".into())
                ),
                None,
            ),
        };
        let dialog = cx.new(|cx| CleanupDialog::new(title, message, typed, cx));
        let subscription = cx.subscribe(&dialog, move |this, _, result, cx| {
            this.confirm = None;
            if *result == DialogResult::Confirmed {
                this.run(action, cx);
            }
            cx.notify();
        });
        dialog.update(cx, |dialog, cx| dialog.show(window, cx));
        self.confirm = Some((dialog, subscription));
        cx.notify();
    }

    fn run(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(action);
        self.error = None;
        self.result = None;
        let socket = self.socket.clone();
        let profile = self.profile.clone();
        let auto_trim = crate::settings::get(cx).trim_after_prune;
        let task = cx.background_executor().spawn(async move {
            match action {
                Action::Prune(prune) => {
                    // Measure before pruning: Docker may discard blocks itself.
                    let before = if auto_trim {
                        profile
                            .as_deref()
                            .and_then(|profile| commands::vm_disk(profile).ok())
                            .and_then(|disk| disk.allocated)
                    } else {
                        None
                    };
                    let bytes = system::prune(&socket, prune)?;
                    let freed = format!("Freed {} in Docker", format_size(bytes));
                    Ok(if auto_trim {
                        format!("{freed}. {}", return_space(profile.as_deref(), before))
                    } else {
                        freed
                    })
                }
                Action::ColimaCache => cache::prune()
                    .map(|bytes| format!("Freed {} in Colima download cache", format_size(bytes))),
                Action::Trim => {
                    let before = profile
                        .as_deref()
                        .and_then(|profile| commands::vm_disk(profile).ok())
                        .and_then(|disk| disk.allocated);
                    Ok(return_space(profile.as_deref(), before))
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result: Result<String, crate::colima::cli::CliError> = task.await;
            this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(result) => this.result = Some(result.into()),
                    Err(error) => this.error = Some(error.to_string().into()),
                }
                this.refresh(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

// Docker's summary only exposes total volume count. Query unused volumes separately,
// so empty unused volumes remain actionable without enabling pruning of in-use ones.
fn unused_volumes(socket: &str) -> Result<usize, cli::CliError> {
    let output = cli::run(
        "docker",
        &[
            "--host",
            socket,
            "volume",
            "ls",
            "--filter",
            "dangling=true",
            "--format",
            "json",
        ],
    )?;
    serde_json::Deserializer::from_slice(&output.stdout)
        .into_iter::<serde_json::Value>()
        .try_fold(0, |count, volume| volume.map(|_| count + 1))
        .map_err(|error| cli::CliError::Parse(error.to_string()))
}

fn prune_available(prune: Prune, usage: &[Usage], unused_volumes: Option<usize>) -> bool {
    if prune == Prune::Volumes {
        unused_volumes.is_some_and(|count| count > 0)
    } else {
        prune.estimate(usage) > 0
    }
}

// Blocking helper, called only on the background executor. Keep successful prune
// output visible even if trim or the subsequent host measurement fails.
fn return_space(profile: Option<&str>, before: Option<u64>) -> String {
    let Some(profile) = profile else {
        return "Could not return space: unknown Colima profile.".into();
    };
    if let Err(error) = commands::trim(profile) {
        return format!("Could not return space to this computer: {error}");
    }
    match commands::vm_disk(profile) {
        Ok(after) => match (before, after.allocated) {
            (Some(before), Some(after)) => format!(
                "Returned {} to this computer",
                format_size(before.saturating_sub(after))
            ),
            _ => {
                "Trim complete; returned space unavailable (host allocation could not be measured)."
                    .into()
            }
        },
        Err(error) => format!("Trim complete; could not measure returned space: {error}"),
    }
}

const CATEGORIES: [(&str, &str, &str, usize); 4] = [
    (
        "Images",
        "Images",
        "Images: layers stored for running or future containers. Reclaimable = not used by any container.",
        CHART_IMAGES,
    ),
    (
        "Containers",
        "Containers",
        "Containers: writable container layers. Reclaimable = stopped containers.",
        CHART_CONTAINERS,
    ),
    (
        "Volumes",
        "Local Volumes",
        "Volumes: persistent container data. Reclaimable = not referenced by any container.",
        CHART_VOLUMES,
    ),
    (
        "Build cache",
        "Build Cache",
        "Build cache: saved build steps. Reclaimable = unused cache; future builds may take longer.",
        CHART_BUILD_CACHE,
    ),
];

impl CleanupView {
    fn action_tile(
        &self,
        index: usize,
        action: Action,
        label: &'static str,
        icon: IconName,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let t = cx.theme().clone();
        let (estimate, detail, available) = match action {
            Action::Prune(prune) => {
                let estimate = prune.estimate(&self.usage);
                (
                    format!("~{}", format_size(estimate)),
                    prune.description().to_owned(),
                    prune_available(prune, &self.usage, self.unused_volumes)
                        && self.list_error.is_none()
                        && (prune != Prune::Everything || self.profile.is_some()),
                )
            }
            Action::ColimaCache => (
                self.cache_size.map(format_size).unwrap_or_else(|| "Size unavailable".into()),
                "Cached downloads shared by all Colima profiles on this computer. Pruning keeps profiles and VM data; future starts may download these assets again.".into(),
                self.cache_size.is_some_and(|bytes| bytes > 0) && self.cache_error.is_none(),
            ),
            Action::Trim => {
                let estimate = self.disk.as_ref().and_then(VmDisk::returnable);
                (estimate.map(|bytes| format!("~{}", format_size(bytes))).unwrap_or_else(|| "Estimate unavailable".into()),
                    "Nothing is deleted. Return unused VM disk blocks to this computer with fstrim. The estimate is host file allocation minus data-disk used bytes.".into(),
                    estimate.is_some_and(|bytes| bytes > 0) && self.disk_error.is_none() && self.profile.is_some())
            }
        };
        let danger = action == Action::Prune(Prune::Everything);
        let enabled = available && self.busy.is_none() && self.confirm.is_none();
        let working = self.busy == Some(action);
        div()
            .id(("cleanup-action", index))
            .min_w_0()
            .p_2()
            .flex()
            .items_center()
            .gap_2()
            .bg(t.surface_background)
            .border_1()
            .border_color(t.border_variant)
            .rounded_sm()
            .text_color(if danger { t.error } else { t.text_muted })
            .tooltip(crate::ui::tooltip::tooltip(detail))
            .child(Icon::new(icon).color(if danger { t.error } else { t.text_muted }))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(div().child(label))
                    .child(div().text_xs().whitespace_nowrap().child(if working {
                        "Working…".into()
                    } else {
                        estimate
                    })),
            )
            .when(!enabled, |el| el.opacity(0.5))
            .when(enabled, |el| {
                el.focusable()
                    .cursor_pointer()
                    .focus_visible(move |s| s.border_color(t.border_focused))
                    .hover(move |s| {
                        s.bg(t.element_hover)
                            .text_color(if danger { t.error } else { t.text })
                    })
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.confirm(action, window, cx)),
                    )
                    .on_key_down(cx.listener(
                        move |this, event: &gpui::KeyDownEvent, window, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                this.confirm(action, window, cx);
                                cx.stop_propagation();
                            }
                        },
                    ))
            })
    }
}

impl Render for CleanupView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let segments: Vec<_> = CATEGORIES
            .iter()
            .map(|&(label, kind, _, color)| {
                let row = self.usage.iter().find(|row| row.kind == kind);
                Segment {
                    label: label.into(),
                    value_label: row
                        .map(|row| format_size(row.size))
                        .unwrap_or_else(|| "—".into())
                        .into(),
                    value: row.map_or(0., |row| row.size as f64),
                    reclaimable: row.map_or(0., |row| row.reclaimable as f64),
                    color: t.chart[color],
                }
            })
            .collect();
        let categories = div()
            .grid()
            .gap_2()
            .children(CATEGORIES.iter().enumerate().map(
                |(index, &(label, kind, description, _))| {
                    let usage = self.usage.iter().find(|usage| usage.kind == kind);
                    div()
                        .min_w_0()
                        .p_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .bg(t.surface_background)
                        .border_1()
                        .border_color(t.border_variant)
                        .rounded_sm()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(label)
                                .child(InfoTip::new(("cleanup-info", index), description)),
                        )
                        .child(
                            usage
                                .map(|usage| format_size(usage.size))
                                .unwrap_or_else(|| "—".into()),
                        )
                        .child(StackedBar::new(vec![segments[index].clone()]).without_legend())
                        .child(
                            div()
                                .text_xs()
                                .whitespace_nowrap()
                                .text_color(t.text_muted)
                                .child(
                                    usage
                                        .map(|usage| {
                                            format!(
                                                "{} reclaimable",
                                                format_size(usage.reclaimable)
                                            )
                                        })
                                        .unwrap_or_else(|| "— reclaimable".into()),
                                ),
                        )
                },
            ));
        let disk = div()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .bg(t.surface_background)
            .border_1()
            .border_color(t.border_variant)
            .rounded_sm()
            .child(
                div().flex().items_center().gap_1().child("Data disk")
                    .child(InfoTip::new("vm-disk-info", "Docker data filesystem usage, separate from the VM operating system disk. Host allocation measures the sparse disk file's allocated blocks.")),
            )
            .when_some(self.disk.as_ref(), |el, disk| {
                el.child(Meter::new(
                    "Used",
                    format!("{} / {}", format_size(disk.used), format_size(disk.size)),
                    disk.used as f64,
                    disk.size as f64,
                ))
            })
            .child(div().text_xs().text_color(t.text_muted).child(format!(
                "On this computer: {}",
                self.disk.as_ref().and_then(|disk| disk.allocated)
                    .map(format_size).unwrap_or_else(|| "unavailable".into()),
            )))
            .when_some(self.disk_error.clone(), |el, error| {
                el.child(div().text_xs().child(ErrorNote::new("cleanup-disk-error", error)))
            });
        let actions = div().grid().gap_2().children(
            [
                (
                    Action::Prune(Prune::Containers),
                    "Stopped containers",
                    IconName::Box,
                ),
                (
                    Action::Prune(Prune::DanglingImages),
                    "Dangling images",
                    IconName::Layers,
                ),
                (
                    Action::Prune(Prune::BuildCache),
                    "Build cache",
                    IconName::HardDrive,
                ),
                (
                    Action::Prune(Prune::Volumes),
                    "Unused volumes",
                    IconName::Database,
                ),
                (
                    Action::Prune(Prune::Images),
                    "All unused images",
                    IconName::Layers,
                ),
                (
                    Action::ColimaCache,
                    "Colima download cache",
                    IconName::HardDrive,
                ),
                (Action::Trim, "Return free space", IconName::HardDrive),
                (
                    Action::Prune(Prune::Everything),
                    "Remove everything",
                    IconName::Trash,
                ),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (action, label, icon))| self.action_tile(index, action, label, icon, cx)),
        );
        // Query the viewport, then scroll the content inside it. Query children
        // cannot determine their parent's height, so keep the content unconstrained.
        let content = div().w_full().flex_none().flex().flex_col().gap_2()
            .when_some(self.error.clone().or(self.list_error.clone()), |el, error| el.child(div().text_xs().child(ErrorNote::new("cleanup-error", error))))
            .when_some(self.volumes_error.clone(), |el, error| el.child(ErrorNote::new("cleanup-volumes-error", error)))
            .when_some(self.cache_error.clone(), |el, error| el.child(ErrorNote::new("cleanup-cache-error", error)))
            .when_some(self.result.clone(), |el, result| el.child(div().text_xs().text_color(t.success).child(result)))
            .when(!self.loaded, |el| el.child(div().text_color(t.text_muted).child("Loading disk usage…")))
            .when(self.disk.as_ref().is_some_and(VmDisk::nearly_full), |el| el.child(div().p_2().text_xs().text_color(t.warning).border_1().border_color(t.warning).rounded_sm()
                .child("Data disk is at least 90% full. Prune unused data or increase the disk size via Edit.")))
            .child(StackedBar::new(segments).without_legend());
        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .text_sm()
            .text_color(t.text)
            .bg(t.editor_background)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .h_8()
                    .px_2()
                    .border_b_1()
                    .border_color(t.border_variant)
                    .child("Disk usage and cleanup")
                    .child(
                        Button::new("refresh-cleanup", "Refresh")
                            .disabled(self.busy.is_some() || self.confirm.is_some())
                            .loading(self.refreshing)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
            .child(
                container_query(move |size, _, _| {
                    let columns = if size.width < px(720.) { 2 } else { 4 };
                    let action_columns = if size.width < px(500.) {
                        2
                    } else if size.width < px(900.) {
                        3
                    } else {
                        4
                    };
                    div()
                        .id("cleanup-scroll")
                        .size_full()
                        .overflow_y_scroll()
                        .p_2()
                        .child(
                            content
                                .child(categories.grid_cols(columns))
                                .child(disk)
                                .child(div().text_color(t.text_muted).child("Actions"))
                                .child(actions.grid_cols(action_columns)),
                        )
                })
                .flex_1()
                .min_h_0(),
            )
            .when_some(self.confirm.as_ref(), |el, (dialog, _)| {
                el.child(dialog.clone())
            })
    }
}

/// Local modal: the shared ConfirmDialog has no typed-confirmation slot.
struct CleanupDialog {
    title: String,
    message: String,
    required: Option<String>,
    input: Entity<TextInput>,
    cancel_focus: FocusHandle,
    confirm_focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    _input_subscription: Subscription,
}
impl EventEmitter<DialogResult> for CleanupDialog {}
impl CleanupDialog {
    fn new(
        title: String,
        message: String,
        required: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new(cx).placeholder("Profile name"));
        let subscription = cx.subscribe(&input, |_, _, _, cx| cx.notify());
        Self {
            title,
            message,
            required,
            input,
            cancel_focus: cx.focus_handle(),
            confirm_focus: cx.focus_handle(),
            previous_focus: None,
            _input_subscription: subscription,
        }
    }
    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.previous_focus = window.focused(cx);
        window.focus(&self.cancel_focus, cx);
    }
    fn finish(&mut self, result: DialogResult, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = self.previous_focus.take() {
            window.focus(&focus, cx);
        }
        cx.emit(result);
    }
}
impl Render for CleanupDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let viewport = window.viewport_size();
        let enabled = self
            .required
            .as_ref()
            .is_none_or(|required| self.input.read(cx).text() == required);
        div().child(
            deferred(
                anchored().position(point(px(0.), px(0.))).child(
                    div()
                        .id("cleanup-confirm-overlay")
                        .w(viewport.width)
                        .h(viewport.height)
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(t.editor_background.opacity(0.85))
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .capture_key_down(cx.listener(
                            |this, event: &gpui::KeyDownEvent, window, cx| {
                                match event.keystroke.key.as_str() {
                                    "escape" => this.finish(DialogResult::Cancelled, window, cx),
                                    "tab" => {
                                        // Cycle inside the modal before TextInput's Tab binding runs.
                                        let mut stops = vec![this.cancel_focus.clone()];
                                        if this.required.is_some() {
                                            stops.push(this.input.focus_handle(cx));
                                        }
                                        if this.required.as_ref().is_none_or(|required| {
                                            this.input.read(cx).text() == required
                                        }) {
                                            stops.push(this.confirm_focus.clone());
                                        }
                                        let current = stops
                                            .iter()
                                            .position(|focus| focus.is_focused(window))
                                            .unwrap_or(0);
                                        let next = if event.keystroke.modifiers.shift {
                                            (current + stops.len() - 1) % stops.len()
                                        } else {
                                            (current + 1) % stops.len()
                                        };
                                        window.focus(&stops[next], cx);
                                    }
                                    _ => return,
                                }
                                cx.stop_propagation();
                            },
                        ))
                        .child(
                            div()
                                .w(px(440.))
                                .max_w(viewport.width)
                                .p_3()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .rounded_sm()
                                .border_1()
                                .border_color(t.border)
                                .bg(t.elevated_surface_background)
                                .text_sm()
                                .text_color(t.text)
                                .child(self.title.clone())
                                .child(div().text_color(t.text_muted).child(self.message.clone()))
                                .when_some(self.required.clone(), |el, profile| {
                                    el.child(format!("Type “{profile}” to confirm:"))
                                        .child(self.input.clone())
                                })
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .gap_2()
                                        .child(
                                            Button::new("cancel-cleanup", "Cancel")
                                                .track_focus(&self.cancel_focus)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.finish(DialogResult::Cancelled, window, cx)
                                                })),
                                        )
                                        .child(
                                            Button::new("confirm-cleanup", "Confirm")
                                                .style(ButtonStyle::Danger)
                                                .track_focus(&self.confirm_focus)
                                                .disabled(!enabled)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    if this.required.as_ref().is_none_or(
                                                        |required| {
                                                            this.input.read(cx).text() == required
                                                        },
                                                    ) {
                                                        this.finish(
                                                            DialogResult::Confirmed,
                                                            window,
                                                            cx,
                                                        );
                                                    }
                                                })),
                                        ),
                                ),
                        ),
                ),
            )
            .priority(10),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_unused_volumes_are_actionable() {
        let usage = vec![Usage {
            kind: "Local Volumes".into(),
            total: 3,
            size: 0,
            reclaimable: 0,
        }];
        assert!(prune_available(Prune::Volumes, &usage, Some(2)));
        assert!(!prune_available(Prune::Volumes, &usage, Some(0)));
        assert!(!prune_available(Prune::Volumes, &usage, None));
        assert!(!prune_available(Prune::Images, &usage, Some(2)));
    }
}
