//! Container detail state and rendering. The parent owns the single visible poller.
use crate::ui::ErrorNote;
use std::collections::HashSet;

use gpui::{Context, Entity, Window, div, prelude::*};

use super::log_panel::LogPanel;
use super::navigate::{Navigate, Target};

use super::containers::{Action, ContainersView};
use crate::{
    docker::{
        commands::{self, ContainerInspect},
        model::Container,
    },
    theme::ActiveTheme,
    ui::{Button, CardGrid, DetailCard, DetailPage, Field, IconButton, IconName, Meter, StatusDot},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DetailTab {
    Info,
    Logs,
    Stats,
}

pub(super) struct DetailState {
    container: Container,
    tab: DetailTab,
    logs: Option<(Entity<LogPanel>, commands::LogGuard)>,
    inspect: Option<Result<ContainerInspect, String>>,
    loading: bool,
    revealed: HashSet<usize>,
}

impl ContainersView {
    pub(super) fn close_removed_detail(&mut self, id: &str) {
        if self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.container.id == id)
        {
            self.detail_request += 1;
            self.detail = None;
        }
    }

    pub(super) fn open_detail(
        &mut self,
        container: Container,
        tab: DetailTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_detail(container, tab, cx);
        window.focus(&self.detail_focus, cx);
    }

    /// Opens the detail page and focuses it on the next render.
    pub(super) fn show_detail(
        &mut self,
        container: Container,
        tab: DetailTab,
        cx: &mut Context<Self>,
    ) {
        self.detail_request += 1;
        self.detail = Some(DetailState {
            container,
            tab,
            logs: None,
            inspect: None,
            loading: false,
            revealed: HashSet::new(),
        });
        self.select_detail_tab(tab, cx);
        self.focus_detail = true;
        self.refresh_detail(cx);
        cx.notify();
    }

    fn select_detail_tab(&mut self, tab: DetailTab, cx: &mut Context<Self>) {
        if let Some(detail) = &mut self.detail {
            detail.tab = tab;
            if tab == DetailTab::Logs {
                if detail.logs.is_none() {
                    let (receiver, guard) =
                        commands::logs_follow(&self.socket, &detail.container.id, 500).into_parts();
                    let name = detail
                        .container
                        .names
                        .split(',')
                        .next()
                        .unwrap_or_default()
                        .to_owned();
                    detail.logs = Some((
                        cx.new(|cx| {
                            LogPanel::new(receiver, cx)
                                .plain_stderr()
                                .running_label("Following logs…")
                                .save_name(name)
                        }),
                        guard,
                    ));
                }
            } else {
                detail.logs = None;
            }
        }
    }

    pub(super) fn suspend_detail_logs(&mut self) {
        if let Some(detail) = &mut self.detail {
            detail.logs = None;
        }
    }

    pub(super) fn refresh_detail(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.detail.as_ref().map(|detail| detail.tab) {
            self.select_detail_tab(tab, cx);
        }
        let Some(detail) = &mut self.detail else {
            return;
        };
        if let Some(container) = self.containers.iter().find(|c| c.id == detail.container.id) {
            detail.container = container.clone();
        }
        if detail.loading {
            return;
        }
        detail.loading = true;
        let request = self.detail_request;
        let id = detail.container.id.clone();
        let socket = self.socket.clone();
        let task = cx
            .background_executor()
            .spawn(async move { commands::inspect(&socket, &id).map_err(|e| e.to_string()) });
        cx.spawn(async move |this, cx| {
            let inspect = task.await;
            this.update(cx, |this, cx| {
                if request != this.detail_request {
                    return;
                }
                if let Some(detail) = &mut this.detail {
                    detail.loading = false;
                    detail.inspect = Some(inspect);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let detail = self.detail.as_ref().expect("visible detail");
        let container = &detail.container;
        let theme = cx.theme().clone();
        let running = container.state == "running";
        let busy = self.busy.is_some();
        let id = container.id.clone();
        let restart_id = id.clone();
        let remove = container.clone();
        let action = if running { Action::Stop } else { Action::Start };
        let body = match detail.tab {
            DetailTab::Logs => div()
                .h_full()
                .min_h_0()
                .p_2()
                .children(detail.logs.as_ref().map(|(panel, _)| panel.clone()))
                .into_any_element(),
            DetailTab::Info => self.render_info(cx).into_any_element(),
            DetailTab::Stats => {
                let usage = running
                    .then(|| self.usage.iter().find(|u| u.matches(&container.id)))
                    .flatten();
                let mut body = div().p_3().flex().flex_col().gap_4();
                if let Some(usage) = usage {
                    let cpu = usage.cpu.trim_end_matches('%').parse::<f64>().unwrap_or(0.);
                    body = body.child(Meter::new(
                        "CPU (100% = one core)",
                        usage.cpu.clone(),
                        cpu,
                        100.,
                    ));
                    if let Some((used, total)) = memory_pair(&usage.memory) {
                        body = body.child(Meter::new(
                            "Memory / limit",
                            usage.memory.clone(),
                            used,
                            total,
                        ));
                    } else {
                        body = body.child(format!("Memory / limit: {}", usage.memory));
                    }
                } else {
                    body = body.child(if running {
                        "Waiting for resource usage…"
                    } else {
                        "Resource usage is available while running."
                    });
                }
                body.when_some(self.usage_error.clone(), |el, error| {
                    el.child(ErrorNote::new("container-usage-error", error))
                })
                .into_any_element()
            }
        };
        let mut page = DetailPage::new(container.names.clone())
            .subtitle(container.image.clone())
            .status(StatusDot::for_container(
                "detail-status",
                &container.state,
                &container.status,
            ))
            .track_focus(&self.detail_focus)
            .on_back({
                let entity = cx.entity().downgrade();
                move |_, cx| {
                    entity
                        .update(cx, |this, cx| {
                            this.detail_request += 1;
                            this.detail = None;
                            cx.notify();
                        })
                        .ok();
                }
            })
            .action(
                IconButton::new(
                    "detail-start-stop",
                    if running {
                        IconName::Square
                    } else {
                        IconName::Play
                    },
                )
                .tooltip(if running { "Stop" } else { "Start" })
                .disabled(busy)
                .on_click(cx.listener(move |this, _, _, cx| this.run(action, id.clone(), cx))),
            )
            .action(
                IconButton::new("detail-restart", IconName::RotateCw)
                    .tooltip("Restart")
                    .disabled(busy || !running)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.run(Action::Restart, restart_id.clone(), cx)
                    })),
            )
            .action(
                IconButton::new("detail-remove", IconName::Trash)
                    .tooltip("Remove")
                    .danger()
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_remove(remove.clone(), window, cx)
                    })),
            );
        for (label, tab) in [
            ("Info", DetailTab::Info),
            ("Logs", DetailTab::Logs),
            ("Stats", DetailTab::Stats),
        ] {
            page = page.tab(
                label,
                detail.tab == tab,
                cx.listener(move |this, _, window, cx| {
                    this.select_detail_tab(tab, cx);
                    window.focus(&this.detail_focus, cx);
                    cx.notify();
                }),
            );
        }
        // Logs fill the page and scroll inside the panel; Info and Stats keep their natural
        // height so the page itself scrolls.
        let fill = detail.tab == DetailTab::Logs;
        page.body(
            div()
                .when(fill, |el| el.h_full().min_h_0())
                .flex()
                .flex_col()
                .text_sm()
                .text_color(theme.text)
                .bg(theme.editor_background)
                .when_some(
                    self.error.clone().or(self.list_error.clone()),
                    |el, error| {
                        el.child(
                            div()
                                .p_2()
                                .child(ErrorNote::new("container-detail-error", error)),
                        )
                    },
                )
                .child(body),
        )
    }

    fn render_info(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let detail = self.detail.as_ref().expect("visible detail");
        let container = &detail.container;
        let usage = self
            .usage
            .iter()
            .find(|u| container.state == "running" && u.matches(&container.id));
        let image = container.image.clone();
        let summary = DetailCard::new(IconName::Box, "Summary")
            .child(Field::new("Status", container.state.clone()))
            .child(Field::new("Uptime", container.status.clone()))
            .child(Field::new("Image", container.image.clone()).link(
                "container-image-link",
                cx.listener(move |_, _, _, cx| cx.emit(Navigate(Target::Image(image.clone())))),
            ))
            .child(Field::new("Created", container.created_at.clone()))
            .child(
                Field::new(
                    "Container ID",
                    detail
                        .inspect
                        .as_ref()
                        .and_then(|result| result.as_ref().ok())
                        .map(|inspect| inspect.id.clone())
                        .unwrap_or_else(|| container.id.clone()),
                )
                .mono()
                .copyable(),
            );
        let mut resources = DetailCard::new(IconName::Cpu, "Resources");
        if let Some(usage) = usage {
            let cpu = usage.cpu.trim_end_matches('%').parse::<f64>().unwrap_or(0.);
            resources = resources.child(Meter::new(
                "CPU (100% = one core)",
                usage.cpu.clone(),
                cpu,
                100.,
            ));
            if let Some((used, total)) = memory_pair(&usage.memory) {
                resources = resources.child(Meter::new(
                    "Memory / limit",
                    usage.memory.clone(),
                    used,
                    total,
                ));
            } else {
                resources = resources.child(Field::new("Memory / limit", usage.memory.clone()));
            }
        } else {
            resources = resources.empty(if container.state == "running" {
                "Waiting for resource usage…"
            } else {
                "Resource usage is available while running."
            });
        }
        if let Some(error) = &self.usage_error {
            resources = resources.child(ErrorNote::new("container-resources-error", error.clone()));
        }
        let grid = CardGrid::new().card(summary).card(resources);
        let body = div().p_3().text_sm();
        let inspect = match &detail.inspect {
            Some(Ok(inspect)) => inspect,
            Some(Err(error)) => {
                return body
                    .child(grid)
                    .child(ErrorNote::new("container-error", error.clone()))
                    .into_any_element();
            }
            None => {
                return body
                    .child(grid)
                    .child("Loading inspect…")
                    .into_any_element();
            }
        };
        let command = DetailCard::new(IconName::Terminal, "Command")
            .child(
                Field::new(
                    "Command",
                    format!("{} {}", inspect.path, inspect.args.join(" "))
                        .trim()
                        .to_owned(),
                )
                .mono()
                .copyable(),
            )
            .child(
                Field::new(
                    "Entrypoint",
                    inspect
                        .config
                        .entrypoint
                        .as_deref()
                        .unwrap_or_default()
                        .join(" "),
                )
                .mono()
                .copyable(),
            )
            .child(
                Field::new(
                    "Cmd",
                    inspect.config.cmd.as_deref().unwrap_or_default().join(" "),
                )
                .mono()
                .copyable(),
            );
        let mut ports = DetailCard::new(IconName::ArrowUpRight, "Ports").empty("No ports");
        for (port, bindings) in inspect.network_settings.ports.iter().flatten() {
            let value = bindings
                .as_ref()
                .filter(|b| !b.is_empty())
                .map(|bindings| {
                    bindings
                        .iter()
                        .map(|b| {
                            if b.host_ip.contains(':') {
                                format!("[{}]:{}", b.host_ip, b.host_port)
                            } else {
                                format!("{}:{}", b.host_ip, b.host_port)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "not published".into());
            ports = ports.child(Field::new(port.clone(), value).mono().copyable());
        }
        let mut mounts = DetailCard::new(IconName::HardDrive, "Mounts").empty("No mounts");
        for (index, mount) in inspect.mounts.iter().enumerate() {
            let mut source = Field::new("Source", mount.source.clone()).mono().copyable();
            if mount.kind == "volume"
                && let Some(name) = mount.name.as_ref().filter(|name| !name.is_empty())
            {
                let name = name.clone();
                source = Field::new("Volume", name.clone()).link(
                    format!("container-volume-{index}"),
                    cx.listener(move |_, _, _, cx| cx.emit(Navigate(Target::Volume(name.clone())))),
                );
            }
            mounts = mounts.child(
                div()
                    .id(("mount", index))
                    .min_w_0()
                    .child(source)
                    .child(
                        Field::new("Destination", mount.destination.clone())
                            .mono()
                            .copyable(),
                    )
                    .child(Field::new(
                        "Access",
                        format!(
                            "{} · {}",
                            mount.kind,
                            if mount.writable {
                                "read/write"
                            } else {
                                "read-only"
                            }
                        ),
                    )),
            );
        }
        let mut networks = DetailCard::new(IconName::Network, "Networks").empty("No networks");
        for (name, network) in inspect.network_settings.networks.iter().flatten() {
            networks = networks.child(
                Field::new(
                    name.clone(),
                    format!("{} · gateway {}", network.address, network.gateway),
                )
                .mono()
                .copyable(),
            );
        }
        let mut environment =
            DetailCard::new(IconName::FileText, "Environment").empty("No variables");
        for (index, env) in inspect
            .config
            .env
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let (key, value) = env.split_once('=').unwrap_or((env, ""));
            let secret = secret_key(key);
            let revealed = detail.revealed.contains(&index);
            // The Field receives only the visible value, so hidden secrets cannot be copied.
            environment = environment.child(
                div()
                    .id(("environment", index))
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div().flex_1().min_w_0().child(
                            Field::new(
                                key.to_owned(),
                                if secret && !revealed {
                                    "••••••••"
                                } else {
                                    value
                                }
                                .to_owned(),
                            )
                            .mono()
                            .copyable(),
                        ),
                    )
                    .when(secret, |el| {
                        el.child(
                            Button::new(
                                ("reveal-env", index),
                                if revealed { "Hide" } else { "Reveal" },
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if let Some(detail) = &mut this.detail
                                        && !detail.revealed.remove(&index)
                                    {
                                        detail.revealed.insert(index);
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                    }),
            )
        }
        let body = body.child(
            grid.card(command)
                .card(ports)
                .card(mounts)
                .card(networks)
                .wide(environment),
        );
        body.into_any_element()
    }
}

fn secret_key(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    [
        "SECRET",
        "TOKEN",
        "PASSWORD",
        "PASSWD",
        "PWD",
        "KEY",
        "COOKIE",
        "SESSION",
        "CONNECTION_STRING",
        "API_KEY",
        "APIKEY",
        "PRIVATE_KEY",
        "CREDENTIAL",
        "AUTH",
        "ACCESS_KEY",
        "DATABASE_URL",
        "DSN",
    ]
    .iter()
    .any(|part| key.contains(part))
}

fn memory_pair(text: &str) -> Option<(f64, f64)> {
    fn bytes(text: &str) -> Option<f64> {
        let text = text.trim();
        let split = text
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(text.len());
        let value: f64 = text[..split].parse().ok()?;
        let scale = match text[split..].trim() {
            "B" | "" => 1.,
            "kB" | "KB" => 1e3,
            "MB" => 1e6,
            "GB" => 1e9,
            "TB" => 1e12,
            "KiB" => 1024.,
            "MiB" => 1024_f64.powi(2),
            "GiB" => 1024_f64.powi(3),
            "TiB" => 1024_f64.powi(4),
            _ => return None,
        };
        (value.is_finite() && value >= 0.).then_some(value * scale)
    }
    let (used, total) = text.split_once('/')?;
    Some((bytes(used)?, bytes(total)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secret_names_and_docker_memory_units() {
        for key in [
            "API_TOKEN",
            "db_password",
            "AWS_ACCESS_KEY_ID",
            "PRIVATE_KEY",
            "DATABASE_URL",
        ] {
            assert!(secret_key(key));
        }
        assert!(!secret_key("APP_MODE"));
        assert_eq!(memory_pair("32MiB / 1GiB"), Some((33554432., 1073741824.)));
        assert_eq!(memory_pair("1MB / 2GB"), Some((1e6, 2e9)));
        assert!(memory_pair("N/A").is_none());
    }
}
