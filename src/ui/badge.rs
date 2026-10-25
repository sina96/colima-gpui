use crate::theme::ActiveTheme;
use gpui::{App, SharedString, Window, div, prelude::*};
#[derive(Clone, Copy, Default)]
pub enum BadgeStyle {
    Running,
    Stopped,
    Broken,
    #[default]
    Neutral,
}
#[derive(IntoElement)]
pub struct Badge {
    label: SharedString,
    style: BadgeStyle,
}
impl Badge {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            style: BadgeStyle::Neutral,
        }
    }
    pub fn style(mut self, style: BadgeStyle) -> Self {
        self.style = style;
        self
    }
    pub fn status(status: &str) -> Self {
        Self::new(status.to_owned()).style(match status {
            "Running" => BadgeStyle::Running,
            "Stopped" => BadgeStyle::Stopped,
            "Broken" => BadgeStyle::Broken,
            _ => BadgeStyle::Neutral,
        })
    }
}
impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let color = match self.style {
            BadgeStyle::Running => t.success,
            BadgeStyle::Stopped | BadgeStyle::Neutral => t.text_muted,
            BadgeStyle::Broken => t.error,
        };
        div()
            .flex()
            .items_center()
            .gap_1()
            .text_sm()
            .text_color(color)
            .child(div().size_1p5().rounded_full().bg(color))
            .child(self.label)
    }
}
