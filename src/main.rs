use colima_gpui::{app, assets::Assets, settings, store, theme, ui};
use gpui::App;
use gpui_platform::application;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod tray;

fn main() {
    let application = application().with_assets(Assets);
    #[cfg(target_os = "macos")]
    application.on_reopen(tray::reopen);
    application.run(|cx: &mut App| {
        settings::init(cx);
        theme::init(cx);
        ui::init(cx);
        app::init(cx);
        store::init(cx);
        #[cfg(target_os = "macos")]
        tray::init(cx);

        // Closing the last window quits, unless the app keeps running in the menu bar.
        cx.on_window_closed(|cx, _| {
            if !cx.windows().is_empty() {
                return;
            }
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            if tray::keeps_running(cx) {
                tray::enter_background(cx);
                return;
            }
            cx.quit();
        })
        .detach();

        app::open_main_window(cx).expect("failed to open window");
        // The Linux tray restores a missing window when disabled or unavailable.
        // Give it the startup window before initializing that fallback.
        #[cfg(target_os = "linux")]
        tray::init(cx);
    });
}
