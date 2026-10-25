//! Root view: Zed-style title bar, sidebar + detail, status bar. Also app actions and menus.

use gpui::{
    App, Bounds, Context, Entity, KeyBinding, Menu, MenuItem, MouseButton, PromptLevel,
    Subscription, SystemMenuType, TitlebarOptions, Window, WindowBounds, WindowControlArea,
    WindowHandle, WindowOptions, actions, div, point, prelude::*, px, size,
};

use crate::links;
use crate::settings::{self, Palette, Settings, ThemeMode};
use crate::store::{ProfileStore, Selection};
use crate::theme::{self, ActiveTheme, UI_FONT};
use crate::views::{
    profile_detail::{ProfileDetail, ProfileTab},
    profile_list::ProfileList,
    settings::SettingsView,
    update_badge::UpdateBadge,
};

actions!(
    colima_gpui,
    [
        About,
        CheckForUpdates,
        Refresh,
        Quit,
        OpenSettings,
        CloseSettings,
        Hide,
        HideOthers,
        ShowAll,
        ShowOverview,
        ShowContainers,
        ShowImages,
        ShowVolumes,
        ShowCleanup,
        ShowAi,
        AppearanceSystem,
        AppearanceLight,
        AppearanceDark,
        PaletteZed,
        PaletteColima,
        Minimize,
        Zoom,
        CloseWindow,
    ]
);

/// Opens a web page from the Help menu. Its URL comes from `crate::links`.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = colima_gpui, no_json)]
pub struct OpenLink(pub &'static str);

/// Shown in the app menu, the About box, the window list and the menu bar icon. The window
/// itself doesn't show it.
pub const APP_NAME: &str = "Colima GPUI";

/// Matches Zed's macOS title bar (`platform_title_bar_height`).
const TITLE_BAR_HEIGHT: f32 = 32.;
/// Clears the traffic lights, as Zed's `TRAFFIC_LIGHT_PADDING` does.
const TRAFFIC_LIGHT_PADDING: f32 = 71.;
const SIDEBAR_WIDTH: f32 = 220.;

/// Registers app-level actions, key bindings and the macOS menu bar.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Refresh, cx| {
        ProfileStore::global(cx).update(cx, |store, cx| store.refresh(cx));
    });
    cx.on_action(about);
    // An absent handler also disables the native menu item.
    if crate::update::enabled() {
        cx.on_action(|_: &CheckForUpdates, cx| {
            if cx.windows().is_empty() {
                let _ = open_main_window(cx);
            }
            crate::views::update_badge::show_updates(cx);
            crate::views::update_badge::Updater::global(cx)
                .update(cx, |updater, cx| updater.check(true, cx));
        });
    }
    cx.on_action(|_: &OpenSettings, cx| {
        ProfileStore::global(cx).update(cx, |store, cx| store.select_settings(cx));
    });
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    let appearance = |mode: ThemeMode| move |cx: &mut App| settings::update(cx, |s| s.theme = mode);
    cx.on_action(move |_: &AppearanceSystem, cx| appearance(ThemeMode::System)(cx));
    cx.on_action(move |_: &AppearanceLight, cx| appearance(ThemeMode::Light)(cx));
    cx.on_action(move |_: &AppearanceDark, cx| appearance(ThemeMode::Dark)(cx));
    let palette =
        |palette: Palette| move |cx: &mut App| settings::update(cx, |s| s.palette = palette);
    cx.on_action(move |_: &PaletteZed, cx| palette(Palette::Zed)(cx));
    cx.on_action(move |_: &PaletteColima, cx| palette(Palette::Colima)(cx));
    cx.on_action(|OpenLink(url): &OpenLink, cx| cx.open_url(url));
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-r", Refresh, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-1", ShowOverview, None),
        KeyBinding::new("cmd-2", ShowContainers, None),
        KeyBinding::new("cmd-3", ShowImages, None),
        KeyBinding::new("cmd-4", ShowVolumes, None),
        KeyBinding::new("cmd-5", ShowCleanup, None),
        KeyBinding::new("cmd-6", ShowAi, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new(
            "escape",
            CloseSettings,
            Some(crate::views::settings::KEY_CONTEXT),
        ),
    ]);
    set_menus(cx);
    // Re-tick View → Appearance when the palette or theme mode changes.
    cx.observe_global::<Settings>(set_menus).detach();
}

/// The macOS menu bar. Items whose action has no handler in the focused window (e.g.
/// Containers for a stopped profile) are greyed out by GPUI.
fn set_menus(cx: &mut App) {
    let mode = settings::get(cx).theme;
    let palette = settings::get(cx).palette;
    cx.set_menus([
        Menu::new(APP_NAME).items([
            MenuItem::action(format!("About {APP_NAME}"), About),
            MenuItem::action("Check for Updates…", CheckForUpdates)
                .disabled(!crate::update::enabled()),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action(format!("Hide {APP_NAME}"), Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action(format!("Quit {APP_NAME}"), Quit),
        ]),
        Menu::new("View").items([
            MenuItem::action("Overview", ShowOverview),
            MenuItem::action("Containers", ShowContainers),
            MenuItem::action("Images", ShowImages),
            MenuItem::action("Volumes", ShowVolumes),
            MenuItem::action("Cleanup", ShowCleanup),
            MenuItem::action("AI", ShowAi),
            MenuItem::separator(),
            MenuItem::action("Refresh", Refresh),
            MenuItem::separator(),
            MenuItem::submenu(Menu::new("Appearance").items([
                MenuItem::action("System", AppearanceSystem).checked(mode == ThemeMode::System),
                MenuItem::action("Light", AppearanceLight).checked(mode == ThemeMode::Light),
                MenuItem::action("Dark", AppearanceDark).checked(mode == ThemeMode::Dark),
            ])),
            MenuItem::submenu(Menu::new("Color Profile").items([
                MenuItem::action("Colima", PaletteColima).checked(palette == Palette::Colima),
                MenuItem::action("Zed One", PaletteZed).checked(palette == Palette::Zed),
            ])),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
            MenuItem::separator(),
            MenuItem::action("Close", CloseWindow),
        ]),
        Menu::new("Help").items(help_menu()),
    ]);
}

/// The project's pages, then the documentation links from Settings → Help.
fn help_menu() -> Vec<MenuItem> {
    let item = |link: &links::Link| MenuItem::action(link.menu, OpenLink(link.url));
    links::PROJECT
        .iter()
        .map(item)
        .chain([MenuItem::separator()])
        .chain(links::DOCS.iter().map(item))
        .collect()
}

fn about(_: &About, cx: &mut App) {
    let colima = ProfileStore::global(cx)
        .read(cx)
        .version()
        .map_or("colima version unknown".to_owned(), |v| {
            format!("colima {}", v.version)
        });
    let detail = format!("Version {}\n{colima}", env!("CARGO_PKG_VERSION"));
    if let Some(window) = cx.active_window() {
        window
            .update(cx, |_, window, cx| {
                // Fire-and-forget: the only answer is OK.
                drop(window.prompt(PromptLevel::Info, APP_NAME, Some(&detail), &["OK"], cx));
            })
            .ok();
    }
}

/// Opens the main window. Also used to reopen it from the menu bar.
pub fn open_main_window(cx: &mut App) -> gpui::Result<WindowHandle<ColimaApp>> {
    let store = ProfileStore::global(cx);
    let bounds = Bounds::centered(None, size(px(1000.), px(640.)), cx);
    let window = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(640.), px(400.))),
            titlebar: Some(TitlebarOptions {
                title: Some(APP_NAME.into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(9.), px(9.))),
            }),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| ColimaApp::new(store, window, cx)),
    )?;
    cx.activate(true);
    Ok(window)
}

pub struct ColimaApp {
    store: Entity<ProfileStore>,
    sidebar: Entity<ProfileList>,
    detail: Entity<ProfileDetail>,
    update_badge: Entity<UpdateBadge>,
    /// Full-window Settings page; exists only while it's open.
    settings: Option<Entity<SettingsView>>,
    /// Set on mouse-down in the title bar; the next mouse-move starts a window drag.
    should_move: bool,
    _subscriptions: Vec<Subscription>,
}

impl ColimaApp {
    pub fn new(store: Entity<ProfileStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.observe_window_appearance(window, |_, window, cx| {
                if theme::sync_appearance(window.appearance(), cx) {
                    cx.notify();
                }
            }),
            cx.observe(&store, |_, _, cx| cx.notify()),
        ];
        Self {
            sidebar: cx.new(|cx| ProfileList::new(store.clone(), cx)),
            detail: cx.new(|cx| ProfileDetail::new(store.clone(), cx)),
            update_badge: cx.new(UpdateBadge::new),
            settings: None,
            store,
            should_move: false,
            _subscriptions: subscriptions,
        }
    }

    fn show_tab(&mut self, tab: ProfileTab, cx: &mut Context<Self>) {
        self.detail
            .update(cx, |detail, cx| detail.show_tab(tab, cx));
    }

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        div()
            .id("title-bar")
            .window_control_area(WindowControlArea::Drag)
            .flex()
            .items_center()
            .flex_none()
            .w_full()
            .h(px(TITLE_BAR_HEIGHT))
            .pl(px(TRAFFIC_LIGHT_PADDING))
            .bg(theme.title_bar_background)
            .border_b_1()
            .border_color(theme.border_variant)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.should_move = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.should_move = false),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, _| this.should_move = false))
            .on_mouse_move(cx.listener(|this, _, window, _| {
                if this.should_move {
                    this.should_move = false;
                    window.start_window_move();
                }
            }))
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let store = self.store.read(cx);
        let instances = store.instances();
        let running = instances.iter().filter(|i| i.is_running()).count();
        let summary = format!("{} profiles · {running} running", instances.len());
        let update_badge = self.update_badge.clone();
        let theme = cx.theme();
        div()
            .flex()
            .items_center()
            .justify_between()
            .flex_none()
            .h(px(26.))
            .px_2()
            .bg(theme.status_bar_background)
            .border_t_1()
            .border_color(theme.border_variant)
            .text_xs()
            .text_color(theme.text_muted)
            .child(summary)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(update_badge)
                    .child(concat!("v", env!("CARGO_PKG_VERSION"))),
            )
    }
}

impl Render for ColimaApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if *self.store.read(cx).selection() == Selection::Settings {
            if self.settings.is_none() {
                let store = self.store.clone();
                self.settings = Some(cx.new(|cx| SettingsView::new(store, window, cx)));
            }
        } else {
            self.settings = None;
        }
        let title_bar = self.render_title_bar(cx);
        let status_bar = self.render_status_bar(cx);
        let theme = cx.theme();

        let store = self.store.read(cx);
        // The profile tabs apply while a profile is showing; the Docker ones only while it
        // runs Docker (the same rule as the tab row in `ProfileDetail`).
        let profile = matches!(store.selection(), Selection::Profile(_));
        let docker = profile
            && store
                .selected_instance()
                .is_some_and(|i| i.is_running() && i.is_docker());

        div()
            .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
            .on_action(cx.listener(|_, _: &Zoom, window, _| window.zoom_window()))
            .on_action(cx.listener(|_, _: &CloseWindow, window, _| window.remove_window()))
            .when(profile, |el| {
                el.on_action(cx.listener(|this, _: &ShowOverview, _, cx| {
                    this.show_tab(ProfileTab::Overview, cx)
                }))
                .on_action(cx.listener(|this, _: &ShowAi, _, cx| this.show_tab(ProfileTab::Ai, cx)))
            })
            .when(docker, |el| {
                el.on_action(cx.listener(|this, _: &ShowContainers, _, cx| {
                    this.show_tab(ProfileTab::Containers, cx)
                }))
                .on_action(
                    cx.listener(|this, _: &ShowImages, _, cx| {
                        this.show_tab(ProfileTab::Images, cx)
                    }),
                )
                .on_action(cx.listener(|this, _: &ShowVolumes, _, cx| {
                    this.show_tab(ProfileTab::Volumes, cx)
                }))
                .on_action(cx.listener(|this, _: &ShowCleanup, _, cx| {
                    this.show_tab(ProfileTab::Cleanup, cx)
                }))
            })
            .flex()
            .flex_col()
            .size_full()
            .font_family(UI_FONT)
            .text_color(theme.text)
            .bg(theme.editor_background)
            .child(title_bar)
            .when_some(self.settings.clone(), |el, settings| {
                el.child(div().flex_1().min_h_0().child(settings))
            })
            .when(self.settings.is_none(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .child(
                            div()
                                .flex_none()
                                .w(px(SIDEBAR_WIDTH))
                                .h_full()
                                .border_r_1()
                                .border_color(theme.border_variant)
                                .child(self.sidebar.clone()),
                        )
                        .child(div().flex_1().min_w_0().h_full().child(self.detail.clone())),
                )
            })
            .child(status_bar)
    }
}
