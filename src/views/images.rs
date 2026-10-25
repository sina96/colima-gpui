//! Images for one socket. The visible tab owns this view and its poller.

use crate::ui::ErrorNote;
use std::rc::Rc;

use super::image_detail::{ImageDetail, ImageTab};
use super::navigate::{self, Navigate};
use crate::docker::images::{self, Image};
use crate::theme::{ActiveTheme, CHART_IMAGES};
use crate::ui::dialog::{ConfirmDialog, DialogResult};
use crate::ui::{
    Button, ButtonStyle, Column, DetailPage, InfoTip, SizeBar, StatusDot, Table, TextCell,
};
use gpui::{
    Context, Entity, EventEmitter, SharedString, Subscription, Task, Window, div, prelude::*, px,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Dangling,
    Unused,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Size,
    Created,
}

pub struct ImagesView {
    socket: String,
    detail: Option<ImageDetail>,
    detail_focus: gpui::FocusHandle,
    detail_request: u64,
    detail_loading: bool,
    /// Cross-link target to open once the list has loaded (see `navigate`).
    pending_target: Option<String>,
    /// Focus the detail page on the next render; set where no `Window` is at hand.
    focus_detail: bool,
    images: Vec<Image>,
    filter: Filter,
    sort: Sort,
    loaded: bool,
    refreshing: bool,
    refresh_again: bool,
    busy: bool,
    error: Option<SharedString>,
    list_error: Option<SharedString>,
    confirm: Option<(Entity<ConfirmDialog>, Subscription)>,
    remove_dialog: Option<(Entity<ImageRemoveDialog>, Subscription)>,
    _poll: Task<()>,
}

impl ImagesView {
    pub fn new(socket: String, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let Ok(interval) = this.update(cx, |this, cx| {
                    this.refresh(cx);
                    crate::store::poll_interval(cx)
                }) else {
                    break;
                };
                cx.background_executor().timer(interval).await;
            }
        });
        Self {
            socket,
            detail: None,
            detail_focus: cx.focus_handle(),
            detail_request: 0,
            detail_loading: false,
            pending_target: None,
            focus_detail: false,
            images: Vec::new(),
            filter: Filter::All,
            sort: Sort::Size,
            loaded: false,
            refreshing: false,
            refresh_again: false,
            busy: false,
            error: None,
            list_error: None,
            confirm: None,
            remove_dialog: None,
            _poll: poll,
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.refreshing {
            self.refresh_again = true;
            return;
        }
        self.refreshing = true;
        self.refresh_detail(cx);
        let socket = self.socket.clone();
        let task = cx
            .background_executor()
            .spawn(async move { images::list(&socket) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.refreshing = false;
                this.loaded = true;
                match result {
                    Ok(images) => {
                        if let Some(detail) = &mut this.detail
                            && let Some(image) = images.iter().find(|image| {
                                image.id == detail.image.id && image.name() == detail.image.name()
                            })
                        {
                            detail.image = image.clone();
                            detail.tags = images
                                .iter()
                                .filter(|image| image.id == detail.image.id)
                                .map(Image::name)
                                .collect();
                        }
                        this.images = images;
                        this.list_error = None;
                    }
                    Err(error) => this.list_error = Some(error.to_string().into()),
                }
                this.resolve_target(cx);
                if std::mem::take(&mut this.refresh_again) {
                    this.refresh(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn run(&mut self, target: Option<(String, bool)>, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let removed = target.as_ref().map(|(target, _)| target.clone());
        let socket = self.socket.clone();
        let task = cx.background_executor().spawn(async move {
            match target {
                Some((target, force)) => images::remove(&socket, &target, force),
                None => images::prune(&socket),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Err(error) => this.error = Some(error.to_string().into()),
                    Ok(()) => {
                        if this
                            .detail
                            .as_ref()
                            .is_some_and(|detail| Some(detail.image.remove_target()) == removed)
                        {
                            this.detail = None;
                            this.detail_request += 1;
                            this.detail_loading = false;
                        }
                    }
                }
                this.refresh(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn confirm_prune(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.confirm.is_some() || self.remove_dialog.is_some() {
            return;
        }
        let estimate = images::reclaimable(&self.images)
            .map(images::format_size)
            .unwrap_or_else(|| "unknown".into());
        let dialog = cx.new(|cx| ConfirmDialog::new("Remove all unused images?",
            format!("Estimated reclaimable unique data: {estimate}. Removes every image with no containers in this profile, including tagged images. Shared layers may free additional space. Docker rechecks usage when removing."), cx).confirm_label("Remove all unused"));
        let subscription = cx.subscribe(&dialog, |this, _, result, cx| {
            this.confirm = None;
            if *result == DialogResult::Confirmed {
                this.run(None, cx);
            }
            cx.notify();
        });
        dialog.update(cx, |dialog, cx| dialog.show(window, cx));
        self.confirm = Some((dialog, subscription));
        cx.notify();
    }

    fn confirm_remove(&mut self, image: Image, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.confirm.is_some() || self.remove_dialog.is_some() {
            return;
        }
        let target = image.remove_target();
        let dialog = cx.new(|cx| ImageRemoveDialog::new(
            format!("Remove “{}”?", image.name()),
            "Removes this tag, or the image if it has no remaining tags. Force may remove references used by stopped containers.", cx));
        let subscription = cx.subscribe(&dialog, move |this, _, result: &RemoveResult, cx| {
            this.remove_dialog = None;
            if let Some(force) = result.0 {
                this.run(Some((target.clone(), force)), cx);
            }
            cx.notify();
        });
        dialog.update(cx, |dialog, cx| dialog.show(window, cx));
        self.remove_dialog = Some((dialog, subscription));
        cx.notify();
    }

    fn open_detail(&mut self, image: Image, window: &mut Window, cx: &mut Context<Self>) {
        self.show_detail(image, cx);
        window.focus(&self.detail_focus, cx);
    }

    /// Opens an image's detail page from a cross-link, once the list has it.
    pub fn open_target(&mut self, key: String, cx: &mut Context<Self>) {
        self.pending_target = Some(key);
        self.resolve_target(cx);
    }

    fn resolve_target(&mut self, cx: &mut Context<Self>) {
        if !self.loaded {
            return;
        }
        let Some(key) = self.pending_target.take() else {
            return;
        };
        match self
            .images
            .iter()
            .find(|i| navigate::image_matches(&i.id, &i.repository, &i.tag, &key))
        {
            Some(image) => {
                self.show_detail(image.clone(), cx);
                self.focus_detail = true;
            }
            None => self.error = Some(format!("Image “{key}” no longer exists.").into()),
        }
        cx.notify();
    }

    fn show_detail(&mut self, image: Image, cx: &mut Context<Self>) {
        let tags = self
            .images
            .iter()
            .filter(|row| row.id == image.id)
            .map(Image::name)
            .collect();
        self.error = None;
        self.detail_loading = false;
        self.detail = Some(ImageDetail {
            image,
            tags,
            data: None,
            error: None,
            tab: ImageTab::Info,
            expanded_layer: None,
        });
        self.refresh_detail(cx);
        cx.notify();
    }

    pub(super) fn toggle_layer(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(detail) = &mut self.detail {
            detail.expanded_layer = if detail.expanded_layer == Some(index) {
                None
            } else {
                Some(index)
            };
            cx.notify();
        }
    }

    fn refresh_detail(&mut self, cx: &mut Context<Self>) {
        if self.detail_loading {
            return;
        }
        let Some(detail) = &self.detail else {
            return;
        };
        let id = detail.image.id.clone();
        let socket = self.socket.clone();
        self.detail_loading = true;
        self.detail_request += 1;
        let request = self.detail_request;
        let task = cx
            .background_executor()
            .spawn(async move { images::details(&socket, &id) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if request != this.detail_request {
                    return;
                }
                this.detail_loading = false;
                if let Some(detail) = &mut this.detail {
                    match result {
                        Ok(data) => {
                            detail.data = Some(data);
                            detail.error = None;
                        }
                        Err(error) => detail.error = Some(error.to_string()),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn visible_images(&self) -> Vec<Image> {
        let mut images: Vec<_> = self
            .images
            .iter()
            .filter(|image| match self.filter {
                Filter::All => true,
                Filter::Dangling => image.dangling(),
                Filter::Unused => image.containers == Some(0),
            })
            .cloned()
            .collect();
        images.sort_by(|a, b| {
            match self.sort {
                Sort::Size => images::size_bytes(&b.size).cmp(&images::size_bytes(&a.size)),
                Sort::Created => b.created_at.cmp(&a.created_at),
            }
            .then_with(|| a.name().cmp(&b.name()))
        });
        images
    }
}

impl EventEmitter<Navigate> for ImagesView {}

impl Render for ImagesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.focus_detail) {
            window.focus(&self.detail_focus, cx);
        }
        let theme = cx.theme().clone();
        if let Some(detail) = &self.detail {
            let image = detail.image.clone();
            let remove = image.clone();
            let mut page = DetailPage::new(image.name())
                .status(StatusDot::for_image(
                    "image-detail-status",
                    image.containers.unwrap_or(0) > 0,
                    image.dangling(),
                ))
                .subtitle(format!("{} · {}", image.size, image.created_at))
                .track_focus(&self.detail_focus)
                .on_back({
                    let this = cx.entity().downgrade();
                    move |_, cx| {
                        this.update(cx, |this, cx| {
                            this.detail = None;
                            this.detail_request += 1;
                            this.detail_loading = false;
                            cx.notify();
                        })
                        .ok();
                    }
                })
                .action(
                    Button::new("image-detail-remove", "Remove")
                        .style(ButtonStyle::Danger)
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.confirm_remove(remove.clone(), window, cx)
                        })),
                )
                .body(
                    div()
                        .flex()
                        .flex_col()
                        .when_some(
                            self.error.clone().or(self.list_error.clone()),
                            |el, error| {
                                el.child(
                                    div()
                                        .px_3()
                                        .py_1()
                                        .child(ErrorNote::new("images-row-error", error)),
                                )
                            },
                        )
                        .child(detail.body(&theme, cx)),
                );
            for (tab, label) in [
                (ImageTab::Info, "Info"),
                (ImageTab::Layers, "Layers"),
                (ImageTab::Containers, "Containers"),
            ] {
                page = page.tab(
                    label,
                    detail.tab == tab,
                    cx.listener(move |this, _, _, cx| {
                        if let Some(detail) = &mut this.detail {
                            detail.tab = tab;
                            cx.notify();
                        }
                    }),
                );
            }
            return div()
                .size_full()
                .text_sm()
                .text_color(theme.text)
                .bg(theme.editor_background)
                .child(page)
                .when_some(self.remove_dialog.as_ref(), |el, (dialog, _)| {
                    el.child(dialog.clone())
                })
                .into_any_element();
        }
        let visible = Rc::new(self.visible_images());
        let max = visible
            .iter()
            .filter_map(|image| images::size_bytes(&image.size))
            .max()
            .unwrap_or(0);
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .text_sm()
            .text_color(theme.text)
            .bg(theme.editor_background)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .h_8()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border_variant)
                    .child(format!("{} image tags", visible.len()))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new("refresh-images", "Refresh")
                                    .loading(self.refreshing)
                                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                            )
                            .child(
                                Button::new("prune-images", "Remove all unused")
                                    .style(ButtonStyle::Danger)
                                    .disabled(
                                        self.busy || !self.loaded || self.list_error.is_some(),
                                    )
                                    .loading(self.busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_prune(window, cx)
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .h_8()
                    .children(
                        [
                            (Filter::All, "All"),
                            (Filter::Dangling, "Dangling"),
                            (Filter::Unused, "Unused"),
                        ]
                        .into_iter()
                        .map(|(filter, label)| {
                            Button::new(label, label)
                                .style(if self.filter == filter {
                                    ButtonStyle::Selected
                                } else {
                                    ButtonStyle::Ghost
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.filter = filter;
                                    cx.notify();
                                }))
                        }),
                    )
                    .child(InfoTip::new("image-filter-info", "Dangling images have no tags. Unused images have no containers. Size bars compare the visible images."))
                    .child(div().ml_3().text_color(theme.text_muted).child("Sort:"))
                    .children(
                        [(Sort::Size, "Size"), (Sort::Created, "Created")]
                            .into_iter()
                            .map(|(sort, label)| {
                                Button::new(label, label)
                                    .style(if self.sort == sort {
                                        ButtonStyle::Selected
                                    } else {
                                        ButtonStyle::Ghost
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.sort = sort;
                                        cx.notify();
                                    }))
                            }),
                    ),
            )
            .when_some(
                self.error.clone().or(self.list_error.clone()),
                |el, error| el.child(div().px_2().py_1().child(ErrorNote::new("images-error", error))),
            )
            .child(
                div()
                    .id("image-table")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child({
                        let rows = visible.clone();
                        let clicked = visible.clone();
                        let this = cx.entity().downgrade();
                        Table::new(
                            "images",
                            vec![
                                Column::fixed("", px(16.)),
                                Column::flex("Repository:tag", 1.),
                                Column::fixed("Size", px(120.)).right(),
                                Column::fixed("Created", px(165.)),
                            ],
                            rows.len(),
                            move |index, _, cx| {
                                let image = &rows[index];
                                vec![
                                    StatusDot::for_image(
                                        ("image-status", index),
                                        image.containers.unwrap_or(0) > 0,
                                        image.dangling(),
                                    ).into_any_element(),
                                    TextCell::new(("image-name", index), image.name())
                                        .into_any_element(),
                                    div().flex().flex_col().items_end().gap_1().w_full()
                                        .child(TextCell::new(("image-size", index), image.size.clone()).mono())
                                        .child(SizeBar::new(
                                            images::size_bytes(&image.size).unwrap_or(0) as f64,
                                            max as f64,
                                        ).color(cx.theme().chart[CHART_IMAGES]))
                                        .into_any_element(),
                                    TextCell::new(("image-created", index), image.created_at.clone())
                                        .mono().into_any_element(),
                                ]
                            },
                        )
                        .on_row_click(move |index, window, cx| {
                            this.update(cx, |this, cx| {
                                this.open_detail(clicked[index].clone(), window, cx);
                            }).ok();
                        })
                        .empty(div().p_4().text_color(theme.text_muted).child(
                            if !self.loaded {
                                "Loading images…"
                            } else {
                                "No images match this filter."
                            },
                        ))
                    }),
            )
            .when_some(self.confirm.as_ref(), |el, (dialog, _)| {
                el.child(dialog.clone())
            })
            .when_some(self.remove_dialog.as_ref(), |el, (dialog, _)| {
                el.child(dialog.clone())
            }).into_any_element()
    }
}

// Local extension of ConfirmDialog: force is opt-in and resets for each image.
use gpui::{FocusHandle, anchored, deferred, point};

#[derive(Clone, Copy)]
struct RemoveResult(Option<bool>);

pub struct ImageRemoveDialog {
    title: SharedString,
    message: SharedString,
    force: bool,
    force_focus: FocusHandle,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    confirm_focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    open: bool,
}
impl EventEmitter<RemoveResult> for ImageRemoveDialog {}
impl ImageRemoveDialog {
    pub fn new(
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            force: false,
            force_focus: cx.focus_handle(),
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            confirm_focus: cx.focus_handle(),
            previous_focus: None,
            open: false,
        }
    }
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.previous_focus = window.focused(cx);
        self.open = true;
        window.focus(&self.cancel_focus, cx);
        cx.notify();
    }
    fn finish(&mut self, result: RemoveResult, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if let Some(focus) = self.previous_focus.take() {
            window.focus(&focus, cx);
        }
        cx.emit(result);
        cx.notify();
    }
}
impl Render for ImageRemoveDialog {
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
                                        "escape" => this.finish(RemoveResult(None), window, cx),
                                        "tab" => {
                                            let target = match (
                                                this.cancel_focus.is_focused(window),
                                                this.force_focus.is_focused(window),
                                                event.keystroke.modifiers.shift,
                                            ) {
                                                (true, _, false) | (false, false, true) => {
                                                    &this.force_focus
                                                }
                                                (true, _, true) | (_, true, false) => {
                                                    &this.confirm_focus
                                                }
                                                _ => &this.cancel_focus,
                                            };
                                            window.focus(target, cx);
                                        }
                                        "space" | "enter" => {
                                            if this.force_focus.is_focused(window) {
                                                this.force = !this.force;
                                                cx.notify();
                                            } else if this.confirm_focus.is_focused(window) {
                                                this.finish(
                                                    RemoveResult(Some(this.force)),
                                                    window,
                                                    cx,
                                                );
                                            } else {
                                                this.finish(RemoveResult(None), window, cx);
                                            }
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
                                            .id("force-image-removal")
                                            .role(gpui::Role::CheckBox)
                                            .aria_label("Force removal")
                                            .aria_toggled(if self.force {
                                                gpui::Toggled::True
                                            } else {
                                                gpui::Toggled::False
                                            })
                                            .track_focus(&self.force_focus)
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .h(px(22.))
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(gpui::transparent_black())
                                            .focus_visible(|s| s.border_color(t.border_focused))
                                            .hover(|s| s.bg(t.ghost_element_hover))
                                            .cursor_pointer()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .size(px(16.))
                                                    .rounded_sm()
                                                    .border_1()
                                                    .border_color(t.border)
                                                    .child(if self.force { "✓" } else { "" }),
                                            )
                                            .child("Force removal")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.force = !this.force;
                                                cx.notify();
                                            })),
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
                                                                RemoveResult(None),
                                                                window,
                                                                cx,
                                                            )
                                                        },
                                                    )),
                                            )
                                            .child(
                                                Button::new("confirm", "Remove")
                                                    .style(ButtonStyle::Danger)
                                                    .track_focus(&self.confirm_focus)
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.finish(
                                                                RemoveResult(Some(this.force)),
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
