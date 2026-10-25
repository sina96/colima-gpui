//! Scaffold for item detail pages (container, image, volume) that replace a table.
//!
//! Pattern for a tab view:
//! - keep `detail: Option<Id>` and a `FocusHandle`;
//! - when a row is clicked, set `detail` and focus the handle (`window.focus(&handle, cx)`);
//! - in `render`, return a `DetailPage` instead of the table while `detail` is set;
//! - `.on_back(...)` clears `detail`. ← Back and Esc (via [`Back`]) both call it.
//!
//! Sub-tabs are optional: pass `(label, selected, on_click)` with `.tab(...)`.

use gpui::{
    AnyElement, App, ClickEvent, FocusHandle, SharedString, Window, actions, div, prelude::*,
};

use crate::theme::ActiveTheme;
use crate::ui::button::ButtonSize;
use crate::ui::{Button, ButtonStyle};

actions!(detail_page, [Back]);

pub const KEY_CONTEXT: &str = "DetailPage";

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type BackHandler = std::rc::Rc<dyn Fn(&mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct DetailPage {
    title: SharedString,
    status: Option<AnyElement>,
    subtitle: Option<SharedString>,
    actions: Vec<AnyElement>,
    tabs: Vec<(SharedString, bool, Handler)>,
    body: Option<AnyElement>,
    focus: Option<FocusHandle>,
    on_back: Option<BackHandler>,
}

impl DetailPage {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            status: None,
            subtitle: None,
            actions: Vec::new(),
            tabs: Vec::new(),
            body: None,
            focus: None,
            on_back: None,
        }
    }

    /// Usually a `StatusDot`, shown before the title.
    pub fn status(mut self, status: impl IntoElement) -> Self {
        self.status = Some(status.into_any_element());
        self
    }

    /// Short secondary line, e.g. the image of a container.
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Header actions on the right, e.g. `IconButton`s or `Button`s.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    pub fn tab(
        mut self,
        label: impl Into<SharedString>,
        selected: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.tabs.push((label.into(), selected, Box::new(on_click)));
        self
    }

    pub fn body(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }

    /// The owner's focus handle, so Esc reaches this page.
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    pub fn on_back(mut self, on_back: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_back = Some(std::rc::Rc::new(on_back));
        self
    }
}

impl RenderOnce for DetailPage {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let back_click = self.on_back.clone();
        let back_key = self.on_back.clone();

        div()
            .key_context(KEY_CONTEXT)
            .when_some(self.focus, |el, focus| el.track_focus(&focus))
            .on_action(move |_: &Back, window, cx| {
                if let Some(back) = &back_key {
                    back(window, cx);
                }
            })
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .h_12()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border_variant)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                Button::new("detail-back", "← Back")
                                    .size(ButtonSize::Medium)
                                    .tooltip("Back (Esc)")
                                    .on_click(move |_, window, cx| {
                                        if let Some(back) = &back_click {
                                            back(window, cx);
                                        }
                                    }),
                            )
                            .children(self.status)
                            .child(div().text_lg().truncate().child(self.title))
                            .when_some(self.subtitle, |el, subtitle| {
                                el.child(
                                    div()
                                        .text_sm()
                                        .truncate()
                                        .text_color(theme.text_muted)
                                        .child(subtitle),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap_1()
                            .children(self.actions),
                    ),
            )
            .when(!self.tabs.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap_1()
                        .h_8()
                        .px_2()
                        .border_b_1()
                        .border_color(theme.border_variant)
                        .children(self.tabs.into_iter().map(|(label, selected, on_click)| {
                            Button::new(SharedString::from(format!("detail-tab-{label}")), label)
                                .style(if selected {
                                    ButtonStyle::Selected
                                } else {
                                    ButtonStyle::Ghost
                                })
                                .on_click(on_click)
                        })),
                )
            })
            .child(
                div()
                    .id("detail-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(self.body),
            )
    }
}

/// Registers Esc → [`Back`] for detail pages.
pub fn init(cx: &mut App) {
    cx.bind_keys([gpui::KeyBinding::new("escape", Back, Some(KEY_CONTEXT))]);
}
