//! Full-window Settings page: a section list on the left, the section on the right.
//!
//! `app.rs` shows it in place of the sidebar and detail pane while `Selection::Settings`
//! is active. "Back" (or Esc) returns to where the user came from.
//! To add a section: a `SettingsSection` variant, its label, and a `render_*` arm.

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, SharedString, Subscription, Window, div,
    prelude::*, px, svg,
};

use crate::app::CloseSettings;
use crate::colima::cli;
use crate::links;
use crate::settings::{self, MAX_POLL_SECS, MIN_POLL_SECS, Palette, ThemeMode};
use crate::store::ProfileStore;
use crate::theme::{ActiveTheme, MONO_FONT, Theme};
use crate::ui::button::{ButtonSize, ButtonStyle};
use crate::ui::tooltip::tooltip;
use crate::ui::{Button, ErrorNote, Icon, IconName, InfoTip, Switch};

pub const KEY_CONTEXT: &str = "Settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Behaviour,
    Help,
    About,
}

impl SettingsSection {
    const ALL: [SettingsSection; 4] = [Self::Appearance, Self::Behaviour, Self::Help, Self::About];

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Behaviour => "Behaviour",
            Self::Help => "Help",
            SettingsSection::About => "About",
        }
    }
}

pub struct SettingsView {
    store: Entity<ProfileStore>,
    section: SettingsSection,
    focus_handle: FocusHandle,
    _observe: Subscription,
    updater: Entity<super::update_badge::Updater>,
    _update_observe: Subscription,
    /// The user clicked "Check for updates" on this page: show its result ("Up to date" or
    /// why it failed) until they switch sections or close Settings.
    show_check_result: bool,
    docker_version: SharedString,
    colima_path: SharedString,
}

impl SettingsView {
    pub fn new(store: Entity<ProfileStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&store, |_, _, cx| cx.notify());
        let focus_handle = cx.focus_handle();
        // Focus the page so Esc reaches it.
        window.focus(&focus_handle, cx);
        let task = cx.background_executor().spawn(async move {
            let path = cli::command("colima")
                .get_program()
                .to_string_lossy()
                .into_owned();
            // Docker can return client JSON even when its daemon is unavailable.
            let version = cli::command("docker")
                .args(["version", "--format", "json"])
                .output()
                .map_err(|error| error.to_string())
                .and_then(|output| docker_client_version(&output.stdout));
            (path, version)
        });
        cx.spawn(async move |this, cx| {
            let (path, version) = task.await;
            this.update(cx, |this, cx| {
                this.colima_path = path.into();
                this.docker_version = version
                    .unwrap_or_else(|error| format!("Unavailable: {error}"))
                    .into();
                cx.notify();
            })
            .ok();
        })
        .detach();
        let updater = super::update_badge::Updater::global(cx);
        let update_observe = cx.observe(&updater, |_, _, cx| cx.notify());
        Self {
            updater,
            _update_observe: update_observe,
            show_check_result: false,
            store,
            section: SettingsSection::Appearance,
            focus_handle,
            _observe: observe,
            docker_version: "Loading…".into(),
            colima_path: "Loading…".into(),
        }
    }

    fn close(&mut self, _: &CloseSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| store.close_settings(cx));
    }

    fn render_about(&self, theme: &Theme, cx: &mut Context<Self>) -> Vec<AnyRow> {
        let mut rows = self.update_rows(theme, cx);
        let version = self.store.read(cx).version().cloned();
        rows.extend([
            row(
                theme,
                "Colima version",
                version
                    .as_ref()
                    .map_or("unknown".into(), |v| v.version.clone().into()),
                false,
            ),
            row(
                theme,
                "Colima commit",
                version
                    .and_then(|v| v.commit)
                    .map_or("unknown".into(), SharedString::from),
                true,
            ),
            row(
                theme,
                "Docker client version",
                self.docker_version.clone(),
                false,
            ),
            row(theme, "Colima binary", self.colima_path.clone(), true),
            section_heading(theme, "Project"),
            link_tiles(theme, links::PROJECT),
        ]);
        rows
    }

    /// Documentation for Colima and the tools it builds on, as tiles.
    fn render_help(&self, theme: &Theme) -> Vec<AnyRow> {
        vec![
            div()
                .pb_3()
                .text_sm()
                .text_color(theme.text_muted)
                .child(
                    "Colima runs Linux VMs with Lima, and Docker, containerd or Kubernetes inside them.",
                )
                .into_any_element(),
            link_tiles(theme, links::DOCS),
        ]
    }

    fn render_appearance(&self, theme: &Theme, cx: &mut Context<Self>) -> Vec<AnyRow> {
        let current = settings::get(cx).theme;
        let palette = settings::get(cx).palette;
        let palettes = [("Colima", Palette::Colima), ("Zed One", Palette::Zed)];
        let colors = setting_row(
            theme,
            setting_label(
                "color-profile-info",
                "Color profile",
                "Colima uses the colors of colima.run. Zed One matches Zed. Both follow the theme below.",
            ),
            div()
                .flex()
                .gap_1()
                .children(palettes.into_iter().map(|(label, choice)| {
                    Button::new(label, label)
                        .style(if choice == palette {
                            ButtonStyle::Selected
                        } else {
                            ButtonStyle::Ghost
                        })
                        .on_click(cx.listener(move |_, _, _, cx| {
                            settings::update(cx, |settings| settings.palette = choice);
                            cx.notify();
                        }))
                })),
        );
        let modes = [
            ("System", ThemeMode::System),
            ("Light", ThemeMode::Light),
            ("Dark", ThemeMode::Dark),
        ];
        vec![
            colors,
            setting_row(
                theme,
                setting_label(
                    "theme-info",
                    "Theme",
                    "System follows your operating system’s appearance.",
                ),
                div()
                    .flex()
                    .gap_1()
                    .children(modes.into_iter().map(|(label, mode)| {
                        Button::new(label, label)
                            .style(if mode == current {
                                ButtonStyle::Selected
                            } else {
                                ButtonStyle::Ghost
                            })
                            .on_click(cx.listener(move |_, _, _, cx| {
                                settings::update(cx, |settings| settings.theme = mode);
                                cx.notify();
                            }))
                    })),
            ),
        ]
    }

    /// The "App version" row with "Check for updates", then an available update (version,
    /// notes, "Install and restart") or the last install error.
    fn update_rows(&self, theme: &Theme, cx: &mut Context<Self>) -> Vec<AnyRow> {
        let updater = self.updater.read(cx);
        let enabled = crate::update::enabled();
        let busy = updater.busy;
        // After a check: a tick when up to date, or why it failed. An available update shows
        // as the panel below instead.
        // Only for a check started here; an available update shows as the panel below.
        let checked = self.show_check_result
            && updater.last_check.is_some()
            && updater.release.is_none()
            && !busy;
        let up_to_date = checked && updater.last_error.is_none();
        let check_error = updater.last_error.clone().filter(|_| checked);
        let mut rows = vec![value_row(
            theme,
            "App version",
            div()
                .flex()
                .items_center()
                .gap_3()
                .min_w_0()
                .child(env!("CARGO_PKG_VERSION"))
                .child(
                    Button::new("update-check", "Check for updates")
                        .loading(busy)
                        .disabled(!enabled)
                        .tooltip(if enabled {
                            "Check GitHub Releases for a newer version"
                        } else {
                            "Only release builds from GitHub can update themselves"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_check_result = true;
                            this.updater
                                .update(cx, |updater, cx| updater.check(true, cx));
                        })),
                )
                .when(up_to_date, |el| {
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .text_color(theme.success)
                            .child(
                                Icon::new(IconName::Check)
                                    .size(px(14.))
                                    .color(theme.success),
                            )
                            .child("Up to date"),
                    )
                })
                .when_some(check_error, |el, error| {
                    el.child(div().min_w_0().text_xs().child(
                        ErrorNote::new("update-check-error", "Couldn't check").details(error),
                    ))
                }),
        )];
        if let Some(error) = updater.install_error.clone() {
            rows.push(
                div()
                    .py_2()
                    .text_sm()
                    .child(
                        ErrorNote::new("update-error", "The update wasn't installed")
                            .details(error),
                    )
                    .into_any_element(),
            );
        }
        if let Some(release) = updater.release.clone() {
            let url = release.html_url.clone();
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .my_2()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.accent)
                    .bg(theme.element_selected)
                    .text_sm()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(format!(
                                "Colima GPUI {} is available",
                                release.version_text()
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(Button::new("update-github", "View on GitHub").on_click(
                                        move |_, _, cx| {
                                            if url.starts_with("https://") {
                                                cx.open_url(&url);
                                            }
                                        },
                                    ))
                                    .child(
                                        Button::new("update-install", "Install and restart")
                                            .style(ButtonStyle::Primary)
                                            .loading(busy)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.updater
                                                    .update(cx, |updater, cx| updater.install(cx));
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("release-notes")
                            .max_h(px(200.))
                            .overflow_y_scroll()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(release.body.unwrap_or_else(|| "No release notes.".into())),
                    )
                    .into_any_element(),
            );
        }
        rows
    }

    fn render_behaviour(&self, theme: &Theme, cx: &mut Context<Self>) -> Vec<AnyRow> {
        let settings = settings::get(cx).clone();
        vec![
            setting_row(
                theme,
                setting_label(
                    "poll-info",
                    "Poll interval",
                    "Refresh profiles and visible Docker tabs every 2–60 seconds.",
                ),
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("poll-less", "−")
                            .disabled(settings.poll_secs <= MIN_POLL_SECS)
                            .tooltip("Decrease by one second")
                            .on_click(cx.listener(|_, _, _, cx| {
                                settings::update(cx, |settings| {
                                    settings.poll_secs = settings.poll_secs.saturating_sub(1)
                                });
                                cx.notify();
                            })),
                    )
                    .child(format!("{} s", settings.poll_secs))
                    .child(
                        Button::new("poll-more", "+")
                            .disabled(settings.poll_secs >= MAX_POLL_SECS)
                            .tooltip("Increase by one second")
                            .on_click(cx.listener(|_, _, _, cx| {
                                settings::update(cx, |settings| settings.poll_secs += 1);
                                cx.notify();
                            })),
                    ),
            ),
            setting_row(
                theme,
                setting_label(
                    "auto-trim-info",
                    "Return free space after cleanup",
                    "Run fstrim after every prune to return unused VM disk blocks to macOS. Nothing else is deleted.",
                ),
                Switch::new("auto-trim", settings.trim_after_prune).on_toggle(cx.listener(
                    |_, on: &bool, _, cx| {
                        let on = *on;
                        settings::update(cx, |settings| settings.trim_after_prune = on);
                        cx.notify();
                    },
                )),
            ),
            setting_row(
                theme,
                setting_label(
                    "menu-bar-info",
                    "Menu bar",
                    if cfg!(target_os = "macos") {
                        "Show Colima GPUI in the menu bar, and keep it running there when the window closes."
                    } else {
                        "Keep running in the menu bar when the window closes. Not available on this platform yet."
                    },
                ),
                Switch::new("keep-running", settings.keep_running_in_menu_bar)
                    .disabled(!cfg!(target_os = "macos"))
                    .on_toggle(cx.listener(|_, on: &bool, _, cx| {
                        let on = *on;
                        settings::update(cx, |settings| settings.keep_running_in_menu_bar = on);
                        cx.notify();
                    })),
            ),
            setting_row(
                theme,
                setting_label(
                    "auto-updates-info",
                    "Automatic updates",
                    "Check GitHub Releases at launch and every 12 hours. Installation always needs your approval.",
                ),
                Switch::new("auto-updates", settings.check_for_updates).on_toggle(cx.listener(
                    |_, on: &bool, _, cx| {
                        let on = *on;
                        settings::update(cx, |settings| settings.check_for_updates = on);
                        cx.notify();
                    },
                )),
            ),
        ]
    }
}

fn docker_client_version(bytes: &[u8]) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    value
        .get("Client")
        .and_then(|client| client.get("Version"))
        .and_then(|version| version.as_str())
        .filter(|version| !version.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Docker returned no client version".into())
}

fn setting_label(
    id: &'static str,
    label: &'static str,
    explanation: &'static str,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(label)
        .child(InfoTip::new(id, explanation))
}

fn setting_row(theme: &Theme, label: impl IntoElement, control: impl IntoElement) -> AnyRow {
    div()
        .flex()
        .items_center()
        .min_h(px(36.))
        .gap_2()
        .border_b_1()
        .border_color(theme.border_variant)
        .text_sm()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(theme.text_muted)
                .child(label),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

type AnyRow = gpui::AnyElement;

fn row(theme: &Theme, label: &'static str, value: SharedString, mono: bool) -> AnyRow {
    value_row(
        theme,
        label,
        div()
            .truncate()
            .when(mono, |el| el.font_family(MONO_FONT))
            .child(value),
    )
}

/// An About row: label column, then the value (text or a link) left-aligned beside it.
/// A small uppercase heading inside a section, like the section title.
fn section_heading(theme: &Theme, text: &'static str) -> AnyRow {
    div()
        .pt_5()
        .pb_2()
        .text_xs()
        .text_color(theme.text_muted)
        .child(text.to_uppercase())
        .into_any_element()
}

/// A wrapping grid of link tiles: a one-color logo over a caption, opening the link in the
/// browser. The summary is the tooltip.
fn link_tiles(theme: &Theme, links: &'static [links::Link]) -> AnyRow {
    div()
        .flex()
        .flex_wrap()
        .gap_2()
        .children(links.iter().map(|link| {
            let group = SharedString::from(format!("tile-{}", link.title));
            let theme = theme.clone();
            div()
                .id(link.title)
                .group(group.clone())
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .w(px(112.))
                .h(px(84.))
                .rounded_md()
                .border_1()
                .border_color(theme.border_variant)
                .bg(theme.surface_background)
                .cursor_pointer()
                .hover(|s| s.bg(theme.element_hover).border_color(theme.border))
                .focusable()
                .tab_stop(true)
                .focus_visible(|s| s.border_color(theme.border_focused))
                .tooltip(tooltip(link.summary))
                .on_click(move |_, _, cx| cx.open_url(link.url))
                .child(
                    svg()
                        .path(link.logo)
                        .size(px(28.))
                        .flex_none()
                        .text_color(theme.text_muted)
                        .group_hover(group.clone(), |s| s.text_color(theme.text)),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.text_muted)
                        .group_hover(group, |s| s.text_color(theme.text))
                        .child(link.title),
                )
        }))
        .into_any_element()
}

fn value_row(theme: &Theme, label: &'static str, value: impl IntoElement) -> AnyRow {
    div()
        .flex()
        .items_center()
        .min_h(px(32.))
        .border_b_1()
        .border_color(theme.border_variant)
        .text_sm()
        .child(
            div()
                .w(px(180.))
                .flex_none()
                .text_color(theme.text_muted)
                .child(label),
        )
        .child(div().flex().flex_1().min_w_0().child(value))
        .into_any_element()
}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.updater.update(cx, |updater, _| {
            // Opened by the update badge or "Check for Updates…": show that check's result.
            if updater.open_updates {
                self.section = SettingsSection::About;
                self.show_check_result = true;
                updater.open_updates = false;
            }
        });
        let theme = cx.theme().clone();
        let rows = match self.section {
            SettingsSection::Appearance => self.render_appearance(&theme, cx),
            SettingsSection::Behaviour => self.render_behaviour(&theme, cx),
            SettingsSection::Help => self.render_help(&theme),
            SettingsSection::About => self.render_about(&theme, cx),
        };

        let nav = SettingsSection::ALL.into_iter().map(|section| {
            let selected = section == self.section;
            div()
                .id(section.label())
                .flex()
                .items_center()
                .h(px(28.))
                .px_3()
                .text_sm()
                .cursor_pointer()
                .text_color(if selected {
                    theme.text
                } else {
                    theme.text_muted
                })
                .when(selected, |el| el.bg(theme.element_selected))
                .when(!selected, |el| {
                    el.hover(|s| s.bg(theme.ghost_element_hover).text_color(theme.text))
                })
                .child(section.label())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.section = section;
                    this.show_check_result = false;
                    cx.notify();
                }))
        });

        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::close))
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
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border_variant)
                    .child(
                        Button::new("settings-back", "← Back")
                            .size(ButtonSize::Medium)
                            .tooltip("Back (Esc)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close(&CloseSettings, window, cx)
                            })),
                    )
                    .child(div().text_lg().child("Settings")),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .w(px(200.))
                            .py_2()
                            .bg(theme.surface_background)
                            .border_r_1()
                            .border_color(theme.border_variant)
                            .children(nav),
                    )
                    .child(
                        div()
                            .id("settings-section")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .p_4()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .pb_2()
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child(self.section.label().to_uppercase()),
                            )
                            // Keeps right-aligned controls near their labels on wide windows.
                            .child(div().flex().flex_col().max_w(px(640.)).children(rows)),
                    ),
            )
    }
}
