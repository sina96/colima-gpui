//! ⋯ action menu: an icon button that opens a small popover list.
//!
//! Open/closed state lives in GPUI keyed element state, so a table can render one
//! `MenuButton` per row without allocating anything per row.
//!
//! ```ignore
//! MenuButton::new(("row-menu", ix))
//!     .item(MenuItem::new("Restart", cx.listener(move |this, _, _, cx| this.restart(id.clone(), cx))))
//!     .item(MenuItem::new("Remove", cx.listener(…)).danger())
//! ```
//! Keyboard: Enter/Space opens; ↑/↓ moves; Enter runs; Esc or a click outside closes.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Context, ElementId, FocusHandle, KeyDownEvent, SharedString, Window, anchored,
    deferred, div, prelude::*, px,
};

use crate::theme::ActiveTheme;
use crate::ui::icon::{Icon, IconName};
use crate::ui::icon_button::IconButton;

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct MenuItem {
    label: SharedString,
    icon: Option<IconName>,
    danger: bool,
    disabled: bool,
    handler: Handler,
}

impl MenuItem {
    pub fn new(
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            icon: None,
            danger: false,
            disabled: false,
            handler: Rc::new(handler),
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Destructive action: drawn in the error colour. Still confirm in the handler.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

struct MenuState {
    open: bool,
    highlighted: Option<usize>,
    focus: FocusHandle,
}

#[derive(IntoElement)]
pub struct MenuButton {
    id: ElementId,
    tooltip: SharedString,
    items: Vec<MenuItem>,
}

impl MenuButton {
    /// `id` must be unique among siblings, e.g. `("row-menu", ix)`.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            tooltip: "More actions".into(),
            items: Vec::new(),
        }
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = text.into();
        self
    }

    pub fn item(mut self, item: MenuItem) -> Self {
        self.items.push(item);
        self
    }
}

fn step(from: Option<usize>, items: &[(bool, Handler)], forward: bool) -> Option<usize> {
    let enabled: Vec<usize> = (0..items.len()).filter(|&i| !items[i].0).collect();
    if enabled.is_empty() {
        return None;
    }
    let pos = from.and_then(|f| enabled.iter().position(|&i| i == f));
    Some(match (pos, forward) {
        (None, true) => enabled[0],
        (None, false) => enabled[enabled.len() - 1],
        (Some(p), true) => enabled[(p + 1) % enabled.len()],
        (Some(p), false) => enabled[(p + enabled.len() - 1) % enabled.len()],
    })
}

impl RenderOnce for MenuButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state =
            window.use_keyed_state(self.id.clone(), cx, |_, cx: &mut Context<MenuState>| {
                MenuState {
                    open: false,
                    highlighted: None,
                    focus: cx.focus_handle(),
                }
            });
        let theme = cx.theme().clone();
        let open = state.read(cx).open;
        let highlighted = state.read(cx).highlighted;
        let focus = state.read(cx).focus.clone();
        // (disabled, handler) per item, shared by the key handler and the rows.
        let actions: Rc<Vec<(bool, Handler)>> = Rc::new(
            self.items
                .iter()
                .map(|i| (i.disabled, i.handler.clone()))
                .collect(),
        );

        let toggle = {
            let state = state.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                state.update(cx, |s, cx| {
                    s.open = !s.open;
                    s.highlighted = None;
                    if s.open {
                        window.focus(&s.focus, cx);
                    }
                    cx.notify();
                });
            }
        };

        let rows = self.items.into_iter().enumerate().map(|(ix, item)| {
            let state = state.clone();
            let handler = item.handler.clone();
            let color = if item.danger { theme.error } else { theme.text };
            div()
                .id(ix)
                .flex()
                .items_center()
                .gap_2()
                .h(px(26.))
                .px_2()
                .mx_1()
                .rounded_sm()
                .text_sm()
                .text_color(color)
                .when(highlighted == Some(ix), |el| el.bg(theme.element_hover))
                .when(item.disabled, |el| el.opacity(0.5))
                .when_some(item.icon, |el, icon| {
                    el.child(Icon::new(icon).size(px(14.)).color(color))
                })
                .child(item.label)
                .when(!item.disabled, |el| {
                    el.cursor_pointer()
                        .hover(|s| s.bg(theme.element_hover))
                        .on_click(move |event, window, cx| {
                            state.update(cx, |s, cx| {
                                s.open = false;
                                cx.notify();
                            });
                            handler(event, window, cx);
                        })
                })
        });

        div()
            .id(self.id)
            .flex_none()
            .child(
                IconButton::new("menu-trigger", IconName::Ellipsis)
                    .tooltip(self.tooltip)
                    .on_click(toggle),
            )
            .when(open, |el| {
                let key_state = state.clone();
                let close_state = state.clone();
                el.child(
                    deferred(
                        anchored().snap_to_window_with_margin(px(8.)).child(
                            div()
                                .id("menu")
                                .track_focus(&focus)
                                .occlude()
                                .min_w(px(160.))
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.elevated_surface_background)
                                .shadow_md()
                                .children(rows)
                                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                                    let key = event.keystroke.key.as_str();
                                    let mut run = None;
                                    key_state.update(cx, |s, cx| {
                                        match key {
                                            "down" => {
                                                s.highlighted = step(s.highlighted, &actions, true)
                                            }
                                            "up" => {
                                                s.highlighted = step(s.highlighted, &actions, false)
                                            }
                                            "escape" => s.open = false,
                                            "enter" | "space" => {
                                                if let Some(ix) = s.highlighted {
                                                    run = Some(actions[ix].1.clone());
                                                    s.open = false;
                                                }
                                            }
                                            _ => return,
                                        }
                                        cx.notify();
                                    });
                                    cx.stop_propagation();
                                    if let Some(handler) = run {
                                        handler(&ClickEvent::default(), window, cx);
                                    }
                                })
                                .on_mouse_down_out(move |_, _, cx| {
                                    close_state.update(cx, |s, cx| {
                                        s.open = false;
                                        cx.notify();
                                    });
                                }),
                        ),
                    )
                    .priority(3),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(disabled: &[bool]) -> Vec<(bool, Handler)> {
        disabled
            .iter()
            .map(|&d| {
                (
                    d,
                    Rc::new(|_: &ClickEvent, _: &mut Window, _: &mut App| {}) as Handler,
                )
            })
            .collect()
    }

    #[test]
    fn arrow_keys_skip_disabled_items_and_wrap() {
        let list = items(&[false, true, false]);
        assert_eq!(step(None, &list, true), Some(0));
        assert_eq!(step(Some(0), &list, true), Some(2));
        assert_eq!(step(Some(2), &list, true), Some(0));
        assert_eq!(step(None, &list, false), Some(2));
        assert_eq!(step(None, &items(&[true]), true), None);
    }
}
