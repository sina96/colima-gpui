//! User settings, persisted as JSON and exposed as a GPUI global.
//!
//! macOS: `~/Library/Application Support/colima-gpui/settings.json`
//! Linux: `$XDG_CONFIG_HOME/colima-gpui/settings.json` (default `~/.config`)
//!
//! A missing or invalid file yields defaults; unknown or missing fields fall back
//! per field, so adding a setting never invalidates an existing file.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{App, Global, Task};
use serde::{Deserialize, Serialize};

pub const MIN_POLL_SECS: u64 = 2;
pub const MAX_POLL_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follow the OS appearance.
    #[default]
    System,
    Light,
    Dark,
}

/// The color profile. Each comes in a light and a dark variant, picked by [`ThemeMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Palette {
    /// colima.run's greys and mint whites, with Colima green for filled controls.
    #[default]
    Colima,
    /// Zed's One Dark / One Light.
    Zed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub palette: Palette,
    /// How often profiles (and visible Docker tabs) refresh.
    pub poll_secs: u64,
    /// Show the menu bar icon, and keep running there when the window closes.
    pub keep_running_in_menu_bar: bool,
    /// Check GitHub Releases for updates at launch and every 12 hours (release builds only).
    pub check_for_updates: bool,
    /// Return unused VM blocks to the host after Docker cleanup.
    pub trim_after_prune: bool,
    /// Last folder the AI tab downloaded the Colima agent skill into.
    pub skill_folder: Option<std::path::PathBuf>,
    /// Profiles started when the app opens (if stopped), in the order they were turned on.
    pub start_on_open: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            palette: Palette::Colima,
            poll_secs: 5,
            keep_running_in_menu_bar: false,
            check_for_updates: true,
            trim_after_prune: true,
            skill_folder: None,
            start_on_open: Vec::new(),
        }
    }
}

impl Global for Settings {}

impl Settings {
    pub fn starts_on_open(&self, profile: &str) -> bool {
        self.start_on_open.iter().any(|name| name == profile)
    }

    /// Adds or removes `profile` from the profiles started when the app opens.
    /// Turning it on again keeps the profile's place in the order.
    pub fn set_start_on_open(&mut self, profile: &str, on: bool) {
        if !on {
            self.start_on_open.retain(|name| name != profile);
        } else if !self.starts_on_open(profile) {
            self.start_on_open.push(profile.to_owned());
        }
    }
}

#[derive(Default)]
struct PendingSave(Option<Task<()>>);
impl Global for PendingSave {}

impl Settings {
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.poll_secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS))
    }

    /// Reads settings, falling back to defaults on any error.
    pub fn load_from(path: &Path) -> Self {
        let mut settings: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        settings.poll_secs = settings.poll_secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS);
        settings
    }

    /// Writes atomically (temp file + rename) so a crash never leaves a torn file.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
    }
}

/// Location of `settings.json`, or `None` when no home directory is known.
pub fn settings_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home?.join("Library/Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".config")))?
    };
    Some(base.join("colima-gpui").join("settings.json"))
}

/// Loads settings into the global. Call once at startup, before other `init`s.
pub fn init(cx: &mut App) {
    let settings = settings_path()
        .map(|p| Settings::load_from(&p))
        .unwrap_or_default();
    cx.set_global(settings);
    cx.set_global(PendingSave::default());
}

pub fn get(cx: &App) -> &Settings {
    cx.global::<Settings>()
}

/// Applies `change` to the global and saves it in the background.
pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    let old = (get(cx).theme, get(cx).palette);
    change(cx.global_mut::<Settings>());
    let settings = cx.global_mut::<Settings>();
    settings.poll_secs = settings.poll_secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS);
    let settings = settings.clone();
    if (settings.theme, settings.palette) != old
        && crate::theme::sync_appearance(cx.window_appearance(), cx)
    {
        cx.refresh_windows();
    }
    if let Some(path) = settings_path() {
        // Await the previous save before starting this one. Background executor tasks
        // may finish out of order; all writes also share the atomic rename temp path.
        let previous = cx.global_mut::<PendingSave>().0.take();
        let executor = cx.background_executor().clone();
        let save = cx.spawn(async move |_| {
            if let Some(previous) = previous {
                previous.await;
            }
            executor
                .spawn(async move {
                    if let Err(e) = settings.save_to(&path) {
                        eprintln!("failed to save settings to {}: {e}", path.display());
                    }
                })
                .await;
        });
        cx.global_mut::<PendingSave>().0 = Some(save);
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn start_on_open_adds_once_and_removes() {
        let mut settings = Settings::default();
        settings.set_start_on_open("default", true);
        settings.set_start_on_open("default", true);
        settings.set_start_on_open("work", true);
        assert_eq!(settings.start_on_open, ["default", "work"]);
        assert!(settings.starts_on_open("work"));
        // Turning it on again keeps its place.
        settings.set_start_on_open("default", true);
        assert_eq!(settings.start_on_open, ["default", "work"]);
        settings.set_start_on_open("default", false);
        assert_eq!(settings.start_on_open, ["work"]);
        assert!(!settings.starts_on_open("default"));
    }

    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "colima-gpui-settings-{}-{name}",
                std::process::id()
            ))
            .join("settings.json")
    }

    #[test]
    fn round_trips_through_disk() {
        let path = temp_path("roundtrip");
        let settings = Settings {
            theme: ThemeMode::Dark,
            palette: Palette::Zed,
            poll_secs: 10,
            keep_running_in_menu_bar: true,
            check_for_updates: false,
            trim_after_prune: false,
            skill_folder: Some("/tmp/agent-skills".into()),
            start_on_open: vec!["default".into(), "gpui-test".into()],
        };
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), settings);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn missing_invalid_and_partial_files_fall_back() {
        let path = temp_path("fallback");
        assert_eq!(Settings::load_from(&path), Settings::default());
        // A file from before color profiles existed gets the default, Colima.
        assert_eq!(Settings::default().palette, Palette::Colima);

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(Settings::load_from(&path), Settings::default());

        std::fs::write(
            &path,
            r#"{"theme":"light","poll_secs":999,"future_field":1}"#,
        )
        .unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.theme, ThemeMode::Light);
        assert_eq!(loaded.poll_secs, MAX_POLL_SECS);
        assert!(loaded.check_for_updates);
        assert!(loaded.trim_after_prune);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
