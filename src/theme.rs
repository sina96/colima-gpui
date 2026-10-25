//! The app's colour palettes and fonts.
//!
//! - **Zed**: token names and values mirror `assets/themes/one/one.json` in the
//!   pinned Zed checkout (One theme: MIT, Copyright (c) 2014 GitHub Inc.).
//! - **Colima** (default): surfaces, borders and text from colima.run's CSS variables (`--bg-*`,
//!   `--border-color`, `--text-*`); the dark site is GitHub's dark grey, so the gaps it
//!   doesn't cover (hover, selection, status, charts) use GitHub's dark and light colours.
//!   Colima green fills controls; links and focus stay blue, as in Zed.
//!
//! Every token needs a value in all four themes. Views only read tokens, so a new palette
//! restyles the whole app.

use std::borrow::Cow;

use gpui::{App, Global, Hsla, WindowAppearance, rgba};

use crate::settings::{Palette, ThemeMode};

pub const UI_FONT: &str = "IBM Plex Sans";
pub const MONO_FONT: &str = "Lilex";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

impl From<WindowAppearance> for Appearance {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub palette: Palette,
    pub appearance: Appearance,

    pub background: Hsla,
    pub surface_background: Hsla,
    pub elevated_surface_background: Hsla,
    pub editor_background: Hsla,
    /// Log/terminal panels. Darker than the editor so logs read as a terminal; One's own
    /// `terminal.background` equals the editor's, so this uses One Dark's classic UI shade.
    pub terminal_background: Hsla,
    pub title_bar_background: Hsla,
    pub status_bar_background: Hsla,

    pub border: Hsla,
    pub border_variant: Hsla,
    pub border_focused: Hsla,

    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_placeholder: Hsla,
    pub text_accent: Hsla,

    pub element_background: Hsla,
    pub element_hover: Hsla,
    pub element_active: Hsla,
    pub element_selected: Hsla,
    /// Hover over a selected item: a step past `element_selected`, never back to grey.
    pub element_selected_hover: Hsla,
    pub ghost_element_hover: Hsla,
    pub ghost_element_selected: Hsla,

    /// Player 0 cursor color: Zed's accent for filled primary controls.
    pub accent: Hsla,
    /// Hover over a filled accent control.
    pub accent_hover: Hsla,
    pub error: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub info: Hsla,
    /// Categorical chart colours: the One theme's player colours, in its order
    /// (blue, red, orange, purple, teal, rose, yellow, green). Use `CHART_*` indices.
    pub chart: [Hsla; 8],
}

/// Fixed chart colour per storage category, so every chart agrees.
pub const CHART_IMAGES: usize = 0;
pub const CHART_CONTAINERS: usize = 3;
pub const CHART_VOLUMES: usize = 4;
pub const CHART_BUILD_CACHE: usize = 2;

fn c(hex: u32) -> Hsla {
    rgba(hex).into()
}

impl Theme {
    pub fn one_dark() -> Self {
        Self {
            palette: Palette::Zed,
            appearance: Appearance::Dark,
            background: c(0x3b414dff),
            surface_background: c(0x2f343eff),
            elevated_surface_background: c(0x2f343eff),
            editor_background: c(0x282c33ff),
            terminal_background: c(0x21252bff),
            title_bar_background: c(0x3b414dff),
            status_bar_background: c(0x3b414dff),
            border: c(0x464b57ff),
            border_variant: c(0x363c46ff),
            border_focused: c(0x47679eff),
            text: c(0xdce0e5ff),
            text_muted: c(0xa9afbcff),
            text_placeholder: c(0x878a98ff),
            text_accent: c(0x74ade8ff),
            element_background: c(0x2e343eff),
            element_hover: c(0x363c46ff),
            element_active: c(0x454a56ff),
            element_selected: c(0x454a56ff),
            element_selected_hover: c(0x4d5361ff),
            ghost_element_hover: c(0x363c46ff),
            ghost_element_selected: c(0x454a56ff),
            accent: c(0x74ade8ff),
            accent_hover: c(0x8bbcefff),
            error: c(0xd07277ff),
            success: c(0xa1c181ff),
            warning: c(0xdec184ff),
            info: c(0x74ade8ff),
            chart: [
                c(0x74ade8ff),
                c(0xbe5046ff),
                c(0xbf956aff),
                c(0xb477cfff),
                c(0x6eb4bfff),
                c(0xd07277ff),
                c(0xdec184ff),
                c(0xa1c181ff),
            ],
        }
    }

    pub fn one_light() -> Self {
        Self {
            palette: Palette::Zed,
            appearance: Appearance::Light,
            background: c(0xdcdcddff),
            surface_background: c(0xebebecff),
            elevated_surface_background: c(0xebebecff),
            editor_background: c(0xfafafaff),
            terminal_background: c(0xf0f0f1ff),
            title_bar_background: c(0xdcdcddff),
            status_bar_background: c(0xdcdcddff),
            border: c(0xc9c9caff),
            border_variant: c(0xdfdfe0ff),
            border_focused: c(0x7d82e8ff),
            text: c(0x242529ff),
            text_muted: c(0x58585aff),
            text_placeholder: c(0x7e8086ff),
            text_accent: c(0x5c78e2ff),
            element_background: c(0xebebecff),
            element_hover: c(0xdfdfe0ff),
            element_active: c(0xcacacaff),
            element_selected: c(0xcacacaff),
            element_selected_hover: c(0xc0c0c1ff),
            ghost_element_hover: c(0xdfdfe0ff),
            ghost_element_selected: c(0xcacacaff),
            accent: c(0x5c78e2ff),
            accent_hover: c(0x4a66d4ff),
            error: c(0xd36151ff),
            success: c(0x669f59ff),
            warning: c(0xa48819ff),
            info: c(0x5c78e2ff),
            chart: [
                c(0x5c78e2ff),
                c(0x984ea5ff),
                c(0xad6e26ff),
                c(0xa349abff),
                c(0x3a82b7ff),
                c(0xd36151ff),
                c(0xa48819ff),
                c(0x669f59ff),
            ],
        }
    }

    /// colima.run's dark theme (`[data-theme=dark]`).
    pub fn colima_dark() -> Self {
        Self {
            palette: Palette::Colima,
            appearance: Appearance::Dark,
            background: c(0x161b22ff),
            surface_background: c(0x161b22ff),
            elevated_surface_background: c(0x1c2128ff),
            editor_background: c(0x0d1117ff),
            terminal_background: c(0x010409ff),
            title_bar_background: c(0x161b22ff),
            status_bar_background: c(0x161b22ff),
            border: c(0x30363dff),
            border_variant: c(0x21262dff),
            border_focused: c(0x1f6febff),
            text: c(0xe6edf3ff),
            text_muted: c(0x9ca3afff),
            text_placeholder: c(0x6e7681ff),
            text_accent: c(0x58a6ffff),
            element_background: c(0x21262dff),
            element_hover: c(0x30363dff),
            element_active: c(0x3d444dff),
            element_selected: c(0x1a5737ff),
            element_selected_hover: c(0x1f6640ff),
            ghost_element_hover: c(0x1b2420ff),
            ghost_element_selected: c(0x1a5737ff),
            accent: c(0x22c55eff),
            accent_hover: c(0x4ade80ff),
            error: c(0xf85149ff),
            success: c(0x22c55eff),
            warning: c(0xd29922ff),
            info: c(0x58a6ffff),
            chart: [
                c(0x58a6ffff),
                c(0xff7b72ff),
                c(0xf0883eff),
                c(0xbc8cffff),
                c(0x39c5cfff),
                c(0xf778baff),
                c(0xd29922ff),
                c(0x3fb950ff),
            ],
        }
    }

    /// colima.run's light theme (`:root`): mint whites and green-tinted text. Filled
    /// controls use the site's darker hover green so white labels stay readable.
    pub fn colima_light() -> Self {
        Self {
            palette: Palette::Colima,
            appearance: Appearance::Light,
            background: c(0xeef9f2ff),
            surface_background: c(0xf7fdf9ff),
            elevated_surface_background: c(0xffffffff),
            editor_background: c(0xfcfefdff),
            terminal_background: c(0xf3faf5ff),
            title_bar_background: c(0xeef9f2ff),
            status_bar_background: c(0xeef9f2ff),
            border: c(0xb9dcc5ff),
            border_variant: c(0xd4eedcff),
            border_focused: c(0x5c78e2ff),
            text: c(0x1a3d2bff),
            text_muted: c(0x5c7568ff),
            text_placeholder: c(0x7f9688ff),
            text_accent: c(0x5c78e2ff),
            element_background: c(0xeef9f2ff),
            element_hover: c(0xe2f3e8ff),
            element_active: c(0xd4eedcff),
            element_selected: c(0xc3e7d0ff),
            element_selected_hover: c(0xb2dfc2ff),
            ghost_element_hover: c(0xe3f4e9ff),
            ghost_element_selected: c(0xc3e7d0ff),
            accent: c(0x16a34aff),
            accent_hover: c(0x15803dff),
            error: c(0xcf222eff),
            success: c(0x16a34aff),
            warning: c(0x9a6700ff),
            info: c(0x5c78e2ff),
            chart: [
                c(0x0969daff),
                c(0xcf222eff),
                c(0xbc4c00ff),
                c(0x8250dfff),
                c(0x1b7c83ff),
                c(0xbf3989ff),
                c(0x9a6700ff),
                c(0x1a7f37ff),
            ],
        }
    }

    pub fn new(palette: Palette, appearance: Appearance) -> Self {
        match (palette, appearance) {
            (Palette::Zed, Appearance::Light) => Self::one_light(),
            (Palette::Zed, Appearance::Dark) => Self::one_dark(),
            (Palette::Colima, Appearance::Light) => Self::colima_light(),
            (Palette::Colima, Appearance::Dark) => Self::colima_dark(),
        }
    }

    /// The default color profile; for `examples/`, which have no settings.
    pub fn for_appearance(appearance: Appearance) -> Self {
        Self::new(Palette::default(), appearance)
    }
}

impl Global for Theme {}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

/// Loads the bundled fonts and installs the theme matching the system appearance.
pub fn init(cx: &mut App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/ibm-plex-sans/IBMPlexSans-SemiBold.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/ibm-plex-sans/IBMPlexSans-Italic.ttf"
        )),
        Cow::Borrowed(include_bytes!("../assets/fonts/lilex/Lilex-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/lilex/Lilex-Bold.ttf")),
    ];
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("failed to load bundled fonts: {e}");
    }
    let (palette, mode) = chosen(cx);
    cx.set_global(Theme::new(
        palette,
        resolved_appearance(mode, cx.window_appearance()),
    ));
}

/// The user's color profile and theme mode, or the defaults (Colima, following the system)
/// when settings aren't loaded (e.g. in `examples/`).
fn chosen(cx: &App) -> (Palette, ThemeMode) {
    cx.try_global::<crate::settings::Settings>()
        .map(|settings| (settings.palette, settings.theme))
        .unwrap_or_default()
}

/// Swaps the theme when the system appearance or the user's choice changes. Returns
/// whether it changed.
pub fn sync_appearance(appearance: WindowAppearance, cx: &mut App) -> bool {
    let (palette, mode) = chosen(cx);
    let appearance = resolved_appearance(mode, appearance);
    if cx.theme().appearance == appearance && cx.theme().palette == palette {
        return false;
    }
    cx.set_global(Theme::new(palette, appearance));
    true
}

fn resolved_appearance(mode: ThemeMode, system: WindowAppearance) -> Appearance {
    match mode {
        ThemeMode::System => system.into(),
        ThemeMode::Light => Appearance::Light,
        ThemeMode::Dark => Appearance::Dark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_has_a_light_and_a_dark_theme() {
        for palette in [Palette::Zed, Palette::Colima] {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let theme = Theme::new(palette, appearance);
                assert_eq!((theme.palette, theme.appearance), (palette, appearance));
                // Text must stand out from the main content background.
                assert!((theme.text.l - theme.editor_background.l).abs() > 0.5);
            }
        }
        assert_ne!(
            Theme::colima_dark().editor_background,
            Theme::one_dark().editor_background
        );
    }

    #[test]
    fn explicit_modes_override_the_os_and_system_follows_it() {
        for system in [
            WindowAppearance::Light,
            WindowAppearance::Dark,
            WindowAppearance::VibrantLight,
            WindowAppearance::VibrantDark,
        ] {
            assert_eq!(
                resolved_appearance(ThemeMode::Light, system),
                Appearance::Light
            );
            assert_eq!(
                resolved_appearance(ThemeMode::Dark, system),
                Appearance::Dark
            );
            assert_eq!(
                resolved_appearance(ThemeMode::System, system),
                Appearance::from(system)
            );
        }
    }
}
