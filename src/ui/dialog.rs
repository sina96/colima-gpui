//! Modal layer following GPUI's pinned deferred/anchored popover example.
use crate::{
    theme::ActiveTheme,
    ui::{Button, ButtonStyle},
};
use gpui::{
    Context, EventEmitter, FocusHandle, SharedString, Window, anchored, deferred, div, point,
    prelude::*, px,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogResult {
    Confirmed,
    Cancelled,
}

pub struct ConfirmDialog {
    title: SharedString,
    message: SharedString,
    confirm_label: SharedString,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    confirm_focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    open: bool,
}
impl EventEmitter<DialogResult> for ConfirmDialog {}
impl ConfirmDialog {
    pub fn new(
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            confirm_label: "Delete".into(),
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            confirm_focus: cx.focus_handle(),
            previous_focus: None,
            open: false,
        }
    }
    pub fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.previous_focus = window.focused(cx);
        self.open = true;
        window.focus(&self.cancel_focus, cx);
        cx.notify();
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    fn finish(&mut self, result: DialogResult, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if let Some(focus) = self.previous_focus.take() {
            window.focus(&focus, cx);
        }
        cx.emit(result);
        cx.notify();
    }
}
impl Render for ConfirmDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let viewport = window.viewport_size();
        div().when(self.open, |el| {
            el.child(
                deferred(
                    anchored().position(point(px(0.), px(0.))).child(
                        div()
                            .id("confirm-overlay")
                            .track_focus(&self.focus)
                            .w(viewport.width)
                            .h(viewport.height)
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(t.editor_background.opacity(0.85))
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_key_down(cx.listener(
                                |this, event: &gpui::KeyDownEvent, window, cx| {
                                    match event.keystroke.key.as_str() {
                                        "escape" => {
                                            this.finish(DialogResult::Cancelled, window, cx)
                                        }
                                        "tab" => {
                                            let target = if this.cancel_focus.is_focused(window) {
                                                &this.confirm_focus
                                            } else {
                                                &this.cancel_focus
                                            };
                                            window.focus(target, cx);
                                        }
                                        _ => return,
                                    }
                                    cx.stop_propagation();
                                },
                            ))
                            .child(
                                div()
                                    .w(px(360.))
                                    .p_3()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(t.border)
                                    .bg(t.elevated_surface_background)
                                    .text_sm()
                                    .text_color(t.text)
                                    .child(self.title.clone())
                                    .child(
                                        div().text_color(t.text_muted).child(self.message.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .justify_end()
                                            .gap_2()
                                            .child(
                                                Button::new("cancel", "Cancel")
                                                    .track_focus(&self.cancel_focus)
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.finish(
                                                                DialogResult::Cancelled,
                                                                window,
                                                                cx,
                                                            )
                                                        },
                                                    )),
                                            )
                                            .child(
                                                Button::new("confirm", self.confirm_label.clone())
                                                    .style(ButtonStyle::Danger)
                                                    .track_focus(&self.confirm_focus)
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.finish(
                                                                DialogResult::Confirmed,
                                                                window,
                                                                cx,
                                                            )
                                                        },
                                                    )),
                                            ),
                                    ),
                            ),
                    ),
                )
                .priority(10),
            )
        })
    }
}
