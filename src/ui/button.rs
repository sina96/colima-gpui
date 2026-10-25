//! Button proportions follow Zed's pinned ui/components/button implementation.
use crate::theme::ActiveTheme;
use gpui::{App, ClickEvent, ElementId, FocusHandle, SharedString, Window, div, prelude::*, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonStyle {
    #[default]
    Ghost,
    Primary,
    /// Compatibility with the T0 prototype.
    Accent,
    Secondary,
    /// The chosen item in a group (tabs, segmented choices): `element_selected` fill, and a
    /// stronger fill on hover.
    Selected,
    Danger,
}
#[derive(Debug, Clone, Copy, Default)]
pub enum ButtonSize {
    #[default]
    Default,
    Medium,
    Large,
}
type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    style: ButtonStyle,
    size: ButtonSize,
    disabled: bool,
    loading: bool,
    tooltip: Option<SharedString>,
    focus: Option<FocusHandle>,
    tab_stop: bool,
    icon: Option<crate::ui::IconName>,
    on_click: Option<ClickHandler>,
}
impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            style: ButtonStyle::default(),
            size: ButtonSize::default(),
            disabled: false,
            loading: false,
            tooltip: None,
            focus: None,
            tab_stop: true,
            icon: None,
            on_click: None,
        }
    }
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Icon before the label, drawn in the label's colour.
    pub fn icon(mut self, icon: crate::ui::IconName) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
    /// Whether Tab can focus the button; off for buttons inside a focusable control.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}
impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (bg, fg, hover) = match self.style {
            ButtonStyle::Ghost => (None, theme.text_muted, theme.ghost_element_hover),
            ButtonStyle::Primary | ButtonStyle::Accent => (
                Some(theme.accent),
                theme.editor_background,
                theme.accent_hover,
            ),
            ButtonStyle::Secondary => (
                Some(theme.element_background),
                theme.text_muted,
                theme.element_hover,
            ),
            ButtonStyle::Selected => (
                Some(theme.element_selected),
                theme.text,
                theme.element_selected_hover,
            ),
            ButtonStyle::Danger => (None, theme.error, theme.error.opacity(0.15)),
        };
        let inactive = self.disabled || self.loading;
        let button = div()
            .id(self.id)
            .flex()
            .items_center()
            .justify_center()
            .h(px(match self.size {
                ButtonSize::Default => 22.,
                ButtonSize::Medium => 28.,
                ButtonSize::Large => 32.,
            }))
            .px_2()
            .rounded_sm()
            .text_sm()
            .border_1()
            .border_color(if self.style == ButtonStyle::Secondary {
                theme.border
            } else {
                gpui::transparent_black()
            })
            .text_color(fg)
            .when_some(bg, |el, bg| el.bg(bg))
            .when_some(self.icon.filter(|_| !self.loading), |el, icon| {
                el.gap_1p5()
                    .child(crate::ui::Icon::new(icon).size(px(14.)).color(fg))
            })
            .child(if self.loading {
                "Working…".into()
            } else {
                self.label
            })
            .when_some(self.tooltip, |el, text| {
                el.tooltip(crate::ui::tooltip::tooltip(text))
            });
        match self.on_click {
            Some(handler) if !inactive => button
                .focusable()
                .tab_stop(self.tab_stop)
                .when_some(self.focus, |el, focus| el.track_focus(&focus))
                .focus_visible(move |s| s.border_color(theme.border_focused))
                .cursor_pointer()
                .hover(move |s| {
                    s.bg(hover).text_color(
                        if self.style == ButtonStyle::Ghost || self.style == ButtonStyle::Secondary
                        {
                            theme.text
                        } else {
                            fg
                        },
                    )
                })
                .on_click(handler),
            _ => button.opacity(if inactive { 0.5 } else { 1. }),
        }
    }
}
