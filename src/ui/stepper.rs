use crate::{
    theme::{ActiveTheme, MONO_FONT},
    ui::Button,
};
use gpui::{Context, EventEmitter, FocusHandle, SharedString, Window, div, prelude::*, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepperChanged(pub i32);

pub struct Stepper {
    value: i32,
    min: i32,
    max: i32,
    step: i32,
    unit: SharedString,
    focus: FocusHandle,
}
impl EventEmitter<StepperChanged> for Stepper {}
impl Stepper {
    /// The bounds must be ordered and step must be positive.
    pub fn new(value: i32, min: i32, max: i32, step: i32, cx: &mut Context<Self>) -> Self {
        assert!(min <= max, "stepper min must not exceed max");
        assert!(step > 0, "stepper step must be positive");
        Self {
            value: value.clamp(min, max),
            min,
            max,
            step,
            unit: "".into(),
            focus: cx.focus_handle().tab_stop(true),
        }
    }
    pub fn unit(mut self, unit: impl Into<SharedString>) -> Self {
        self.unit = unit.into();
        self
    }
    pub fn value(&self) -> i32 {
        self.value
    }
    pub fn set_value(&mut self, value: i32, cx: &mut Context<Self>) {
        self.value = value.clamp(self.min, self.max);
        cx.notify();
    }
    fn change(&mut self, value: i32, cx: &mut Context<Self>) {
        let value = value.clamp(self.min, self.max);
        if self.value != value {
            self.value = value;
            cx.emit(StepperChanged(value));
            cx.notify();
        }
    }
    fn adjust(&mut self, increase: bool, cx: &mut Context<Self>) {
        self.change(
            next_value(self.value, self.min, self.max, self.step, increase),
            cx,
        );
    }
}
fn next_value(value: i32, min: i32, max: i32, step: i32, increase: bool) -> i32 {
    if increase {
        value.saturating_add(step)
    } else {
        value.saturating_sub(step)
    }
    .clamp(min, max)
}
impl Render for Stepper {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        div()
            .id("stepper")
            .track_focus(&self.focus)
            .tab_stop(true)
            .flex()
            .items_center()
            .gap_1()
            .h(px(28.))
            .rounded_sm()
            .border_1()
            .border_color(t.border)
            .focus(|s| s.border_color(t.border_focused))
            .bg(t.element_background)
            .text_color(t.text)
            .text_sm()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "up" | "right" => this.adjust(true, cx),
                    "down" | "left" => this.adjust(false, cx),
                    "home" => this.change(this.min, cx),
                    "end" => this.change(this.max, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(
                Button::new("minus", "−")
                    .tab_stop(false)
                    .disabled(self.value == self.min)
                    .on_click(cx.listener(|this, _, _, cx| this.adjust(false, cx))),
            )
            .child(
                div()
                    .min_w(px(60.))
                    .text_center()
                    .font_family(MONO_FONT)
                    .child(format!("{} {}", self.value, self.unit)),
            )
            .child(
                Button::new("plus", "+")
                    .tab_stop(false)
                    .disabled(self.value == self.max)
                    .on_click(cx.listener(|this, _, _, cx| this.adjust(true, cx))),
            )
    }
}
impl gpui::Focusable for Stepper {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}
#[cfg(test)]
mod tests {
    use super::next_value;
    #[test]
    fn bounds_step_and_overflow() {
        assert_eq!(next_value(2, 1, 8, 3, true), 5);
        assert_eq!(next_value(7, 1, 8, 3, true), 8);
        assert_eq!(next_value(2, 1, 8, 3, false), 1);
        assert_eq!(
            next_value(i32::MAX - 1, i32::MIN, i32::MAX, 10, true),
            i32::MAX
        );
        assert_eq!(
            next_value(i32::MIN + 1, i32::MIN, i32::MAX, 10, false),
            i32::MIN
        );
    }
}
