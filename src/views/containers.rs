//! Docker containers for one profile/socket. Dropping the tab stops its poller.

use gpui::{
    Context, Entity, EventEmitter, FocusHandle, SharedString, Subscription, Task, Window, div,
    prelude::*, px,
};

use std::rc::Rc;

use super::container_detail::{DetailState, DetailTab};
use super::navigate::{self, Navigate};
use crate::docker::{commands, model::Container, stats};
use crate::theme::ActiveTheme;
use crate::ui::dialog::{ConfirmDialog, DialogResult};
use crate::ui::{
    Button, ButtonStyle, Column, IconName, MenuButton, MenuItem, StatusDot, Table, TextCell,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Start,
    Stop,
    Restart,
    Remove,
}

pub struct ContainersView {
    pub(super) socket: String,
    pub(super) containers: Vec<Container>,
    pub(super) usage: Vec<stats::ContainerStats>,
    pub(super) usage_error: Option<SharedString>,
    loaded: bool,
    refreshing: bool,
    refresh_again: bool,
    pub(super) busy: Option<(String, Action)>,
    /// Last `docker ps` failure; cleared by the next successful listing.
    pub(super) list_error: Option<SharedString>,
    /// Last failed row action; cleared when the next action starts.
    pub(super) error: Option<SharedString>,
    pub(super) detail: Option<DetailState>,
    pub(super) detail_focus: FocusHandle,
    pub(super) detail_request: u64,
    /// Cross-link target to open once the list has loaded (see `navigate`).
    pending_target: Option<String>,
    /// Focus the detail page on the next render; set where no `Window` is at hand.
    pub(super) focus_detail: bool,
    confirm: Option<(Entity<ConfirmDialog>, Subscription)>,
    /// List the containers Kubernetes runs (`k8s_…`); hidden by default as they're mostly
    /// system pods and their pause containers.
    show_kubernetes: bool,
    _poll: Task<()>,
}

impl ContainersView {
    pub fn new(socket: String, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let Ok(interval) = this.update(cx, |this, cx| {
                    // Settings hides the parent pane without necessarily dropping it.
                    let store = crate::store::ProfileStore::global(cx);
                    if matches!(
                        store.read(cx).selection(),
                        crate::store::Selection::Profile(_)
                    ) {
                        this.refresh(cx);
                    } else {
                        this.suspend_detail_logs();
                    }
                    crate::store::poll_interval(cx)
                }) else {
                    break;
                };
                cx.background_executor().timer(interval).await;
            }
        });
        Self {
            socket,
            containers: Vec::new(),
            usage: Vec::new(),
            usage_error: None,
            loaded: false,
            refreshing: false,
            refresh_again: false,
            busy: None,
            list_error: None,
            error: None,
            detail: None,
            detail_focus: cx.focus_handle(),
            detail_request: 0,
            pending_target: None,
            focus_detail: false,
            confirm: None,
            show_kubernetes: false,
            _poll: poll,
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.refreshing {
            self.refresh_again = true;
            return;
        }
        self.refreshing = true;
        let socket = self.socket.clone();
        let task = cx
            .background_executor()
            .spawn(async move { (commands::list(&socket), stats::containers(&socket)) });
        cx.spawn(async move |this, cx| {
            let (result, usage) = task.await;
            this.update(cx, |this, cx| {
                this.refreshing = false;
                this.loaded = true;
                match result {
                    Ok(containers) => {
                        this.containers = containers;
                        this.list_error = None;
                    }
                    Err(error) => this.list_error = Some(error.to_string().into()),
                }
                this.usage.clear();
                match usage {
                    Ok(usage) => {
                        this.usage = usage;
                        this.usage_error = None;
                    }
                    Err(error) => {
                        this.usage_error = Some(format!("Resource usage: {error}").into())
                    }
                }
                this.refresh_detail(cx);
                this.resolve_target(cx);
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

    /// Opens a container's Info page from a cross-link, once the list has it.
    pub fn open_target(&mut self, key: String, cx: &mut Context<Self>) {
        self.pending_target = Some(key);
        self.resolve_target(cx);
    }

    fn resolve_target(&mut self, cx: &mut Context<Self>) {
        if !self.loaded {
            return;
        }
        let Some(key) = self.pending_target.take() else {
            return;
        };
        match self
            .containers
            .iter()
            .find(|c| navigate::container_matches(&c.id, &c.names, &key))
        {
            Some(container) => self.show_detail(container.clone(), DetailTab::Info, cx),
            None => self.error = Some(format!("Container “{key}” no longer exists.").into()),
        }
        cx.notify();
    }

    pub(super) fn run(&mut self, action: Action, id: String, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some((id.clone(), action));
        self.error = None;
        let socket = self.socket.clone();
        let removed_id = id.clone();
        let task = cx.background_executor().spawn(async move {
            match action {
                Action::Start => commands::start(&socket, &id),
                Action::Stop => commands::stop(&socket, &id),
                Action::Restart => commands::restart(&socket, &id),
                Action::Remove => commands::remove(&socket, &id, true),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.busy = None;
                if let Err(error) = result {
                    this.error = Some(error.to_string().into());
                } else if action == Action::Remove {
                    this.close_removed_detail(&removed_id);
                }
                this.refresh(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn confirm_remove(
        &mut self,
        container: Container,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        let dialog = cx.new(|cx| ConfirmDialog::new(
            format!("Remove container “{}”?", container.names),
            "This stops the container if running and removes its writable layer. Named volumes are kept.", cx,
        ).confirm_label("Remove"));
        let subscription = cx.subscribe(&dialog, move |this, _, result, cx| {
            this.confirm = None;
            if *result == DialogResult::Confirmed {
                this.run(Action::Remove, container.id.clone(), cx);
            }
            cx.notify();
        });
        dialog.update(cx, |dialog, cx| dialog.show(window, cx));
        self.confirm = Some((dialog, subscription));
        cx.notify();
    }

    fn row_cells(
        &self,
        index: usize,
        container: &Container,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let running = container.state == "running";
        let usage = self
            .usage
            .iter()
            .find(|u| running && u.matches(&container.id));
        let busy = self.busy.is_some();
        let start_id = container.id.clone();
        let restart_id = container.id.clone();
        let remove = container.clone();
        let logs = container.clone();
        let action = if running { Action::Stop } else { Action::Start };
        vec![
            StatusDot::for_container(
                ("container-status", index),
                &container.state,
                &container.status,
            )
            .into_any_element(),
            TextCell::new(("container-name", index), container.names.clone()).into_any_element(),
            TextCell::new(("container-image", index), container.image.clone())
                .muted()
                .into_any_element(),
            TextCell::new(("container-ports", index), container.ports.clone())
                .mono()
                .muted()
                .into_any_element(),
            TextCell::new(
                ("container-cpu", index),
                usage.map_or("—", |u| u.cpu.as_str()).to_owned(),
            )
            .mono()
            .into_any_element(),
            TextCell::new(
                ("container-memory", index),
                usage.map_or("—".to_owned(), |u| u.memory_short()),
            )
            .mono()
            .into_any_element(),
            div()
                .id(("menu-cell", index))
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    MenuButton::new(("container-menu", index))
                        .item(
                            MenuItem::new(
                                if running { "Stop" } else { "Start" },
                                cx.listener(move |this, _, _, cx| {
                                    this.run(action, start_id.clone(), cx)
                                }),
                            )
                            .icon(if running {
                                IconName::Square
                            } else {
                                IconName::Play
                            })
                            .disabled(busy),
                        )
                        .item(
                            MenuItem::new(
                                "Restart",
                                cx.listener(move |this, _, _, cx| {
                                    this.run(Action::Restart, restart_id.clone(), cx)
                                }),
                            )
                            .icon(IconName::RotateCw)
                            .disabled(busy || !running),
                        )
                        .item(
                            MenuItem::new(
                                "Logs",
                                cx.listener(move |this, _, window, cx| {
                                    this.open_detail(logs.clone(), DetailTab::Logs, window, cx)
                                }),
                            )
                            .icon(IconName::FileText),
                        )
                        .item(
                            MenuItem::new(
                                "Remove",
                                cx.listener(move |this, _, window, cx| {
                                    this.confirm_remove(remove.clone(), window, cx)
                                }),
                            )
                            .icon(IconName::Trash)
                            .danger()
                            .disabled(busy),
                        ),
                )
                .into_any_element(),
        ]
    }

    fn kubernetes_count(&self) -> usize {
        self.containers.iter().filter(|c| c.is_kubernetes()).count()
    }

    /// "5 containers", or "5 containers · 6 Kubernetes hidden" while they're filtered out.
    fn count_label(&self) -> String {
        let kubernetes = self.kubernetes_count();
        if self.show_kubernetes || kubernetes == 0 {
            format!("{} containers", self.containers.len())
        } else {
            format!(
                "{} containers · {kubernetes} Kubernetes hidden",
                self.containers.len() - kubernetes
            )
        }
    }

    fn table(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rows = Rc::new(
            self.containers
                .iter()
                .filter(|c| self.show_kubernetes || !c.is_kubernetes())
                .cloned()
                .collect::<Vec<_>>(),
        );
        let cells = rows.clone();
        let view = cx.entity().downgrade();
        let click_view = view.clone();
        Table::new(
            "containers",
            vec![
                Column::fixed("", px(16.)),
                Column::flex("Name", 1.5),
                Column::flex("Image", 1.),
                Column::flex("Ports", 1.),
                Column::fixed("CPU", px(52.)).right(),
                Column::fixed("Memory / limit", px(142.)).right(),
                Column::actions(),
            ],
            rows.len(),
            move |index, _, cx| {
                view.update(cx, |this, cx| this.row_cells(index, &cells[index], cx))
                    .unwrap_or_default()
            },
        )
        .on_row_click(move |index, window, cx| {
            click_view
                .update(cx, |this, cx| {
                    this.open_detail(rows[index].clone(), DetailTab::Info, window, cx)
                })
                .ok();
        })
        .empty(div().p_4().child(if self.loaded {
            "No containers in this profile."
        } else {
            "Loading containers…"
        }))
    }
}

impl EventEmitter<Navigate> for ContainersView {}

impl Render for ContainersView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.focus_detail) {
            window.focus(&self.detail_focus, cx);
        }
        let theme = cx.theme().clone();
        if self.detail.is_some() {
            return div()
                .size_full()
                .child(self.render_detail(cx))
                .when_some(self.confirm.as_ref(), |el, (dialog, _)| {
                    el.child(dialog.clone())
                })
                .into_any_element();
        }

        let kubernetes = self.kubernetes_count();
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
                    .child(self.count_label())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .when(kubernetes > 0, |el| {
                                el.child(
                                    Button::new("show-kubernetes", "Kubernetes")
                                        .style(if self.show_kubernetes {
                                            ButtonStyle::Selected
                                        } else {
                                            ButtonStyle::Ghost
                                        })
                                        .tooltip(if self.show_kubernetes {
                                            "Hide the containers Kubernetes runs"
                                        } else {
                                            "Show the containers Kubernetes runs"
                                        })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.show_kubernetes = !this.show_kubernetes;
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(
                                Button::new("refresh-containers", "Refresh")
                                    .loading(self.refreshing)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.error = None;
                                        this.refresh(cx);
                                    })),
                            ),
                    ),
            )
            .when_some(
                self.error.clone().or(self.list_error.clone()),
                |el, error| el.child(div().px_2().py_1().text_color(theme.error).child(error)),
            )
            .when_some(self.usage_error.clone(), |el, error| {
                el.child(div().px_2().py_1().text_color(theme.error).child(error))
            })
            .child(self.table(cx))
            .when_some(self.confirm.as_ref(), |el, (dialog, _)| {
                el.child(dialog.clone())
            })
            .into_any_element()
    }
}
