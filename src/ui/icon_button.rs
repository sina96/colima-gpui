//! Square icon-only buttons (`IconButton`) and the ⓘ hover explainer (`InfoTip`).

use gpui::{App, ClickEvent, ElementId, SharedString, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;
use crate::ui::icon::{Icon, IconName};
use crate::ui::tooltip::tooltip;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Ghost square button with one icon. Always give it a tooltip: the icon is its only label.
#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    icon: IconName,
    tooltip: Option<SharedString>,
    danger: bool,
    disabled: bool,
    /// 22 px (default) or 28 px, matching `ButtonSize::Default` / `Medium`.
    medium: bool,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            id: id.into(),
            icon,
            tooltip: None,
            danger: false,
            disabled: false,
            medium: false,
            on_click: None,
        }
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn medium(mut self) -> Self {
        self.medium = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let size = px(if self.medium { 28. } else { 22. });
        let (hover_color, hover_bg) = if self.danger {
            (theme.error, theme.error.opacity(0.15))
        } else {
            (theme.text, theme.ghost_element_hover)
        };
        let button = div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(size)
            .rounded_sm()
            .when_some(self.tooltip, |el, text| el.tooltip(tooltip(text)));
        match self.on_click {
            Some(handler) if !self.disabled => button
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .focus_visible(move |s| s.border_1().border_color(theme.border_focused))
                .group("icon-button")
                .child(Icon::new(self.icon).group_hover_color("icon-button", hover_color))
                .on_click(handler),
            _ => button.opacity(0.5).child(Icon::new(self.icon)),
        }
    }
}

/// ⓘ that explains something on hover, replacing paragraphs of help text.
#[derive(IntoElement)]
pub struct InfoTip {
    id: ElementId,
    text: SharedString,
}

impl InfoTip {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
        }
    }
}

impl RenderOnce for InfoTip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(18.))
            .rounded_full()
            .hover(|s| s.bg(theme.ghost_element_hover))
            .child(Icon::new(IconName::Info).size(px(14.)))
            .tooltip(tooltip(self.text))
    }
}
