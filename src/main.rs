use colima_gpui::{app, assets::Assets, settings, store, theme, ui};
use gpui::App;
use gpui_platform::application;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod tray;

/// Prints GPUI's and the app's log messages to stderr when `COLIMA_GPUI_LOG` is set:
/// `1` or `info`, or a level (`error`, `warn`, `debug`, `trace`). Off by default.
struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

fn init_debug_log() {
    let Some(value) = std::env::var_os("COLIMA_GPUI_LOG") else {
        return;
    };
    let level = match value.to_string_lossy().to_ascii_lowercase().as_str() {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    };
    static LOG: StderrLog = StderrLog;
    if log::set_logger(&LOG).is_ok() {
        log::set_max_level(level);
        log::info!(
            "debug log on ({level}), Colima GPUI {}",
            env!("CARGO_PKG_VERSION")
        );
    }
}

fn main() {
    init_debug_log();
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
