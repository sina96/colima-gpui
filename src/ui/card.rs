use crate::theme::ActiveTheme;
use gpui::{AnyElement, App, Window, div, prelude::*};
#[derive(IntoElement)]
pub struct Card {
    header: AnyElement,
    body: AnyElement,
}
impl Card {
    pub fn new(header: impl IntoElement, body: impl IntoElement) -> Self {
        Self {
            header: header.into_any_element(),
            body: body.into_any_element(),
        }
    }
}
impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        div()
            .flex()
            .flex_col()
            .rounded_sm()
            .border_1()
            .border_color(t.border_variant)
            .bg(t.surface_background)
            .child(
                div()
                    .h_8()
                    .px_3()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(t.border_variant)
                    .child(self.header),
            )
            .child(div().p_3().bg(t.editor_background).child(self.body))
    }
}
