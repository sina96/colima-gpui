//! macOS menu bar icon, Dock menu and background mode.
//!
//! - **Icon:** shown while Settings' "Keep running in the menu bar" is on, and removed as soon
//!   as it's turned off. It's created on the main thread, inside `application().run`.
//! - **Menu:** one submenu per profile (status dot, Start or Stop and Restart, Open), then
//!   "Open Colima GPUI", "New Profile…", "Settings…" and "Quit Colima GPUI". It's rebuilt when the profiles' rows change, not on
//!   every poll, so an open menu isn't swapped out from under the pointer.
//! - **Events:** muda queues menu clicks on a channel. A foreground task drains it every
//!   100 ms and routes each action through `ProfileStore`, like the window's buttons.
//! - **Dock menu:** right-clicking the Dock icon lists the same profiles and actions, whether
//!   or not the menu bar icon is on. Its items dispatch [`DockCommand`], which runs the same
//!   [`Command`] as a menu bar click.
//! - **Background mode:** with the icon showing, closing the last window hides the Dock icon
//!   (`Accessory`) and keeps the app running; opening the window restores `Regular`.

mod icon;
mod model;

use std::time::Duration;

use colima_gpui::app::{self, APP_NAME};
use colima_gpui::settings;
use colima_gpui::store::{Op, ProfileStore};
use colima_gpui::theme::ActiveTheme;
use gpui::{
    Action, ActivationPolicy, App, AppContext, Context, Entity, Global, Hsla, Rgba, Subscription,
    Task,
};
use tray_icon::menu::{Icon as MenuIcon, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

use model::{Command, Row, State};

const POLL: Duration = Duration::from_millis(100);

struct GlobalTray(Entity<Tray>);
impl Global for GlobalTray {}

/// A Dock menu click. GPUI menus dispatch actions, so this wraps the menu bar's [`Command`].
#[derive(Clone, PartialEq, Action)]
#[action(namespace = colima_gpui, no_json)]
struct DockCommand(Command);

struct Tray {
    icon: Option<TrayIcon>,
    /// The rows the menus were built from; `None` until the first build.
    rows: Option<Vec<Row>>,
    _subscriptions: Vec<Subscription>,
    _events: Task<()>,
}

/// Sets up the menu bar icon and the Dock behaviour. Call once, inside `application().run`.
pub fn init(cx: &mut App) {
    cx.on_action(|DockCommand(command): &DockCommand, cx| run(command.clone(), cx));
    let tray = cx.new(Tray::new);
    cx.set_global(GlobalTray(tray));
}

/// The app was launched again (Finder, Spotlight, the Dock) while running: show the window.
/// Register it with `Application::on_reopen`.
pub fn reopen(cx: &mut App) {
    show_window(None, cx);
}

/// Whether closing the last window should keep the app running: only while the menu bar icon
/// is actually showing, so the app can never be left running with no way back in.
pub fn keeps_running(cx: &App) -> bool {
    cx.try_global::<GlobalTray>()
        .is_some_and(|tray| tray.0.read(cx).icon.is_some())
}

/// The last window closed while running in the menu bar: drop the Dock icon.
pub fn enter_background(cx: &mut App) {
    cx.set_activation_policy(ActivationPolicy::Accessory);
}

impl Tray {
    fn new(cx: &mut Context<Self>) -> Self {
        let store = ProfileStore::global(cx);
        let subscriptions = vec![
            cx.observe(&store, |this, _, cx| this.sync(cx)),
            cx.observe_global::<settings::Settings>(|this, cx| this.sync(cx)),
        ];
        let events = cx.spawn(async move |this, cx| {
            loop {
                let commands = drain_events();
                if !commands.is_empty()
                    && this
                        .update(cx, |_, cx| commands.into_iter().for_each(|c| run(c, cx)))
                        .is_err()
                {
                    break;
                }
                cx.background_executor().timer(POLL).await;
            }
        });
        let mut tray = Self {
            icon: None,
            rows: None,
            _subscriptions: subscriptions,
            _events: events,
        };
        tray.sync(cx);
        tray
    }

    /// Rebuilds the Dock menu when the profiles change, and shows, updates or removes the
    /// menu bar icon to match the setting.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let store = ProfileStore::global(cx).read(cx);
        let rows = model::rows(
            store.instances(),
            |name| store.busy(name),
            |name| store.error(name).is_some(),
        );
        let changed = self.rows.as_ref() != Some(&rows);
        if changed {
            cx.set_dock_menu(dock_menu(&rows));
        }
        if !settings::get(cx).keep_running_in_menu_bar {
            // Dropping the TrayIcon removes it from the menu bar.
            self.icon = None;
        } else if let Some(icon) = &self.icon {
            if changed {
                icon.set_menu(Some(Box::new(build_menu(&rows, cx))));
            }
        } else {
            let menu = build_menu(&rows, cx);
            let built = icon::glyph()
                .and_then(|rgba| {
                    Icon::from_rgba(rgba, icon::GLYPH_PX, icon::GLYPH_PX)
                        .map_err(|error| error.to_string())
                })
                .and_then(|glyph| {
                    TrayIconBuilder::new()
                        .with_icon_templated(glyph)
                        .with_tooltip(APP_NAME)
                        .with_menu(Box::new(menu))
                        .build()
                        .map_err(|error| error.to_string())
                });
            match built {
                Ok(icon) => self.icon = Some(icon),
                Err(error) => eprintln!("colima-gpui: menu bar icon unavailable: {error}"),
            }
        }
        self.rows = Some(rows);
    }
}

/// The Dock icon's right-click menu: one submenu per profile, then "New Profile…". macOS adds
/// its own items (Options, Show All Windows, Hide, Quit) below. GPUI menus have no icons, so
/// the status is in the title only.
fn dock_menu(rows: &[Row]) -> Vec<gpui::MenuItem> {
    use gpui::{Menu as DockMenu, MenuItem as DockItem};
    let item =
        |command: Command, label: &str| DockItem::action(label.to_owned(), DockCommand(command));
    let mut items: Vec<_> = rows
        .iter()
        .map(|row| {
            let name = row.name.clone();
            let idle = row.state != State::Busy;
            let mut actions = if row.state == State::Running {
                vec![
                    item(Command::Stop(name.clone()), "Stop").disabled(!idle),
                    item(Command::Restart(name.clone()), "Restart").disabled(!idle),
                ]
            } else {
                vec![item(Command::Start(name.clone()), "Start").disabled(!idle)]
            };
            actions.push(DockItem::separator());
            actions.push(item(
                Command::Show(Some(name)),
                &format!("Open in {APP_NAME}"),
            ));
            DockItem::submenu(DockMenu::new(row.title.clone()).items(actions))
        })
        .collect();
    if !items.is_empty() {
        items.push(DockItem::separator());
    }
    items.push(item(Command::NewProfile, "New Profile…"));
    items
}

fn build_menu(rows: &[Row], cx: &App) -> Menu {
    let theme = cx.theme();
    let dot = |color: Hsla| {
        let Rgba { r, g, b, a } = color.into();
        let channel = |value: f32| (value * 255.).round() as u8;
        MenuIcon::from_rgba(
            icon::dot([channel(r), channel(g), channel(b), channel(a)]),
            icon::DOT_PX,
            icon::DOT_PX,
        )
        .ok()
    };
    let item = |command: Command, label: &str, enabled: bool| {
        MenuItem::with_id(command.id(), label, enabled, None)
    };
    let menu = Menu::new();
    if rows.is_empty() {
        let _ = menu.append(&MenuItem::new("No profiles", false, None));
    }
    for row in rows {
        let name = row.name.clone();
        let idle = row.state != State::Busy;
        let running = row.state == State::Running;
        let submenu = Submenu::new(&row.title, true);
        submenu.set_icon(dot(match row.state {
            State::Running => theme.success,
            State::Busy => theme.warning,
            State::Failed => theme.error,
            State::Stopped => theme.text_muted,
        }));
        let _ = if running {
            submenu.append_items(&[
                &item(Command::Stop(name.clone()), "Stop", idle),
                &item(Command::Restart(name.clone()), "Restart", idle),
            ])
        } else {
            submenu.append(&item(Command::Start(name.clone()), "Start", idle))
        };
        let _ = submenu.append_items(&[
            &PredefinedMenuItem::separator(),
            &item(
                Command::Show(Some(name)),
                &format!("Open in {APP_NAME}"),
                true,
            ),
        ]);
        let _ = menu.append(&submenu);
    }
    let _ = menu.append_items(&[
        &PredefinedMenuItem::separator(),
        &item(Command::Show(None), &format!("Open {APP_NAME}"), true),
        &item(Command::NewProfile, "New Profile…", true),
        &item(Command::Settings, "Settings…", true),
        &PredefinedMenuItem::separator(),
        &item(Command::Quit, &format!("Quit {APP_NAME}"), true),
    ]);
    menu
}

/// Takes every queued menu click. Icon clicks only open the menu, so they're discarded to
/// keep their channel from growing.
fn drain_events() -> Vec<Command> {
    while TrayIconEvent::receiver().try_recv().is_ok() {}
    std::iter::from_fn(|| MenuEvent::receiver().try_recv().ok())
        .filter_map(|event| Command::parse(&event.id.0))
        .collect()
}

fn run(command: Command, cx: &mut App) {
    let store = ProfileStore::global(cx);
    let op = |op: Op, name: String, cx: &mut App| {
        store.update(cx, |store, cx| store.run(op, name, cx));
    };
    match command {
        Command::Start(name) => op(Op::Start, name, cx),
        Command::Stop(name) => op(Op::Stop, name, cx),
        Command::Restart(name) => op(Op::Restart, name, cx),
        Command::Show(name) => show_window(name, cx),
        Command::Settings => {
            show_window(None, cx);
            store.update(cx, |store, cx| store.select_settings(cx));
        }
        Command::NewProfile => {
            show_window(None, cx);
            store.update(cx, |store, cx| store.select_new_profile(cx));
        }
        Command::Quit => cx.quit(),
    }
}

/// Brings the window back (reopening it if it was closed) and returns the Dock icon.
fn show_window(profile: Option<String>, cx: &mut App) {
    cx.set_activation_policy(ActivationPolicy::Regular);
    match cx.windows().into_iter().next() {
        Some(window) => {
            window
                .update(cx, |_, window, _| window.activate_window())
                .ok();
        }
        None => {
            if let Err(error) = app::open_main_window(cx) {
                eprintln!("colima-gpui: could not open the window: {error}");
            }
        }
    }
    if let Some(name) = profile {
        ProfileStore::global(cx).update(cx, |store, cx| store.select(&name, cx));
    }
    cx.activate(true);
}
