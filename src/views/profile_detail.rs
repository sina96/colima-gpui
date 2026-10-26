//! Detail pane: the selected profile's fields and actions, plus the app's empty states.

use gpui::{
    AnyElement, AnyView, App, ClipboardItem, Context, Entity, EventEmitter, SharedString,
    Subscription, Task, Window, container_query, div, prelude::*, px,
};

use crate::colima::commands::{self, KubernetesOp};
use crate::colima::{Instance, model::gib};
use crate::store::{LoadState, Op, ProfileStore, Selection};
use crate::theme::{ActiveTheme, MONO_FONT, Theme};
use crate::ui::badge::Badge;
use crate::ui::button::ButtonSize;
use crate::ui::dialog::{ConfirmDialog, DialogResult};
use crate::ui::{
    Button, ButtonStyle, ErrorNote, Icon, IconButton, IconName, InfoTip, Meter, Switch,
};
use crate::views::ai::AiView;
use crate::views::cleanup::CleanupView;
use crate::views::containers::ContainersView;
use crate::views::images::ImagesView;
use crate::views::navigate::{Navigate, Target};
use crate::views::profile_form::{ProfileForm, ProfileFormEvent};
use crate::views::volumes::VolumesView;

const INSTALL_URL: &str = "https://colima.run/docs/installation/";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProfileTab {
    Overview,
    Containers,
    Images,
    Volumes,
    Cleanup,
    /// Agent skill and prompt; shown for every profile, running or not.
    Ai,
}

impl ProfileTab {
    /// Tabs backed by the profile's Docker socket; shown only for running docker profiles.
    const DOCKER: [ProfileTab; 4] = [
        ProfileTab::Containers,
        ProfileTab::Images,
        ProfileTab::Volumes,
        ProfileTab::Cleanup,
    ];

    fn label(self) -> &'static str {
        match self {
            ProfileTab::Overview => "Overview",
            ProfileTab::Containers => "Containers",
            ProfileTab::Images => "Images",
            ProfileTab::Volumes => "Volumes",
            ProfileTab::Cleanup => "Cleanup",
            ProfileTab::Ai => "AI",
        }
    }

    /// Builds the tab's view for one Docker socket. Each view polls only while it exists.
    /// Views with cross-links report them to `ProfileDetail::navigate`.
    fn docker_view(
        self,
        socket: String,
        cx: &mut Context<ProfileDetail>,
    ) -> Option<(TabView, Option<Subscription>)> {
        fn follow<V: EventEmitter<Navigate>>(
            view: &Entity<V>,
            cx: &mut Context<ProfileDetail>,
        ) -> Option<Subscription> {
            Some(cx.subscribe(view, |this, _, event: &Navigate, cx| {
                this.navigate(event.0.clone(), cx)
            }))
        }
        Some(match self {
            ProfileTab::Overview | ProfileTab::Ai => return None,
            ProfileTab::Containers => {
                let view = cx.new(|cx| ContainersView::new(socket, cx));
                let subscription = follow(&view, cx);
                (TabView::Containers(view), subscription)
            }
            ProfileTab::Images => {
                let view = cx.new(|cx| ImagesView::new(socket, cx));
                let subscription = follow(&view, cx);
                (TabView::Images(view), subscription)
            }
            ProfileTab::Volumes => {
                let view = cx.new(|cx| VolumesView::new(socket, cx));
                let subscription = follow(&view, cx);
                (TabView::Volumes(view), subscription)
            }
            ProfileTab::Cleanup => (
                TabView::Cleanup(cx.new(|cx| CleanupView::new(socket, cx))),
                None,
            ),
        })
    }

    fn for_target(target: &Target) -> Self {
        match target {
            Target::Container(_) => ProfileTab::Containers,
            Target::Image(_) => ProfileTab::Images,
            Target::Volume(_) => ProfileTab::Volumes,
        }
    }
}

/// A Docker tab's view, typed so cross-links can open an item in it.
enum TabView {
    Containers(Entity<ContainersView>),
    Images(Entity<ImagesView>),
    Volumes(Entity<VolumesView>),
    Cleanup(Entity<CleanupView>),
}

impl TabView {
    fn any(&self) -> AnyView {
        match self {
            TabView::Containers(view) => view.clone().into(),
            TabView::Images(view) => view.clone().into(),
            TabView::Volumes(view) => view.clone().into(),
            TabView::Cleanup(view) => view.clone().into(),
        }
    }

    fn open(&self, target: Target, cx: &mut App) {
        match (self, target) {
            (TabView::Containers(view), Target::Container(key)) => {
                view.update(cx, |view, cx| view.open_target(key, cx))
            }
            (TabView::Images(view), Target::Image(key)) => {
                view.update(cx, |view, cx| view.open_target(key, cx))
            }
            (TabView::Volumes(view), Target::Volume(name)) => {
                view.update(cx, |view, cx| view.open_target(name, cx))
            }
            _ => {}
        }
    }
}

/// The live Docker tab view and what it was built for.
struct DockerTab {
    tab: ProfileTab,
    profile: String,
    socket: String,
    view: TabView,
    _navigate: Option<Subscription>,
}

pub struct ProfileDetail {
    store: Entity<ProfileStore>,
    /// Open delete confirmation and the profile it targets.
    confirm: Option<(Entity<ConfirmDialog>, String, Subscription)>,
    kubernetes_confirm: Option<(Entity<ConfirmDialog>, Subscription)>,
    kubernetes_busy: std::collections::HashMap<String, KubernetesOp>,
    kubernetes_errors: std::collections::HashMap<String, String>,
    /// Form for `Selection::NewProfile` / `EditProfile`, rebuilt when the selection changes.
    form: Option<(Selection, Entity<ProfileForm>, Subscription)>,
    tab: ProfileTab,
    tab_profile: Option<String>,
    docker_tab: Option<DockerTab>,
    /// Exists only while the AI tab is visible.
    ai: Option<Entity<AiView>>,
    /// Exists only for a running profile’s visible Overview usage block.
    usage: Option<(String, Entity<VmUsageView>)>,
    /// Profile, last busy state, and whether its Overview start log is expanded.
    overview_log: Option<(String, bool, bool)>,
    _observe: Subscription,
}

impl ProfileDetail {
    pub fn new(store: Entity<ProfileStore>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&store, |this, store, cx| {
            // Settings can hide this pane without rendering it again.
            let store = store.read(cx);
            let keep_usage = this.usage.as_ref().is_some_and(|(name, _)| {
                matches!(store.selection(), Selection::Profile(selected) if selected == name)
                    && store.selected_instance().is_some_and(Instance::is_running)
            });
            if !keep_usage {
                this.usage = None;
            }
            cx.notify();
        });
        Self {
            store,
            confirm: None,
            kubernetes_confirm: None,
            kubernetes_busy: Default::default(),
            kubernetes_errors: Default::default(),
            form: None,
            tab: ProfileTab::Overview,
            tab_profile: None,
            docker_tab: None,
            ai: None,
            usage: None,
            overview_log: None,
            _observe: observe,
        }
    }

    /// Switches tabs from the View menu. `app.rs` offers the Docker tabs only for a running
    /// Docker profile.
    pub fn show_tab(&mut self, tab: ProfileTab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    /// Opens a cross-link target: switches to its tab (same profile and socket) and asks
    /// that tab's view to open the item.
    fn navigate(&mut self, target: Target, cx: &mut Context<Self>) {
        let Some(current) = &self.docker_tab else {
            return;
        };
        let tab = ProfileTab::for_target(&target);
        if current.tab != tab {
            let profile = current.profile.clone();
            let socket = current.socket.clone();
            self.docker_tab =
                tab.docker_view(socket.clone(), cx)
                    .map(|(view, navigate)| DockerTab {
                        tab,
                        profile,
                        socket,
                        view,
                        _navigate: navigate,
                    });
            self.tab = tab;
        }
        if let Some(docker_tab) = &self.docker_tab {
            docker_tab.view.open(target, cx);
        }
        cx.notify();
    }

    fn form_for(
        &mut self,
        selection: &Selection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<ProfileForm>> {
        if let Some((key, form, _)) = &self.form
            && key == selection
        {
            return Some(form.clone());
        }
        let store = self.store.read(cx);
        let form = match selection {
            Selection::NewProfile => {
                let existing = store.instances().iter().map(|i| i.name.clone()).collect();
                cx.new(|cx| ProfileForm::create(existing, window, cx))
            }
            Selection::EditProfile(name) => {
                let instance = store.instances().iter().find(|i| &i.name == name)?.clone();
                cx.new(|cx| ProfileForm::edit(&instance, window, cx))
            }
            _ => return None,
        };
        let edited = match selection {
            Selection::EditProfile(name) => Some(name.clone()),
            _ => None,
        };
        let subscription = cx.subscribe(&form, move |this, _, event: &ProfileFormEvent, cx| {
            this.form = None;
            this.store.update(cx, |store, cx| match event {
                ProfileFormEvent::Cancelled => match &edited {
                    Some(name) => store.select(name, cx),
                    None => store.select_first(cx),
                },
                ProfileFormEvent::Submitted {
                    profile,
                    options,
                    stop_first,
                } => {
                    store.start(profile.clone(), Some(options.clone()), *stop_first, cx);
                    store.select(profile, cx);
                }
            });
        });
        self.form = Some((selection.clone(), form.clone(), subscription));
        Some(form)
    }

    fn render_log(&self, profile: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = cx.theme().clone();
        let store = self.store.read(cx);
        let panel = store.log(profile)?;
        let can_dismiss = store.busy(profile).is_none();
        let name = profile.to_owned();
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child("START LOG")
                        .when(can_dismiss, |el| {
                            el.child(Button::new("dismiss-log", "Hide").on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.store
                                        .update(cx, |store, cx| store.dismiss_log(&name, cx))
                                },
                            )))
                        }),
                )
                .child(
                    div()
                        .h(px(260.))
                        .rounded_sm()
                        .border_1()
                        .border_color(theme.border_variant)
                        .overflow_hidden()
                        .child(panel),
                )
                .into_any_element(),
        )
    }

    /// A profile whose first `colima start` is running before `colima list` shows it.
    fn render_creating(&self, profile: &str, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let error = self.store.read(cx).error(profile).cloned();
        let log = self.render_log(profile, cx);
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_3()
                    .h_12()
                    .px_4()
                    .border_b_1()
                    .border_color(theme.border_variant)
                    .child(div().text_lg().child(profile.to_owned()))
                    .child(Badge::new("Creating")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_4()
                    .when_some(error, |el, error| el.child(error_box(&theme, error)))
                    .children(log),
            )
            .into_any_element()
    }

    fn run(&mut self, op: Op, profile: String, cx: &mut Context<Self>) {
        if self.kubernetes_busy.contains_key(&profile) {
            return;
        }
        self.store
            .update(cx, |store, cx| store.run(op, profile, cx));
    }

    fn run_kubernetes(&mut self, profile: String, op: KubernetesOp, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        if self.kubernetes_busy.contains_key(&profile)
            || store.busy(&profile).is_some()
            || !store
                .instances()
                .iter()
                .any(|i| i.name == profile && i.is_running())
        {
            return;
        }
        self.kubernetes_busy.insert(profile.clone(), op);
        self.kubernetes_errors.remove(&profile);
        let name = profile.clone();
        let task = cx
            .background_executor()
            .spawn(async move { commands::kubernetes(&name, op) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.kubernetes_busy.remove(&profile);
                if let Err(error) = result {
                    this.kubernetes_errors
                        .insert(profile.clone(), crate::colima::cli::describe(&error));
                }
                this.store.update(cx, |store, cx| {
                    store.refresh_status(&profile, cx);
                    store.refresh(cx);
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn confirm_kubernetes_reset(
        &mut self,
        profile: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dialog = cx.new(|cx| {
            ConfirmDialog::new(
                format!("Reset Kubernetes on “{profile}”?"),
                "This deletes all Kubernetes objects and cluster data, then starts a fresh cluster. The VM stays running.",
                cx,
            ).confirm_label("Reset Kubernetes")
        });
        let subscription = cx.subscribe(&dialog, move |this, _, result, cx| {
            this.kubernetes_confirm = None;
            if *result == DialogResult::Confirmed {
                this.run_kubernetes(profile.clone(), KubernetesOp::Reset, cx);
            }
            cx.notify();
        });
        dialog.update(cx, |dialog, cx| dialog.show(window, cx));
        self.kubernetes_confirm = Some((dialog, subscription));
        cx.notify();
    }

    fn confirm_delete(&mut self, profile: String, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| {
            ConfirmDialog::new(
                format!("Delete profile “{profile}”?"),
                "This removes the VM. Colima keeps the container data for a new profile with the same name and runtime.",
                cx,
            )
            .confirm_label("Delete")
            .option("Also delete container data (images, containers and volumes). Needed to recreate the profile with another runtime.")
        });
        let subscription = cx.subscribe(&dialog, |this, dialog, result, cx| {
            if let Some((_, profile, _)) = this.confirm.take()
                && *result == DialogResult::Confirmed
            {
                let op = if dialog.read(cx).option_on() {
                    Op::DeleteWithData
                } else {
                    Op::Delete
                };
                this.run(op, profile, cx);
            }
            cx.notify();
        });
        dialog.update(cx, |dialog, cx| dialog.show(window, cx));
        self.confirm = Some((dialog, profile, subscription));
        cx.notify();
    }

    /// Keep the log at the foot of Overview, folding it once an operation finishes.
    fn render_overview_log(&mut self, profile: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let store = self.store.read(cx);
        let panel = store.log(profile)?;
        let busy = store.busy(profile).is_some();
        let failed = store.error(profile).is_some();
        let state = self
            .overview_log
            .get_or_insert_with(|| (profile.to_owned(), busy, busy));
        if state.0 != profile || state.1 != busy {
            *state = (profile.to_owned(), busy, busy);
        }
        let expanded = state.2;
        let theme = cx.theme().clone();
        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            Icon::new(if busy {
                                IconName::RotateCw
                            } else if failed {
                                IconName::X
                            } else {
                                IconName::Check
                            })
                            .color(if busy {
                                theme.warning
                            } else if failed {
                                theme.error
                            } else {
                                theme.success
                            }),
                        )
                        .child(
                            Button::new(
                                "toggle-start-log",
                                if expanded {
                                    "Hide start log"
                                } else {
                                    "Show start log"
                                },
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some((_, _, expanded)) = &mut this.overview_log {
                                    *expanded = !*expanded;
                                }
                                cx.notify();
                            })),
                        ),
                )
                .when(expanded, |el| {
                    el.child(
                        div()
                            .h(px(80.))
                            .border_1()
                            .border_color(theme.border_variant)
                            .rounded_sm()
                            .overflow_hidden()
                            .child(panel),
                    )
                })
                .into_any_element(),
        )
    }

    fn render_profile(&mut self, inst: &Instance, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let store = self.store.read(cx);
        let busy = store.busy(&inst.name);
        let error = store.error(&inst.name).cloned();
        let status = store.status(&inst.name).cloned();
        let status_error = store.status_error(&inst.name).cloned();
        let running = inst.is_running();
        let kubernetes_busy = self.kubernetes_busy.get(&inst.name).copied();
        let any_busy = busy.is_some() || kubernetes_busy.is_some();
        let name = inst.name.clone();
        let docker_ok = running && inst.is_docker();
        let socket = status
            .as_ref()
            .map(|s| s.docker_socket.clone())
            .filter(|s| !s.is_empty());
        let ai_view = if self.tab == ProfileTab::Ai {
            if self
                .ai
                .as_ref()
                .is_none_or(|view| view.read(cx).profile != name)
            {
                self.ai = Some(cx.new(|cx| AiView::new(name.clone(), cx)));
            }
            self.ai.clone()
        } else {
            self.ai = None;
            None
        };
        let tab_view = match (self.tab, docker_ok, &socket) {
            (ProfileTab::Overview | ProfileTab::Ai, _, _) | (_, false, _) | (_, _, None) => {
                self.docker_tab = None;
                None
            }
            (tab, true, Some(socket)) => {
                let current = self
                    .docker_tab
                    .as_ref()
                    .is_some_and(|d| d.tab == tab && d.profile == name && &d.socket == socket);
                if !current {
                    self.docker_tab =
                        tab.docker_view(socket.clone(), cx)
                            .map(|(view, navigate)| DockerTab {
                                tab,
                                profile: name.clone(),
                                socket: socket.clone(),
                                view,
                                _navigate: navigate,
                            });
                }
                self.docker_tab.as_ref().map(|d| d.view.any())
            }
        };

        let action = |id: &'static str, label: &'static str, op: Op, style: ButtonStyle| {
            let name = name.clone();
            Button::new(
                id,
                if busy == Some(op) || (op.is_delete() && busy.is_some_and(Op::is_delete)) {
                    op.progress_label()
                } else {
                    label
                },
            )
            .style(style)
            .size(ButtonSize::Medium)
            .disabled(any_busy)
            .on_click(cx.listener(move |this, _, window, cx| {
                if op.is_delete() {
                    this.confirm_delete(name.clone(), window, cx);
                } else {
                    this.run(op, name.clone(), cx);
                }
            }))
        };

        let mut actions = Vec::new();
        if running {
            actions
                .push(action("stop", "Stop", Op::Stop, ButtonStyle::Secondary).into_any_element());
            actions.push(
                action("restart", "Restart", Op::Restart, ButtonStyle::Secondary)
                    .into_any_element(),
            );
        } else {
            actions
                .push(action("start", "Start", Op::Start, ButtonStyle::Primary).into_any_element());
        }
        let edit_name = name.clone();
        actions.push(
            Button::new("edit", "Edit")
                .style(ButtonStyle::Secondary)
                .size(ButtonSize::Medium)
                .disabled(any_busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.store
                        .update(cx, |store, cx| store.select_edit(&edit_name, cx))
                }))
                .into_any_element(),
        );
        actions
            .push(action("delete", "Delete", Op::Delete, ButtonStyle::Danger).into_any_element());
        let context = docker_context(&inst.name);
        let docker = inst.is_docker();
        let missing_status = if running {
            "Waiting for status…"
        } else {
            "Unavailable while stopped"
        };
        let resources = match &self.usage {
            Some((_, usage)) => usage.clone().into_any_element(),
            None => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(overview_value(&theme, "Memory", gib(inst.memory), false))
                .child(overview_value(&theme, "CPUs", inst.cpus.to_string(), false))
                .child(overview_value(
                    &theme,
                    "Disk",
                    crate::docker::system::format_size(inst.disk),
                    false,
                ))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child("Usage available while running"),
                )
                .into_any_element(),
        };
        let resources = overview_card(&theme, "Resources", IconName::Cpu, resources);
        let unavailable =
            |value: Option<&str>| value.filter(|s| !s.is_empty()).unwrap_or("—").to_owned();
        let vm = overview_card(
            &theme,
            "VM",
            IconName::Box,
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(overview_value(
                    &theme,
                    "Driver",
                    unavailable(status.as_ref().map(|s| s.driver.as_str())),
                    false,
                ))
                .child(overview_value(
                    &theme,
                    "Architecture",
                    inst.arch.clone(),
                    false,
                ))
                .child(overview_value(
                    &theme,
                    "Runtime",
                    inst.runtime.clone(),
                    false,
                ))
                .child(overview_value(
                    &theme,
                    "Mount",
                    unavailable(status.as_ref().map(|s| s.mount_type.as_str())),
                    false,
                ))
                .child(start_on_open_row(&theme, &inst.name, cx)),
        );
        let docker_body = div().flex().flex_col().min_w_0().gap_1();
        let docker_body = if docker {
            docker_body
                .child(copy_value(
                    &theme,
                    "Context",
                    context.clone(),
                    "copy-context",
                ))
                .map(|el| match socket.clone() {
                    Some(socket) => el.child(copy_value(&theme, "Socket", socket, "copy-socket")),
                    None => el.child(overview_value(
                        &theme,
                        "Socket",
                        missing_status.into(),
                        false,
                    )),
                })
        } else {
            docker_body.child(div().text_color(theme.text_muted).child("Not using Docker"))
        };
        let docker = overview_card(&theme, "Docker", IconName::Layers, docker_body);
        // `colima list` reports saved configuration, not the live k3s service.
        let kubernetes_on = status.as_ref().is_some_and(|s| s.kubernetes);
        let kubernetes_known = status.is_some();
        let kubernetes = if running {
            match (&status, &status_error) {
                (Some(status), _) => {
                    if status.kubernetes {
                        "On"
                    } else {
                        "Off"
                    }
                }
                (None, Some(_)) => "Unavailable",
                (None, None) => "Loading…",
            }
        } else {
            missing_status
        };
        let kubernetes_name = name.clone();
        let reset_name = name.clone();
        let mut network = Some(overview_card(
            &theme,
            "Network + Kubernetes",
            IconName::Network,
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .gap_1()
                .child(overview_value(
                    &theme,
                    "Address",
                    unavailable(Some(&inst.address)),
                    true,
                ))
                .child(overview_value(
                    &theme,
                    "Kubernetes",
                    kubernetes.into(),
                    false,
                ))
                .when(running, |el| {
                    el.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_1()
                            .pt_1()
                            .child(
                                Button::new(
                                    "kubernetes-toggle",
                                    if kubernetes_on {
                                        "Stop Kubernetes"
                                    } else {
                                        "Start Kubernetes"
                                    },
                                )
                                // Colima switches kubectl on start and leaves it on stop.
                                .tooltip(if kubernetes_on {
                                    "Stop k3s. kubectl keeps this cluster as its current context."
                                } else {
                                    "Start k3s and make it kubectl's current context."
                                })
                                .disabled(any_busy || !kubernetes_known)
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.run_kubernetes(
                                            kubernetes_name.clone(),
                                            if kubernetes_on {
                                                KubernetesOp::Stop
                                            } else {
                                                KubernetesOp::Start
                                            },
                                            cx,
                                        );
                                    },
                                )),
                            )
                            .child(
                                Button::new("kubernetes-reset", "Reset…")
                                    .tooltip("Delete the cluster and start a fresh one. Also makes it kubectl's current context.")
                                    .disabled(any_busy || !kubernetes_known)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.confirm_kubernetes_reset(
                                            reset_name.clone(),
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    )
                })
                .when_some(kubernetes_busy, |el, op| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(op.progress_label()),
                    )
                })
                .when_some(self.kubernetes_errors.get(&name).cloned(), |el, error| {
                    el.child(
                        ErrorNote::new("kubernetes-error", "Kubernetes action failed")
                            .details(error),
                    )
                })
                .when_some(
                    status_error.filter(|_| running && status.is_none()),
                    |el, error| {
                        el.child(
                            ErrorNote::new(
                                "kubernetes-status-error",
                                "Kubernetes status unavailable",
                            )
                            .details(error),
                        )
                    },
                ),
        ));
        let usage = self.usage.as_ref().map(|(_, usage)| usage.clone());
        let disk_full = usage
            .as_ref()
            .is_some_and(|usage| usage.read(cx).disk.as_ref().is_some_and(disk_nearly_full));
        let reclaimable = usage.as_ref().and_then(|usage| usage.read(cx).reclaimable);
        let reclaimable_error = usage
            .as_ref()
            .and_then(|usage| usage.read(cx).reclaimable_error.clone());
        let ssh_error = usage
            .as_ref()
            .and_then(|usage| usage.read(cx).ssh_error.clone());
        let ssh_busy = usage.as_ref().is_some_and(|usage| usage.read(cx).ssh_busy);
        let ssh_profile = inst.name.clone();
        let context_command = format!("docker context use {}", shell_word(&context));
        let quick_actions = overview_card(
            &theme,
            "Quick actions",
            IconName::Play,
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            div().text_xs().text_color(theme.text_muted).child(
                                reclaimable
                                    .map(|bytes| {
                                        format!(
                                            "{} reclaimable",
                                            crate::docker::system::format_size(bytes)
                                        )
                                    })
                                    .unwrap_or_else(|| {
                                        if !docker_ok {
                                            "Docker usage unavailable"
                                        } else if reclaimable_error.is_some() {
                                            "Reclaimable space unavailable"
                                        } else {
                                            "Loading reclaimable space…"
                                        }
                                        .into()
                                    }),
                            ),
                        )
                        .child(
                            Button::new("open-cleanup", "Open Cleanup")
                                .style(ButtonStyle::Secondary)
                                .disabled(!docker_ok)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.tab = ProfileTab::Cleanup;
                                    this.usage = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("ssh-vm", "SSH into VM")
                                .style(ButtonStyle::Secondary)
                                .icon(IconName::ExternalLink)
                                .disabled(!running || any_busy || ssh_busy)
                                .on_click(move |_, _, cx| {
                                    if let Some(usage) = &usage {
                                        usage.update(cx, |usage, cx| {
                                            usage.open_ssh(ssh_profile.clone(), cx)
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("copy-context-command", "Copy context command")
                                .style(ButtonStyle::Secondary)
                                .icon(IconName::Copy)
                                .tooltip(context_command.clone())
                                .disabled(!inst.is_docker())
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        context_command.clone(),
                                    ))
                                }),
                        ),
                )
                .children(
                    reclaimable_error
                        .into_iter()
                        .chain(ssh_error)
                        .enumerate()
                        .map(|(ix, error)| {
                            div()
                                .text_xs()
                                .child(ErrorNote::new(("quick-action-error", ix), error))
                        }),
                ),
        );
        let log = self.render_overview_log(&inst.name, cx);
        let open_cleanup = cx.listener(|this, _, _, cx| {
            this.tab = ProfileTab::Cleanup;
            this.usage = None;
            cx.notify();
        });

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .h_12()
                    .px_4()
                    .border_b_1()
                    .border_color(theme.border_variant)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().text_lg().child(inst.name.clone()))
                            .child(Badge::status(&inst.status)),
                    )
                    .child(div().flex().items_center().gap_1().children(actions)),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1()
                    .h_8()
                    .px_4()
                    .border_b_1()
                    .border_color(theme.border_variant)
                    .children(
                        std::iter::once(ProfileTab::Overview)
                            .chain(docker_ok.then_some(ProfileTab::DOCKER).into_iter().flatten())
                            .chain(std::iter::once(ProfileTab::Ai))
                            .map(|tab| {
                                Button::new(tab.label(), tab.label())
                                    .style(if self.tab == tab {
                                        ButtonStyle::Selected
                                    } else {
                                        ButtonStyle::Ghost
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.tab = tab;
                                        cx.notify();
                                    }))
                            }),
                    )
                    // A short note with the reason on hover, so the tab row stays one line.
                    .when(running && !inst.is_docker(), |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .ml_1()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child("No Docker tabs")
                                .child(InfoTip::new(
                                    "no-docker-tabs",
                                    format!(
                                        "Containers, images, volumes and cleanup need the Docker runtime. This profile uses {}.",
                                        inst.container_runtime()
                                    ),
                                )),
                        )
                    }),
            )
            .when_some(ai_view, |el, view| {
                el.child(div().flex_1().min_h_0().child(view))
            })
            .when(ProfileTab::DOCKER.contains(&self.tab) && docker_ok, |el| {
                el.child(div().flex_1().min_h_0().map(|el| match tab_view {
                    Some(view) => el.child(view),
                    None => el.child(
                        div()
                            .p_4()
                            .text_color(theme.text_muted)
                            .child("Waiting for the profile’s Docker socket…"),
                    ),
                }))
            })
            .when(
                self.tab == ProfileTab::Overview
                    || (!docker_ok && ProfileTab::DOCKER.contains(&self.tab)),
                |el| {
                    el.child(
                        // The query selects columns from the viewport width. Its child
                        // scrolls; the grid itself keeps its natural content height.
                        container_query(move |size, _, _| {
                            let columns = overview_columns(size.width / px(1.) - 24.);
                            let wide = columns == 3;
                            div()
                                .id("overview-scroll")
                                .size_full()
                                .overflow_y_scroll()
                                .child(div().flex().flex_col().gap_2().p_3()
                                    .when_some(error, |el, error| el.child(error_box(&theme, error)))
                                    .when(disk_full, |el| el.child(
                                        div().flex().flex_wrap().items_center().gap_2().p_2()
                                            .rounded_sm().border_1().border_color(theme.warning.opacity(0.5))
                                            .bg(theme.warning.opacity(0.08)).text_xs().text_color(theme.warning)
                                            .child(div().flex_1().min_w_0().child(
                                                "Data disk is at least 90% full. Prune unused data or increase the disk via Edit."
                                            ))
                                            .when(docker_ok, |el| el.child(Button::new("disk-cleanup", "Open Cleanup")
                                                .style(ButtonStyle::Secondary)
                                                .on_click(open_cleanup))),
                                    ))
                                    .child(div().grid().w_full().min_w_0().grid_cols(columns).gap_2()
                                        .child(resources).child(vm)
                                        .when(wide, |el| el.children(network.take()))
                                        .child(div().min_w_0().when(wide, |el| el.col_span_full()).child(docker))
                                        .when(!wide, |el| el.children(network.take()))
                                        .child(div().min_w_0().col_span_full().child(quick_actions)))
                                    .children(log))
                        }).flex_1().min_h_0()
                    )
                },
            )
            .into_any_element()
    }
}

impl Render for ProfileDetail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let store = self.store.read(cx);
        let load = store.load_state().clone();
        let selection = store.selection().clone();
        let selected = store.selected_instance().cloned();
        let usage_socket = selected
            .as_ref()
            .filter(|inst| inst.is_docker())
            .and_then(|inst| store.status(&inst.name))
            .map(|status| status.docker_socket.clone())
            .filter(|socket| !socket.is_empty());
        let tab_profile = selected.as_ref().map(|instance| instance.name.clone());
        if self.tab_profile != tab_profile {
            self.tab = ProfileTab::Overview;
            self.tab_profile = tab_profile;
            self.docker_tab = None;
        }
        if selected
            .as_ref()
            .is_none_or(|instance| !instance.is_running() || !instance.is_docker())
        {
            self.docker_tab = None;
            if ProfileTab::DOCKER.contains(&self.tab) {
                self.tab = ProfileTab::Overview;
            }
        }
        let creating = match &selection {
            Selection::Profile(name) if selected.is_none() && store.log(name).is_some() => {
                Some(name.clone())
            }
            _ => None,
        };
        let form = match &selection {
            Selection::NewProfile | Selection::EditProfile(_) if load == LoadState::Ready => {
                self.form_for(&selection, window, cx)
            }
            _ => {
                self.form = None;
                None
            }
        };

        // Keep resource polling scoped to the running profile’s visible Overview.
        let usage_profile = selected.as_ref().filter(|inst| {
            inst.is_running()
                && self.tab == ProfileTab::Overview
                && matches!(&selection, Selection::Profile(name) if name == &inst.name)
        });
        if let Some(inst) = usage_profile {
            if self.usage.as_ref().is_none_or(|(name, usage)| {
                name != &inst.name || usage.read(cx).socket != usage_socket
            }) {
                let parent = cx.entity().downgrade();
                self.usage = Some((
                    inst.name.clone(),
                    cx.new(|usage_cx| {
                        VmUsageView::new(
                            inst.name.clone(),
                            inst.cpus,
                            usage_socket,
                            parent,
                            usage_cx,
                        )
                    }),
                ));
            }
        } else {
            self.usage = None;
        }

        let content = match (&load, &selection, selected) {
            // `app.rs` shows Settings full-window instead of this pane.
            (_, Selection::Settings, _) => div().into_any_element(),
            (LoadState::ColimaMissing, _, _) => empty_state(
                &theme,
                "Colima isn't installed",
                if cfg!(target_os = "linux") {
                    "Install Colima (see the installation guide). Profiles show up here as soon as it's available."
                } else {
                    "Install it with Homebrew. Profiles show up here as soon as it's available."
                },
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .when(!cfg!(target_os = "linux"), |el| {
                            el.child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .bg(theme.element_background)
                                    .border_1()
                                    .border_color(theme.border_variant)
                                    .font_family(MONO_FONT)
                                    .text_sm()
                                    .child("brew install colima"),
                            )
                        })
                        .child(
                            Button::new("install-docs", "Installation guide")
                                .style(ButtonStyle::Primary)
                                .size(ButtonSize::Medium)
                                .on_click(|_, _, cx| cx.open_url(INSTALL_URL)),
                        )
                        .into_any_element(),
                ),
            ),
            (LoadState::Loading, _, _) => empty_state(&theme, "Loading profiles…", "", None),
            (LoadState::Failed(error), _, _) => empty_state(
                &theme,
                "Couldn't list profiles",
                error,
                Some(
                    Button::new("retry", "Retry")
                        .style(ButtonStyle::Secondary)
                        .size(ButtonSize::Medium)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.store.update(cx, |store, cx| store.refresh(cx))
                        }))
                        .into_any_element(),
                ),
            ),
            (LoadState::Ready, Selection::NewProfile | Selection::EditProfile(_), _)
                if form.is_some() =>
            {
                form.map(|form| form.into_any_element())
                    .unwrap_or_else(|| div().into_any_element())
            }
            (LoadState::Ready, _, Some(inst)) => self.render_profile(&inst, cx),
            (LoadState::Ready, Selection::Profile(_), None) if creating.is_some() => {
                let name = creating.unwrap_or_default();
                self.render_creating(&name, cx)
            }
            (LoadState::Ready, _, None) => empty_state(
                &theme,
                "No profiles yet",
                "Create one to run containers on this computer.",
                Some(
                    Button::new("create-first", "New profile")
                        .style(ButtonStyle::Primary)
                        .size(ButtonSize::Medium)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.store
                                .update(cx, |store, cx| store.select_new_profile(cx))
                        }))
                        .into_any_element(),
                ),
            ),
        };

        div()
            .size_full()
            .bg(theme.editor_background)
            .child(content)
            .when_some(self.confirm.as_ref(), |el, (dialog, _, _)| {
                el.child(dialog.clone())
            })
            .when_some(self.kubernetes_confirm.as_ref(), |el, (dialog, _)| {
                el.child(dialog.clone())
            })
    }
}

/// The Overview usage block owns its poller, so hidden/stopped profiles do no work.
struct VmUsageView {
    usage: Option<crate::docker::stats::VmUsage>,
    disk: Option<crate::docker::system::VmDisk>,
    error: Option<String>,
    disk_error: Option<String>,
    cpus: u32,
    socket: Option<String>,
    reclaimable: Option<u64>,
    reclaimable_error: Option<String>,
    ssh_error: Option<String>,
    ssh_busy: bool,
    parent: gpui::WeakEntity<ProfileDetail>,
    _poll: Task<()>,
}

impl VmUsageView {
    fn new(
        profile: String,
        cpus: u32,
        socket: Option<String>,
        parent: gpui::WeakEntity<ProfileDetail>,
        cx: &mut Context<Self>,
    ) -> Self {
        let poll_socket = socket.clone();
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let name = profile.clone();
                let socket = poll_socket.clone();
                let (usage, disk, reclaimable) = cx
                    .background_executor()
                    .spawn(async move {
                        (
                            crate::docker::stats::vm(&name),
                            crate::colima::commands::vm_disk(&name),
                            socket.as_deref().map(crate::docker::system::disk_usage),
                        )
                    })
                    .await;
                let Ok(interval) = this.update(cx, |this, cx| {
                    match usage {
                        Ok(usage) => {
                            this.usage = Some(usage);
                            this.error = None;
                        }
                        Err(error) => {
                            this.usage = None;
                            this.error = Some(crate::colima::cli::describe(&error));
                        }
                    }
                    match disk {
                        Ok(disk) => {
                            this.disk = Some(disk);
                            this.disk_error = None;
                        }
                        Err(error) => {
                            this.disk = None;
                            this.disk_error = Some(crate::colima::cli::describe(&error));
                        }
                    }
                    match reclaimable {
                        Some(Ok(rows)) => {
                            this.reclaimable = Some(
                                rows.iter()
                                    .fold(0u64, |sum, row| sum.saturating_add(row.reclaimable)),
                            );
                            this.reclaimable_error = None;
                        }
                        Some(Err(error)) => {
                            this.reclaimable = None;
                            this.reclaimable_error = Some(format!("Docker usage: {error}"));
                        }
                        None => {}
                    }
                    let _ = this.parent.update(cx, |_, cx| cx.notify());
                    cx.notify();
                    crate::store::poll_interval(cx)
                }) else {
                    break;
                };
                cx.background_executor().timer(interval).await;
            }
        });
        Self {
            usage: None,
            disk: None,
            error: None,
            disk_error: None,
            cpus,
            socket,
            reclaimable: None,
            reclaimable_error: None,
            ssh_error: None,
            ssh_busy: false,
            parent,
            _poll: poll,
        }
    }

    fn open_ssh(&mut self, profile: String, cx: &mut Context<Self>) {
        self.ssh_busy = true;
        self.ssh_error = None;
        let _ = self.parent.update(cx, |_, cx| cx.notify());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    #[cfg(not(target_os = "linux"))]
                    {
                        let command = crate::colima::cli::command("colima");
                        let binary = command.get_program().to_string_lossy();
                        let path = command
                            .get_envs()
                            .find(|(key, _)| *key == "PATH")
                            .and_then(|(_, value)| value)
                            .unwrap_or_default()
                            .to_string_lossy();
                        let script = terminal_script(&binary, &path, &profile);
                        crate::colima::cli::run("/usr/bin/osascript", &["-e", &script]).map(|_| ())
                    }
                    #[cfg(target_os = "linux")]
                    {
                        open_linux_ssh(&profile)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.ssh_busy = false;
                this.ssh_error = result
                    .err()
                    .map(|error| format!("Couldn't open Terminal: {error}"));
                let _ = this.parent.update(cx, |_, cx| cx.notify());
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for VmUsageView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .text_xs()
            .map(|el| match &self.usage {
                Some(usage) => el
                    .child(Meter::new(
                        "Memory",
                        format!(
                            "{:.2} / {:.2} GiB",
                            usage.memory_used as f64 / 1073741824.,
                            usage.memory_total as f64 / 1073741824.
                        ),
                        usage.memory_used as f64,
                        usage.memory_total as f64,
                    ))
                    .child(Meter::new(
                        "Load · 1 min",
                        format!("{:.2} / {} CPUs", usage.load[0], self.cpus),
                        usage.load[0],
                        f64::from(self.cpus.max(1)),
                    )),
                None => match &self.error {
                    Some(error) => el.child(
                        ErrorNote::new("vm-usage-error", "VM usage unavailable")
                            .details(error.clone()),
                    ),
                    None => el.child(
                        div()
                            .text_color(theme.text_muted)
                            .child("Loading VM usage…"),
                    ),
                },
            })
            .map(|el| match &self.disk {
                Some(disk) => el.child(Meter::new(
                    "Disk",
                    format!(
                        "{} / {}",
                        crate::docker::system::format_size(disk.used),
                        crate::docker::system::format_size(disk.size)
                    ),
                    disk.used as f64,
                    disk.size as f64,
                )),
                None => match &self.disk_error {
                    Some(error) => el.child(
                        ErrorNote::new("vm-disk-error", "Disk usage unavailable")
                            .details(error.clone()),
                    ),
                    None => el.child(
                        div()
                            .text_color(theme.text_muted)
                            .child("Loading disk usage…"),
                    ),
                },
            })
    }
}

fn overview_card(theme: &Theme, title: &str, icon: IconName, body: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .gap_2()
        .p_2()
        .rounded_sm()
        .border_1()
        .border_color(theme.border_variant)
        .bg(theme.surface_background)
        .text_sm()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_color(theme.text_muted)
                .child(Icon::new(icon))
                .child(title.to_owned()),
        )
        .child(body)
        .into_any_element()
}

/// "Start when Colima GPUI opens": a per-profile switch, stored in the app's settings.
fn start_on_open_row(theme: &Theme, profile: &str, cx: &mut Context<ProfileDetail>) -> AnyElement {
    let on = crate::settings::get(cx).starts_on_open(profile);
    let profile = profile.to_owned();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .pt_1()
        .text_xs()
        .child(
            div()
                .text_color(theme.text_muted)
                .child("Start when Colima GPUI opens"),
        )
        .child(
            Switch::new("start-on-open", on)
                .tooltip("If this profile is stopped, start it whenever Colima GPUI opens")
                .on_toggle(cx.listener(move |_, on: &bool, _, cx| {
                    let on = *on;
                    crate::settings::update(cx, |s| s.set_start_on_open(&profile, on));
                    cx.notify();
                })),
        )
        .into_any_element()
}

fn overview_value(theme: &Theme, label: &str, value: String, mono: bool) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .min_w_0()
        .text_xs()
        .child(
            div()
                .flex_none()
                .text_color(theme.text_muted)
                .child(label.to_owned()),
        )
        .child(
            div()
                .id(SharedString::from(format!("overview-{label}")))
                .min_w_0()
                .truncate()
                .tooltip(crate::ui::tooltip::tooltip(value.clone()))
                .when(mono, |el| el.font_family(MONO_FONT))
                .child(value),
        )
        .into_any_element()
}

fn copy_value(theme: &Theme, label: &str, value: String, id: &'static str) -> AnyElement {
    let copied = value.clone();
    let value = crate::colima::paths::tilde(&value);
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(
            div()
                .text_xs()
                .text_color(theme.text_muted)
                .child(label.to_owned()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .min_w_0()
                .child(
                    div()
                        .id((id, 0usize))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(MONO_FONT)
                        .text_xs()
                        .tooltip(crate::ui::tooltip::tooltip(value.clone()))
                        .child(value),
                )
                .child(
                    IconButton::new(id, IconName::Copy)
                        .tooltip(format!("Copy Docker {}", label.to_lowercase()))
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))
                        }),
                ),
        )
        .into_any_element()
}

// Breakpoints use the available pane width, after sidebar and Overview padding.
fn overview_columns(width: f32) -> u16 {
    if width >= 840. {
        3
    } else if width >= 480. {
        2
    } else {
        1
    }
}

fn disk_nearly_full(disk: &crate::docker::system::VmDisk) -> bool {
    disk.size > 0 && u128::from(disk.used) * 10 >= u128::from(disk.size) * 9
}

fn shell_word(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_./-".contains(&c))
    {
        value.into()
    } else {
        shell_quote(value)
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(any(not(target_os = "linux"), test))]
fn terminal_script(binary: &str, path: &str, profile: &str) -> String {
    let command = format!(
        "PATH={} {} ssh -p {}",
        shell_quote(path),
        shell_quote(binary),
        shell_quote(profile)
    );
    let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
    format!("tell application \"Terminal\" to do script \"{escaped}\"")
}

// Keep these helpers available to macOS tests as well as the Linux launcher.
#[cfg(any(target_os = "linux", test))]
const LINUX_TERMINALS: &[&str] = &[
    "x-terminal-emulator",
    "gnome-terminal",
    "konsole",
    "xfce4-terminal",
    "kitty",
    "alacritty",
    "wezterm",
    "foot",
    "xterm",
];

#[cfg(any(target_os = "linux", test))]
fn linux_terminal(
    preferred: Option<&std::ffi::OsStr>,
    path: Option<&std::ffi::OsStr>,
) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dirs: Vec<_> = path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect();
    let find = |name: &std::ffi::OsStr| {
        let name = std::path::Path::new(name);
        let candidates = if name.components().count() > 1 {
            vec![name.to_path_buf()]
        } else {
            dirs.iter().map(|dir| dir.join(name)).collect()
        };
        candidates.into_iter().find(|candidate| {
            candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
    };
    // TERMINAL names one executable, including paths with spaces; never parse shell text.
    if let Some(preferred) = preferred.filter(|value| !value.is_empty()) {
        return find(preferred);
    }
    LINUX_TERMINALS
        .iter()
        .find_map(|name| find(std::ffi::OsStr::new(name)))
}

#[cfg(any(target_os = "linux", test))]
fn linux_terminal_command(
    terminal: &std::path::Path,
    colima: &std::process::Command,
    profile: &str,
) -> std::process::Command {
    let mut command = crate::colima::cli::command(&terminal.to_string_lossy());
    let name = terminal
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match name {
        "gnome-terminal" => {
            command.arg("--");
        }
        "xfce4-terminal" => {
            command.arg("--execute");
        }
        "wezterm" => {
            command.args(["start", "--"]);
        }
        "kitty" | "foot" => {}
        _ => {
            command.arg("-e");
        }
    }
    command
        .arg(colima.get_program())
        .args(["ssh", "-p", profile]);
    command.envs(
        colima
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key, value))),
    );
    command
}

#[cfg(target_os = "linux")]
fn open_linux_ssh(profile: &str) -> Result<(), crate::colima::cli::CliError> {
    let colima = crate::colima::cli::command("colima");
    let terminal = linux_terminal(
        std::env::var_os("TERMINAL").as_deref(),
        std::env::var_os("PATH").as_deref(),
    ).ok_or_else(|| crate::colima::cli::CliError::Parse(format!(
        "No terminal found. Set TERMINAL to a terminal executable, or copy this command:\n{} ssh -p {}",
        shell_word(&colima.get_program().to_string_lossy()), shell_word(profile),
    )))?;
    let mut child = linux_terminal_command(&terminal, &colima, profile)
        .spawn()
        .map_err(|source| crate::colima::cli::CliError::Spawn {
            program: terminal.display().to_string(),
            source,
        })?;
    // Terminal windows can live for hours. Reap without blocking the UI or executor.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Docker context colima creates for a profile.
fn docker_context(profile: &str) -> String {
    if profile == "default" {
        "colima".into()
    } else {
        format!("colima-{profile}")
    }
}

fn error_box(theme: &Theme, error: SharedString) -> impl IntoElement + use<> {
    div()
        .p_2()
        .rounded_sm()
        .border_1()
        .border_color(theme.error.opacity(0.5))
        .bg(theme.error.opacity(0.08))
        .text_sm()
        .text_color(theme.error)
        .font_family(MONO_FONT)
        .child(error)
}

fn empty_state(theme: &Theme, title: &str, detail: &str, action: Option<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .size_full()
        .p_8()
        .child(div().text_color(theme.text).child(title.to_owned()))
        .when(!detail.is_empty(), |el| {
            el.child(
                div()
                    .max_w(px(360.))
                    .text_center()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(detail.to_owned()),
            )
        })
        .when_some(action, |el, action| el.child(div().pt_2().child(action)))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_warning_starts_at_ninety_percent() {
        for (used, size, expected) in [
            (89, 100, false),
            (90, 100, true),
            (100, 100, true),
            (0, 0, false),
            (u64::MAX, u64::MAX, true),
        ] {
            assert_eq!(
                disk_nearly_full(&crate::docker::system::VmDisk {
                    used,
                    size,
                    allocated: None
                }),
                expected
            );
        }
    }

    #[test]
    fn linux_terminals_receive_literal_argument_lists() {
        use std::ffi::OsString;
        let binary = "/tmp/t52-colima with spaces";
        let profile = "t52-a'b\\c$(printf injected)";
        let mut colima = std::process::Command::new(binary);
        colima.env("PATH", "/tmp/t52-bin:/usr/bin");
        for (terminal, prefix) in [
            ("x-terminal-emulator", vec!["-e"]),
            ("gnome-terminal", vec!["--"]),
            ("konsole", vec!["-e"]),
            ("xfce4-terminal", vec!["--execute"]),
            ("kitty", vec![]),
            ("alacritty", vec!["-e"]),
            ("wezterm", vec!["start", "--"]),
            ("foot", vec![]),
            ("xterm", vec!["-e"]),
            ("custom-terminal", vec!["-e"]),
        ] {
            let command = linux_terminal_command(
                &std::path::Path::new("/tmp/t52-terminal dir").join(terminal),
                &colima,
                profile,
            );
            let expected: Vec<OsString> = prefix
                .into_iter()
                .chain([binary, "ssh", "-p", profile])
                .map(Into::into)
                .collect();
            assert_eq!(
                command.get_args().collect::<Vec<_>>(),
                expected,
                "{terminal}"
            );
            assert_eq!(
                command.get_program(),
                std::path::Path::new("/tmp/t52-terminal dir").join(terminal)
            );
            assert!(command.get_envs().any(|(key, value)| key == "PATH"
                && value == Some(std::ffi::OsStr::new("/tmp/t52-bin:/usr/bin"))));
            assert_eq!(command.get_current_dir(), Some(std::path::Path::new("/")));
        }
    }

    #[test]
    fn linux_terminal_selection_respects_preference_path_order_and_executability() {
        use std::ffi::OsStr;
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("t52-terminals-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.as_os_str();
        assert!(linux_terminal(None, Some(path)).is_none());
        for name in [
            "kitty",
            "gnome-terminal",
            "x-terminal-emulator",
            "custom terminal",
        ] {
            let binary = dir.join(name);
            std::fs::write(&binary, "").unwrap();
            std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert_eq!(
            linux_terminal(None, Some(path)),
            Some(dir.join("x-terminal-emulator"))
        );
        assert_eq!(
            linux_terminal(Some(OsStr::new("")), Some(path)),
            Some(dir.join("x-terminal-emulator"))
        );
        assert_eq!(
            linux_terminal(Some(OsStr::new("kitty")), Some(path)),
            Some(dir.join("kitty"))
        );
        assert_eq!(
            linux_terminal(Some(dir.join("custom terminal").as_os_str()), None),
            Some(dir.join("custom terminal"))
        );
        assert!(linux_terminal(Some(OsStr::new("missing")), Some(path)).is_none());
        assert!(linux_terminal(Some(OsStr::new("kitty -e")), Some(path)).is_none());
        std::fs::set_permissions(
            dir.join("x-terminal-emulator"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert_eq!(
            linux_terminal(None, Some(path)),
            Some(dir.join("gnome-terminal"))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn terminal_arguments_survive_both_quoting_layers() {
        let binary = "/tmp/colima \"test\"";
        let path = "/opt/homebrew/bin:/tmp/with space";
        let profile = "t34-a'b\\c$(printf injected)";
        let script = terminal_script(binary, path, profile);
        let encoded = script
            .strip_prefix("tell application \"Terminal\" to do script \"")
            .unwrap()
            .strip_suffix('"')
            .unwrap();
        let command = encoded.replace("\\\"", "\"").replace("\\\\", "\\");
        // Substitute printf for colima to inspect shell arguments without launching a VM.
        let command = command.replace(&shell_quote(binary), "printf '%s\\n'");
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", &command])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("ssh\n-p\n{profile}\n")
        );
        assert_eq!(shell_word("colima-gpui-test"), "colima-gpui-test");
    }

    #[test]
    fn docker_context_names_match_colima() {
        assert_eq!(docker_context("default"), "colima");
        assert_eq!(docker_context("gpui-test"), "colima-gpui-test");
    }
}
