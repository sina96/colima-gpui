//! UI adapter for the plain Rust updater. Its global entity survives window closure.
use crate::{settings, store::ProfileStore, theme::ActiveTheme, ui::Button, update};
use gpui::{App, Context, Entity, Global, Subscription, Window, div, prelude::*};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct UpdateGlobal(Entity<Updater>);
impl Global for UpdateGlobal {}

fn launch_error() -> Option<String> {
    let args: Vec<_> = std::env::args().collect();
    if let Some(ix) = args.iter().position(|a| a == "--colima-update-failed") {
        let reason = args
            .get(ix + 1)
            .map_or("The update could not be installed.", String::as_str);
        Some(format!("{reason} The previous app was restored."))
    } else {
        None
    }
}

pub struct Updater {
    pub release: Option<update::Release>,
    pub result: String,
    pub install_error: Option<String>,
    pub last_check: Option<SystemTime>,
    pub busy: bool,
    pub open_updates: bool,
    last_attempt: Option<Instant>,
    manual: bool,
}
impl Updater {
    pub fn global(cx: &mut App) -> Entity<Self> {
        if !cx.has_global::<UpdateGlobal>() {
            let entity = cx.new(|cx: &mut Context<Self>| {
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_secs(5)).await;
                    loop {
                        if this
                            .update(cx, |this, cx| {
                                if update::enabled()
                                    && settings::get(cx).check_for_updates
                                    && this.last_attempt.is_none_or(|t| {
                                        t.elapsed() >= Duration::from_secs(12 * 60 * 60)
                                    })
                                {
                                    this.check(false, cx);
                                }
                            })
                            .is_err()
                        {
                            break;
                        }
                        cx.background_executor()
                            .timer(Duration::from_secs(30))
                            .await;
                    }
                })
                .detach();
                Self {
                    release: None,
                    result: "Not checked yet".into(),
                    install_error: launch_error(),
                    last_check: None,
                    busy: false,
                    open_updates: false,
                    last_attempt: None,
                    manual: false,
                }
            });
            cx.set_global(UpdateGlobal(entity));
        }
        cx.global::<UpdateGlobal>().0.clone()
    }
    pub fn check(&mut self, manual: bool, cx: &mut Context<Self>) {
        self.manual |= manual;
        if self.busy || !update::enabled() {
            return;
        }
        self.busy = true;
        self.last_attempt = Some(Instant::now());
        let task = cx.background_executor().spawn(async { update::check() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.busy = false;
                this.last_check = Some(SystemTime::now());
                match result {
                    Ok(release) => {
                        this.result = if release.is_some() {
                            "Update available"
                        } else {
                            "You're up to date"
                        }
                        .into();
                        this.release = release;
                    }
                    Err(error) => {
                        this.result = if this.manual {
                            error
                        } else {
                            "Automatic check did not complete; try Check now".into()
                        };
                    }
                }
                this.manual = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
    pub fn install(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(release) = self.release.clone() else {
            return;
        };
        self.busy = true;
        self.install_error = None;
        self.result = "Downloading and verifying update…".into();
        let task = cx
            .background_executor()
            .spawn(async move { update::install(&release) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(()) => cx.quit(),
                Err(error) => {
                    this.busy = false;
                    this.install_error = Some(error);
                    this.result = "Update was not installed".into();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
    pub fn checked_text(&self) -> String {
        self.last_check.map_or_else(
            || "Never".into(),
            |time| {
                // UTC avoids adding a date/time dependency.
                let secs = time
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let days = secs / 86400;
                let z = days as i64 + 719468;
                let era = z / 146097;
                let doe = z - era * 146097;
                let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                let mut year = yoe + era * 400;
                let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                let mp = (5 * doy + 2) / 153;
                let day = doy - (153 * mp + 2) / 5 + 1;
                let month = mp + if mp < 10 { 3 } else { -9 };
                year += i64::from(month <= 2);
                format!(
                    "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
                    secs / 3600 % 24,
                    secs / 60 % 60
                )
            },
        )
    }
}
pub fn show_updates(cx: &mut App) {
    Updater::global(cx).update(cx, |this, cx| {
        this.open_updates = true;
        cx.notify();
    });
    ProfileStore::global(cx).update(cx, |store, cx| store.select_settings(cx));
}
pub struct UpdateBadge {
    updater: Entity<Updater>,
    _observe: Subscription,
}
impl UpdateBadge {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let updater = Updater::global(cx);
        let observe = cx.observe(&updater, |_, _, cx| cx.notify());
        if updater.read(cx).install_error.is_some() {
            show_updates(cx);
        }
        // Acknowledge only after the replacement has created its main view.
        cx.background_executor()
            .spawn(async {
                update::acknowledge_launch();
            })
            .detach();
        Self {
            updater,
            _observe: observe,
        }
    }
}
impl Render for UpdateBadge {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div().text_color(theme.text_muted).when_some(
            self.updater.read(cx).release.as_ref(),
            |el, release| {
                el.child(
                    Button::new("update-badge", format!("Update {}", release.tag_name))
                        .on_click(|_, _, cx| show_updates(cx)),
                )
            },
        )
    }
}
