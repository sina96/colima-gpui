//! A compact error line: an alert icon, a one-line message, and the full text on hover with a
//! copy button for bug reports. Use it instead of raw red text, so a long CLI error doesn't
//! fill the view.

use gpui::{App, ClipboardItem, ElementId, SharedString, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;
use crate::ui::icon::{Icon, IconName};
use crate::ui::icon_button::IconButton;
use crate::ui::tooltip::tooltip;

#[derive(IntoElement)]
pub struct ErrorNote {
    id: ElementId,
    message: SharedString,
    details: Option<SharedString>,
}

impl ErrorNote {
    pub fn new(id: impl Into<ElementId>, message: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            message: message.into(),
            details: None,
        }
    }

    /// The full error behind a short `message`, e.g. "VM usage unavailable". Shown on hover
    /// and copied by the copy button.
    pub fn details(mut self, details: impl Into<SharedString>) -> Self {
        self.details = Some(details.into());
        self
    }
}

impl RenderOnce for ErrorNote {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let full: SharedString = match &self.details {
            Some(details) if details != &self.message => {
                format!("{}\n\n{details}", self.message).into()
            }
            _ => self.message.clone(),
        };
        let group = SharedString::from(format!("error-note-{:?}", self.id));
        let copied = full.clone();
        div()
            .id(self.id.clone())
            .group(group.clone())
            .flex()
            .items_center()
            .gap_1p5()
            .min_w_0()
            .child(
                Icon::new(IconName::CircleAlert)
                    .size(px(14.))
                    .color(theme.error),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(theme.error)
                    .child(self.message),
            )
            .tooltip(tooltip(full))
            .child(
                div()
                    .flex_none()
                    .invisible()
                    .group_hover(group, |s| s.visible())
                    .child(
                        IconButton::new((self.id, "copy"), IconName::Copy)
                            .tooltip("Copy the error")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.to_string()))
                            }),
                    ),
            )
    }
}
