//! Text tooltip shared by every widget: `.tooltip(tooltip("text"))` on any stateful element.

use gpui::{AnyView, App, Context, SharedString, Window, div, prelude::*};

use crate::theme::ActiveTheme;

struct Tooltip(SharedString);

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .max_w_80()
            .px_2()
            .py_1()
            .rounded_sm()
            .text_sm()
            .bg(theme.elevated_surface_background)
            .text_color(theme.text)
            .border_1()
            .border_color(theme.border)
            .child(self.0.clone())
    }
}

/// Builder for GPUI's `.tooltip(...)`.
pub fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tooltip(text.clone())).into()
}
