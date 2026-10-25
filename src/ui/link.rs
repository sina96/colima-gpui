//! Inline link to another page in the app, e.g. a container's image, or to a web page
//! (`.external()`). Accent text with a small arrow icon, underlined on hover; focusable like a
//! button.

use gpui::{App, ClickEvent, ElementId, SharedString, Window, div, prelude::*, px};

use crate::theme::{ActiveTheme, MONO_FONT};
use crate::ui::icon::{Icon, IconName};
use crate::ui::tooltip::tooltip;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Link {
    id: ElementId,
    label: SharedString,
    tooltip: Option<SharedString>,
    mono: bool,
    external: bool,
    on_click: Option<ClickHandler>,
}

impl Link {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            tooltip: None,
            mono: false,
            external: false,
            on_click: None,
        }
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Lilex, for IDs and digests.
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// Opens a web page: the arrow becomes the external-link icon.
    pub fn external(mut self) -> Self {
        self.external = true;
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

impl RenderOnce for Link {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let link = div()
            .id(self.id)
            .flex()
            .items_center()
            .gap_0p5()
            .min_w_0()
            .rounded_sm()
            .text_color(theme.text_accent)
            .when(self.mono, |el| el.font_family(MONO_FONT))
            .child(div().min_w_0().truncate().child(self.label))
            .child(
                Icon::new(if self.external {
                    IconName::ExternalLink
                } else {
                    IconName::ArrowUpRight
                })
                .size(px(12.))
                .color(theme.text_accent),
            )
            .when_some(self.tooltip, |el, text| el.tooltip(tooltip(text)));
        match self.on_click {
            Some(handler) => link
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .hover(|s| s.underline())
                .focus_visible(move |s| s.border_1().border_color(theme.border_focused))
                .on_click(handler),
            None => link,
        }
    }
}
