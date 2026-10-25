//! Docker volumes for one profile. Dropping the tab cancels its poller.
use crate::ui::ErrorNote;
use std::rc::Rc;

use gpui::{
    Context, Entity, EventEmitter, FocusHandle, SharedString, Subscription, Task, Window, div,
    prelude::*, px,
};

use super::navigate::Navigate;
use super::volume_detail;
use crate::docker::volumes::{self, Volume};
use crate::theme::{ActiveTheme, CHART_VOLUMES, MONO_FONT};
use crate::ui::dialog::{ConfirmDialog, DialogResult};
use crate::ui::{Button, ButtonStyle, Column, DetailPage, SizeBar, StatusDot, Table, TextCell};

#[derive(Clone, PartialEq, Eq)]
enum Action {
    Remove(String),
    Prune,
}

pub struct VolumesView {
    socket: String,
    volumes: Vec<Volume>,
    detail: Option<String>,
    detail_focus: FocusHandle,
    details: Option<volumes::VolumeDetails>,
    detail_error: Option<SharedString>,
    /// Cross-link target to open once the list has loaded (see `navigate`).
    pending_target: Option<String>,
    /// Focus the detail page on the next render; set where no `Window` is at hand.
    focus_detail: bool,
    loaded: bool,
    refreshing: bool,
    refresh_again: bool,
    busy: Option<Action>,
    list_error: Option<SharedString>,
    error: Option<SharedString>,
    notice: Option<SharedString>,
    confirm: Option<(Entity<ConfirmDialog>, Subscription)>,
    _poll: Task<()>,
}

impl VolumesView {
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
            socket,
            volumes: Vec::new(),
            detail: None,
            detail_focus: cx.focus_handle(),
            details: None,
            detail_error: None,
            pending_target: None,
            focus_detail: false,
            loaded: false,
            refreshing: false,
            refresh_again: false,
            busy: None,
            list_error: None,
            error: None,
            notice: None,
            confirm: None,
            _poll: poll,
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        // Keep a confirmation's estimate stable; Docker rechecks references on action.
        if self.confirm.is_some() {
            return;
        }
        if self.refreshing || self.busy.is_some() {
            self.refresh_again = true;
            return;
        }
        self.refreshing = true;
        let socket = self.socket.clone();
        let detail = self.detail.clone();
        let inspected = detail.clone();
        let task = cx.background_executor().spawn(async move {
            let listed = volumes::list(&socket);
            let metadata = inspected.map(|name| volumes::inspect(&socket, &name));
            (listed, metadata)
        });
        cx.spawn(async move |this, cx| {
            let (result, metadata) = task.await;
            this.update(cx, |this, cx| {
                this.refreshing = false;
                this.loaded = true;
                match result {
                    Ok(volumes) => {
                        this.volumes = volumes;
                        this.list_error = None;
                    }
                    Err(error) => this.list_error = Some(error.to_string().into()),
                }
                this.resolve_target(cx);
                if this.detail == detail {
                    match metadata {
                        Some(Ok(details)) => {
                            this.details = Some(details);
                            this.detail_error = None;
                        }
                        Some(Err(error)) => {
                            this.details = None;
                            this.detail_error = Some(error.to_string().into());
                        }
                        None => {}
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

    /// Opens a volume's detail page from a cross-link, once the list has it.
    pub fn open_target(&mut self, name: String, cx: &mut Context<Self>) {
        self.pending_target = Some(name);
        self.resolve_target(cx);
    }

    fn resolve_target(&mut self, cx: &mut Context<Self>) {
        if !self.loaded {
            return;
        }
        let Some(name) = self.pending_target.take() else {
            return;
        };
        if self.volumes.iter().any(|volume| volume.name == name) {
            self.show_detail(name, cx);
            self.focus_detail = true;
        } else {
            self.error = Some(format!("Volume “{name}” no longer exists.").into());
        }
        cx.notify();
    }

    fn show_detail(&mut self, name: String, cx: &mut Context<Self>) {
        self.detail = Some(name);
        self.details = None;
        self.detail_error = None;
        self.refresh(cx);
        cx.notify();
    }

    fn run(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.refreshing || self.list_error.is_some() {
            return;
        }
        if let Action::Remove(name) = &action
            && !self
                .volumes
                .iter()
                .any(|volume| &volume.name == name && volume.unused())
        {
            self.error = Some("Volume is no longer unused. Refresh and try again.".into());
            cx.notify();
            return;
        }
        self.busy = Some(action.clone());
        self.error = None;
        self.notice = None;
        let socket = self.socket.clone();
        let completed_action = action.clone();
        let task = cx.background_executor().spawn(async move {
            match &action {
                Action::Remove(name) => {
                    volumes::remove(&socket, name).map(|_| format!("Removed volume “{name}”."))
                }
                Action::Prune => volumes::prune(&socket).map(|output| {
                    output
                        .lines()
                        .rev()
                        .find(|line| line.starts_with("Total reclaimed space:"))
                        .unwrap_or("Volume prune completed.")
                        .to_owned()
                }),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(message) => {
                        if let Action::Remove(name) = &completed_action
                            && this.detail.as_ref() == Some(name)
                        {
                            this.detail = None;
                            this.details = None;
                        }
                        this.notice = Some(message.into());
                    }
                    Err(error) => this.error = Some(error.to_string().into()),
                }
                this.refresh_again = false;
                this.refresh(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn confirm_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some()
            || self.refreshing
            || self.list_error.is_some()
            || self.confirm.is_some()
        {
            return;
        }
        let (title, message, label) = match &action {
            Action::Remove(name) => {
                let Some(volume) = self
                    .volumes
                    .iter()
                    .find(|volume| &volume.name == name && volume.unused())
                else {
                    return;
                };
                let size = volume
                    .size_bytes
                    .map(volumes::format_size)
                    .unwrap_or_else(|| "size unavailable".into());
                (
                    format!("Remove volume “{name}”?"),
                    format!(
                        "Permanently delete this volume and its data ({size}). This cannot be undone."
                    ),
                    "Remove",
                )
            }
            Action::Prune => {
                let unused: Vec<_> = self
                    .volumes
                    .iter()
                    .filter(|volume| volume.unused())
                    .collect();
                if unused.is_empty() {
                    return;
                }
                let bytes = unused
                    .iter()
                    .filter_map(|volume| volume.size_bytes)
                    .fold(0_u64, u64::saturating_add);
                let unknown = unused
                    .iter()
                    .filter(|volume| volume.size_bytes.is_none())
                    .count();
                let size = if unknown == 0 {
                    format!("approximately {}", volumes::format_size(bytes))
                } else {
                    format!(
                        "at least {}; {unknown} size(s) unavailable",
                        volumes::format_size(bytes)
                    )
                };
                (
                    "Prune unused volumes?".into(),
                    format!(
                        "Permanently delete all unused named and anonymous volumes in this profile. Currently {} unused volumes, {size}. Volumes referenced by running or stopped containers are kept. The set may change before confirmation. This cannot be undone.",
                        unused.len()
                    ),
                    "Prune",
                )
            }
        };
        let dialog = cx.new(|cx| ConfirmDialog::new(title, message, cx).confirm_label(label));
        let subscription = cx.subscribe(&dialog, move |this, _, result, cx| {
            this.confirm = None;
            if *result == DialogResult::Confirmed {
                this.run(action.clone(), cx);
            }
            cx.notify();
        });
        dialog.update(cx, |dialog, cx| dialog.show(window, cx));
        self.confirm = Some((dialog, subscription));
        cx.notify();
    }
}

impl EventEmitter<Navigate> for VolumesView {}

impl Render for VolumesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.focus_detail) {
            window.focus(&self.detail_focus, cx);
        }
        let theme = cx.theme().clone();
        let unused = self.volumes.iter().filter(|volume| volume.unused()).count();
        let detail_page = self.detail.clone().map(|name| {
            let volume = self.volumes.iter().find(|volume| volume.name == name);
            let removable = volume.is_some_and(Volume::unused);
            let remove_tip = volume
                .map(|volume| {
                    if removable {
                        "Remove this unused volume".into()
                    } else {
                        format!("Cannot remove: {}", volume.usage_description())
                    }
                })
                .unwrap_or_else(|| "Volume no longer exists".into());
            let action = Action::Remove(name.clone());
            DetailPage::new(name.clone())
                .status(StatusDot::for_volume("volume-detail-status", !removable))
                .subtitle(
                    volume
                        .map(|volume| {
                            format!(
                                "{} · {}",
                                volume
                                    .size_bytes
                                    .map(volumes::format_size)
                                    .unwrap_or_else(|| "Size unavailable".into()),
                                volume.driver
                            )
                        })
                        .unwrap_or_default(),
                )
                .action(
                    Button::new("remove-volume", "Remove")
                        .style(ButtonStyle::Danger)
                        .tooltip(remove_tip)
                        .disabled(
                            !removable
                                || self.busy.is_some()
                                || self.refreshing
                                || self.list_error.is_some(),
                        )
                        .loading(self.busy.as_ref() == Some(&action))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.confirm_action(action.clone(), window, cx)
                        })),
                )
                .track_focus(&self.detail_focus)
                .on_back({
                    let this = cx.entity().downgrade();
                    move |_, cx| {
                        this.update(cx, |this, cx| {
                            this.detail = None;
                            this.details = None;
                            this.detail_error = None;
                            cx.notify();
                        })
                        .ok();
                    }
                })
                .body(volume_detail::body(
                    volume,
                    self.details.as_ref(),
                    self.detail_error.clone(),
                    cx,
                ))
        });
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .text_sm()
            .text_color(theme.text)
            .bg(theme.editor_background)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .h_8()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border_variant)
                    .child(format!("{} volumes · {unused} unused", self.volumes.len()))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new("prune-volumes", "Prune unused volumes")
                                    .style(ButtonStyle::Danger)
                                    .loading(self.busy == Some(Action::Prune))
                                    .disabled(
                                        self.busy.is_some()
                                            || self.refreshing
                                            || self.list_error.is_some()
                                            || unused == 0
                                            || self
                                                .volumes
                                                .iter()
                                                .any(|volume| volume.links.is_none()),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_action(Action::Prune, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("refresh-volumes", "Refresh")
                                    .loading(self.refreshing)
                                    .disabled(self.busy.is_some())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.error = None;
                                        this.refresh(cx);
                                    })),
                            ),
                    ),
            )
            .when_some(
                self.error.clone().or(self.list_error.clone()),
                |el, error| el.child(div().px_2().py_1().child(ErrorNote::new("volumes-error", error))),
            )
            .when_some(self.notice.clone(), |el, notice| {
                el.child(div().px_2().py_1().text_color(theme.success).child(notice))
            })
            .when_some(detail_page, |el, page| {
                el.child(div().flex_1().min_h_0().child(page))
            })
            .when(self.detail.is_none(), |el| {
                el.child(
                    div()
                        .id("volume-table")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .child({
                            let rows = Rc::new(self.volumes.clone());
                            let clicked = rows.clone();
                            let max = rows
                                .iter()
                                .filter_map(|volume| volume.size_bytes)
                                .max()
                                .unwrap_or(0);
                            let this = cx.entity().downgrade();
                            Table::new(
                                "volumes",
                                vec![
                                    Column::fixed("", px(16.)),
                                    Column::flex("Name", 1.),
                                    Column::fixed("Size", px(140.)).right().info(
                                        "Estimated by Docker; bars are scaled to the largest volume.",
                                    ),
                                ],
                                rows.len(),
                                move |index, _, cx| {
                                    let volume = &rows[index];
                                    vec![
                                        StatusDot::for_volume(
                                            ("volume-status", index),
                                            !volume.unused(),
                                        )
                                        .into_any_element(),
                                        div()
                                            .id(("volume-name", index))
                                            .min_w_0()
                                            .truncate()
                                            .font_family(MONO_FONT)
                                            .text_xs()
                                            .child(volume.display_name())
                                            .tooltip(crate::ui::tooltip::tooltip(
                                                volume.name.clone(),
                                            ))
                                            .into_any_element(),
                                        div()
                                            .flex()
                                            .flex_col()
                                            .items_end()
                                            .gap_1()
                                            .w_full()
                                            .child(
                                                TextCell::new(
                                                    ("volume-size", index),
                                                    volume
                                                        .size_bytes
                                                        .map(volumes::format_size)
                                                        .unwrap_or_else(|| "Unavailable".into()),
                                                )
                                                .mono(),
                                            )
                                            .when_some(volume.size_bytes, |el, bytes| {
                                                el.child(
                                                    SizeBar::new(bytes as f64, max as f64)
                                                        .color(cx.theme().chart[CHART_VOLUMES]),
                                                )
                                            })
                                            .into_any_element(),
                                    ]
                                },
                            )
                            .on_row_click(move |index, window, cx| {
                                this.update(cx, |this, cx| {
                                    this.show_detail(clicked[index].name.clone(), cx);
                                    window.focus(&this.detail_focus, cx);
                                })
                                .ok();
                            })
                            .empty(
                                div().p_4().text_color(theme.text_muted).child(
                                    if self.list_error.is_some() {
                                        "Could not load volumes. Refresh to retry."
                                    } else if self.loaded {
                                        "No volumes in this profile."
                                    } else {
                                        "Loading volumes…"
                                    },
                                ),
                            )
                        }),
                )
            })
            .when_some(self.confirm.as_ref(), |el, (dialog, _)| {
                el.child(dialog.clone())
            })
    }
}
