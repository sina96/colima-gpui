//! Sidebar: profile rows in the style of Zed's project panel.

use gpui::{Context, Entity, Subscription, Window, div, prelude::*, px};

use crate::store::{LoadState, ProfileStore, Selection};
use crate::theme::ActiveTheme;
use crate::ui::status::DotState;
use crate::ui::{Button, StatusDot};

pub struct ProfileList {
    store: Entity<ProfileStore>,
    _observe: Subscription,
}

impl ProfileList {
    pub fn new(store: Entity<ProfileStore>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&store, |_, _, cx| cx.notify());
        Self {
            store,
            _observe: observe,
        }
    }
}

impl Render for ProfileList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let store = self.store.read(cx);
        let selection = store.selection().clone();
        // Creating a profile needs a working colima.
        let can_create = *store.load_state() == LoadState::Ready;
        let settings_selected = selection == Selection::Settings;

        let rows = store
            .instances()
            .iter()
            .enumerate()
            .map(|(ix, inst)| {
                let selected = selection == Selection::Profile(inst.name.clone());
                let busy = store.busy(&inst.name).map(|op| op.progress_label());
                let dot = DotState::profile(&inst.status, busy);
                let trailing = store
                    .busy(&inst.name)
                    .map_or(inst.runtime.clone(), |op| op.progress_label().to_owned());
                let name = inst.name.clone();

                div()
                    .id(("profile-row", ix))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(28.))
                    .px_3()
                    .text_sm()
                    .cursor_pointer()
                    .when(selected, |el| el.bg(theme.element_selected))
                    .when(!selected, |el| {
                        el.hover(|s| s.bg(theme.ghost_element_hover))
                    })
                    .child(StatusDot::new(("profile-status", ix), dot))
                    .child(div().flex_1().truncate().child(inst.name.clone()))
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(trailing),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store.update(cx, |store, cx| store.select(&name, cx));
                    }))
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.surface_background)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .h_8()
                    .pl_3()
                    .pr_1()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child("PROFILES")
                    .when(can_create, |el| {
                        el.child(
                            Button::new("new-profile", "+")
                                .tooltip("New profile")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.store
                                        .update(cx, |store, cx| store.select_new_profile(cx));
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .id("profile-rows")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(
                div()
                    .id("settings-item")
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(32.))
                    .px_3()
                    .border_t_1()
                    .border_color(theme.border_variant)
                    .text_sm()
                    .text_color(if settings_selected {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .cursor_pointer()
                    .when(settings_selected, |el| el.bg(theme.element_selected))
                    .when(!settings_selected, |el| {
                        el.hover(|s| s.bg(theme.ghost_element_hover).text_color(theme.text))
                    })
                    .child("Settings")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.store.update(cx, |store, cx| store.select_settings(cx));
                    })),
            )
    }
}
