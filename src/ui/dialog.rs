//! Modal layer following GPUI's pinned deferred/anchored popover example.
use crate::{
    theme::ActiveTheme,
    ui::{Button, ButtonStyle, Switch},
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
    option_focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    open: bool,
    /// An optional extra choice shown as a switch, off each time the dialog opens.
    option: Option<SharedString>,
    option_on: bool,
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
            option_focus: cx.focus_handle(),
            previous_focus: None,
            open: false,
            option: None,
            option_on: false,
        }
    }
    /// Adds a switch with this label, e.g. "Also delete container data". Read it with
    /// [`ConfirmDialog::option_on`] when the dialog reports `Confirmed`.
    pub fn option(mut self, label: impl Into<SharedString>) -> Self {
        self.option = Some(label.into());
        self
    }
    pub fn option_on(&self) -> bool {
        self.option_on
    }
    pub fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.previous_focus = window.focused(cx);
        self.open = true;
        self.option_on = false;
        window.focus(&self.cancel_focus, cx);
        cx.notify();
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    fn toggle_option(&mut self, cx: &mut Context<Self>) {
        self.option_on = !self.option_on;
        cx.notify();
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
                                        // Cancel → Confirm → the option (when shown) → Cancel.
                                        "tab" => {
                                            let target = if this.cancel_focus.is_focused(window) {
                                                &this.confirm_focus
                                            } else if this.confirm_focus.is_focused(window)
                                                && this.option.is_some()
                                            {
                                                &this.option_focus
                                            } else {
                                                &this.cancel_focus
                                            };
                                            window.focus(target, cx);
                                        }
                                        "space" | "enter"
                                            if this.option.is_some()
                                                && this.option_focus.is_focused(window) =>
                                        {
                                            this.toggle_option(cx)
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
                                    .when_some(self.option.clone(), |el, label| {
                                        let on = self.option_on;
                                        let toggle = cx.listener(|this: &mut Self, _, _, cx| {
                                            this.toggle_option(cx)
                                        });
                                        el.child(
                                            div()
                                                .id("confirm-option")
                                                .track_focus(&self.option_focus)
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .p_0p5()
                                                .rounded_sm()
                                                .border_1()
                                                .border_color(gpui::transparent_black())
                                                .focus_visible(|s| s.border_color(t.border_focused))
                                                .cursor_pointer()
                                                .child(Switch::new("confirm-option-switch", on))
                                                .child(label)
                                                .on_click(toggle),
                                        )
                                    })
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
