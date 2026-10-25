//! On/off switch, shaped like a macOS toggle: a pill track that fills with the accent when
//! on, with a round thumb at the "on" or "off" end. Proportions follow Zed's `Switch`
//! (ui/components/toggle.rs); colours lean towards the macOS look (accent fill, light thumb).

use gpui::{App, ElementId, SharedString, Window, div, prelude::*, px};

use crate::theme::{ActiveTheme, Appearance};
use crate::ui::tooltip::tooltip;

type ToggleHandler = Box<dyn Fn(&bool, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    on: bool,
    disabled: bool,
    tooltip: Option<SharedString>,
    on_toggle: Option<ToggleHandler>,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>, on: bool) -> Self {
        Self {
            id: id.into(),
            on,
            disabled: false,
            tooltip: None,
            on_toggle: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Called with the new state when the switch is clicked. Takes `&bool` so `cx.listener`
    /// fits.
    pub fn on_toggle(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (track, track_hover) = if self.on {
            (theme.accent, theme.accent_hover)
        } else {
            (theme.element_active, theme.element_hover)
        };
        let thumb = match theme.appearance {
            Appearance::Dark => theme.text,
            Appearance::Light => theme.editor_background,
        };
        let group = SharedString::from(format!("switch-{:?}", self.id));
        let on = self.on;
        // The focus ring sits on a transparent 2 px border around the track, like Zed's.
        let switch = div()
            .id(self.id)
            .flex_none()
            .p(px(1.))
            .border_2()
            .border_color(gpui::transparent_black())
            .rounded_full()
            .child(
                div()
                    .group(group.clone())
                    .flex()
                    .items_center()
                    .when(on, |el| el.justify_end())
                    .w(px(32.))
                    .h(px(18.))
                    .px(px(2.))
                    .rounded_full()
                    .bg(track)
                    .when(!self.disabled, |el| {
                        el.group_hover(group, move |s| s.bg(track_hover))
                    })
                    .child(div().size(px(14.)).rounded_full().bg(thumb)),
            )
            .when_some(self.tooltip, |el, text| el.tooltip(tooltip(text)));
        match self.on_toggle {
            Some(handler) if !self.disabled => switch
                .focusable()
                .tab_stop(true)
                .focus_visible(move |s| s.border_color(theme.border_focused))
                .cursor_pointer()
                .on_click(move |_, window, cx| handler(&!on, window, cx)),
            _ => switch.opacity(if self.disabled { 0.5 } else { 1. }),
        }
    }
}
