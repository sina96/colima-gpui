//! Fixed-option select; floating layer follows the pinned GPUI popover example.
use crate::theme::ActiveTheme;
use gpui::{
    Context, EventEmitter, FocusHandle, SharedString, Window, anchored, deferred, div, prelude::*,
    px,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectChanged<T>(pub T);

pub struct Select<T: Clone + PartialEq + 'static> {
    options: Vec<(T, SharedString)>,
    selected: Option<usize>,
    highlighted: usize,
    open: bool,
    focus: FocusHandle,
}
impl<T: Clone + PartialEq + 'static> EventEmitter<SelectChanged<T>> for Select<T> {}
impl<T: Clone + PartialEq + 'static> Select<T> {
    pub fn new(options: Vec<(T, SharedString)>, cx: &mut Context<Self>) -> Self {
        let selected = if options.is_empty() { None } else { Some(0) };
        Self {
            options,
            selected,
            highlighted: 0,
            open: false,
            focus: cx.focus_handle().tab_stop(true),
        }
    }
    pub fn value(&self) -> Option<&T> {
        self.selected.map(|i| &self.options[i].0)
    }
    /// Programmatic updates do not emit a user change event.
    pub fn set_value(&mut self, value: &T, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.options.iter().position(|(v, _)| v == value) else {
            return false;
        };
        self.selected = Some(index);
        self.highlighted = index;
        cx.notify();
        true
    }
    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.options.is_empty() {
            return;
        }
        self.open = !self.open;
        self.highlighted = self.selected.unwrap_or(0);
        window.focus(&self.focus, cx);
        cx.notify();
    }
    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.options.len() {
            return;
        }
        self.open = false;
        if self.selected != Some(index) {
            self.selected = Some(index);
            cx.emit(SelectChanged(self.options[index].0.clone()));
        }
        cx.notify();
    }
}
impl<T: Clone + PartialEq + 'static> Render for Select<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let label = self
            .selected
            .map(|i| self.options[i].1.clone())
            .unwrap_or_else(|| "No options".into());
        let rows = self
            .options
            .iter()
            .enumerate()
            .map(|(index, (_, label))| {
                div()
                    .id(("option", index))
                    .h(px(22.))
                    .px_2()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .when(index == self.highlighted, |el| el.bg(t.element_selected))
                    .hover(|s| s.bg(t.element_hover))
                    .child(label.clone())
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if this.highlighted != index {
                            this.highlighted = index;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        window.focus(&this.focus, cx);
                        this.choose(index, cx);
                    }))
            })
            .collect::<Vec<_>>();
        div()
            .id("select")
            .track_focus(&self.focus)
            .tab_stop(true)
            .relative()
            .w(px(180.))
            .text_sm()
            .text_color(t.text)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.options.is_empty() {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" | "down" => {
                        if !this.open {
                            this.open = true;
                            this.highlighted = this.selected.unwrap_or(0);
                        } else if event.keystroke.key == "down" {
                            this.highlighted = (this.highlighted + 1) % this.options.len();
                        } else {
                            this.highlighted =
                                (this.highlighted + this.options.len() - 1) % this.options.len();
                        }
                    }
                    "enter" | "space" => {
                        if this.open {
                            this.choose(this.highlighted, cx);
                        } else {
                            this.toggle(window, cx);
                        }
                    }
                    "escape" => this.open = false,
                    "tab" => {
                        this.open = false;
                        cx.notify();
                        return;
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                div()
                    .id("trigger")
                    .h(px(28.))
                    .px_2()
                    .rounded_sm()
                    .border_1()
                    .border_color(if self.focus.is_focused(window) {
                        t.border_focused
                    } else {
                        t.border
                    })
                    .focus_visible(|s| s.border_color(t.border_focused))
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(t.element_background)
                    .hover(|s| s.bg(t.element_hover))
                    .cursor_pointer()
                    .child(label)
                    .child("⌄")
                    .on_click(cx.listener(|this, _, window, cx| this.toggle(window, cx))),
            )
            .when(self.open, |el| {
                el.child(
                    deferred(
                        anchored().snap_to_window_with_margin(px(8.)).child(
                            div()
                                .id("options")
                                .occlude()
                                .w(px(180.))
                                .max_h(px(240.))
                                .overflow_y_scroll()
                                .py_1()
                                .rounded_sm()
                                .border_1()
                                .border_color(t.border)
                                .bg(t.elevated_surface_background)
                                .children(rows)
                                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                    this.open = false;
                                    cx.notify();
                                })),
                        ),
                    )
                    .priority(2),
                )
            })
    }
}

impl<T: Clone + PartialEq + 'static> gpui::Focusable for Select<T> {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}
