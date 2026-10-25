//! Image detail content. The Images view owns navigation, polling and confirmations.
use crate::ui::ErrorNote;
use std::rc::Rc;

use gpui::{Context, div, prelude::*, px};

use super::images::ImagesView;
use super::navigate::{Navigate, Target};
use crate::docker::images::{self, HistoryLayer, Image, ImageDetails};
use crate::theme::Theme;
use crate::ui::{
    CardGrid, Column, DetailCard, Field, IconName, InfoTip, Link, StatusDot, Table, TextCell,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ImageTab {
    #[default]
    Info,
    Layers,
    Containers,
}

pub struct ImageDetail {
    pub image: Image,
    pub tags: Vec<String>,
    pub data: Option<ImageDetails>,
    pub error: Option<String>,
    pub tab: ImageTab,
    pub expanded_layer: Option<usize>,
}

impl ImageDetail {
    pub fn body(&self, theme: &Theme, cx: &Context<ImagesView>) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .text_sm()
            .when_some(self.error.clone(), |el, error| {
                el.child(
                    div()
                        .p_2()
                        .child(ErrorNote::new("image-detail-error", error)),
                )
            })
            .when(self.data.is_none() && self.error.is_none(), |el| {
                el.child(div().p_2().child("Loading image details…"))
            })
            .when_some(self.data.as_ref(), |el, data| match self.tab {
                ImageTab::Info => el.child(self.info(data)),
                ImageTab::Layers => el.child(self.layers(data, theme, cx)),
                ImageTab::Containers => el.child(self.containers(data, theme, cx)),
            })
    }

    fn info(&self, data: &ImageDetails) -> impl IntoElement + use<> {
        let inspect = &data.inspect;
        let config = inspect.config.clone().unwrap_or_default();
        let tags = inspect
            .repo_tags
            .as_ref()
            .filter(|tags| !tags.is_empty())
            .unwrap_or(&self.tags);
        let arch = if inspect.variant.is_empty() {
            format!("{}/{}", inspect.os, inspect.architecture)
        } else {
            format!(
                "{}/{}/{}",
                inspect.os, inspect.architecture, inspect.variant
            )
        };
        let command = |args: Option<Vec<String>>| {
            args.filter(|args| !args.is_empty())
                .map(|args| serde_json::to_string(&args).expect("string arguments serialize"))
                .unwrap_or_else(|| "—".into())
        };
        let ports = config
            .exposed_ports
            .unwrap_or_default()
            .into_keys()
            .collect::<Vec<_>>()
            .join(", ");
        let env = config.env.unwrap_or_default();
        let labels = config.labels.unwrap_or_default();
        div().p_2().child(
            CardGrid::new()
                .card(
                    DetailCard::new(IconName::Tag, "Summary")
                        .child(
                            Field::new(
                                "Tags",
                                if tags.is_empty() {
                                    "<none>".into()
                                } else {
                                    tags.join(", ")
                                },
                            )
                            .mono()
                            .copyable(),
                        )
                        .child(Field::new("ID", inspect.id.clone()).mono().copyable())
                        .child(
                            Field::new("Created", placeholder(&inspect.created))
                                .mono()
                                .copyable(),
                        )
                        .child(Field::new("Size", images::format_size(inspect.size)))
                        .child(Field::new(
                            "Unique",
                            self.image
                                .unique_size
                                .map(images::format_size)
                                .unwrap_or_else(|| "—".into()),
                        ))
                        .child(Field::new(
                            "Shared",
                            self.image
                                .shared_size
                                .map(images::format_size)
                                .unwrap_or_else(|| "—".into()),
                        )),
                )
                .card(
                    DetailCard::new(IconName::Terminal, "Configuration")
                        .child(Field::new("OS / arch", arch).mono())
                        .child(
                            Field::new("Entrypoint", command(config.entrypoint))
                                .mono()
                                .copyable(),
                        )
                        .child(Field::new("Command", command(config.cmd)).mono().copyable())
                        .child(
                            Field::new("Exposed ports", placeholder(&ports))
                                .mono()
                                .copyable(),
                        ),
                )
                .card(
                    DetailCard::new(IconName::FileText, "Environment").child(
                        div()
                            .id("image-environment")
                            .max_h(px(96.))
                            .overflow_y_scroll()
                            .when(env.is_empty(), |el| el.child("No environment variables."))
                            .children(env.into_iter().map(|entry| {
                                let (key, value) = entry.split_once('=').unwrap_or((&entry, ""));
                                Field::new(key.to_owned(), value.to_owned())
                                    .mono()
                                    .copyable()
                            })),
                    ),
                )
                .card(
                    DetailCard::new(IconName::Tag, "Labels").child(
                        div()
                            .id("image-labels")
                            .max_h(px(96.))
                            .overflow_y_scroll()
                            .when(labels.is_empty(), |el| el.child("No labels."))
                            .children(
                                labels
                                    .into_iter()
                                    .map(|(key, value)| Field::new(key, value).mono().copyable()),
                            ),
                    ),
                ),
        )
    }

    fn containers(
        &self,
        data: &ImageDetails,
        theme: &Theme,
        cx: &Context<ImagesView>,
    ) -> impl IntoElement + use<> {
        let view = cx.entity().downgrade();
        div().p_2().child(
            DetailCard::new(IconName::Box, format!("Containers ({})", data.containers.len()))
                .action(InfoTip::new("image-ancestor-info", "Includes stopped containers and containers using descendant images, as reported by Docker's ancestor filter."))
                .empty("No containers use this image.")
                .children(data.containers.iter().enumerate().map(|(ix, container)| {
                    let view = view.clone();
                    let id = container.id.clone();
                    div().flex().items_center().gap_2().min_h(px(28.))
                        .child(StatusDot::for_container(("image-container", ix), &container.state, &container.status))
                        .child(div().flex_1().min_w_0().child(
                            Link::new(("image-container-link", ix), container.names.clone())
                                .tooltip(container.status.clone())
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |_, cx| cx.emit(Navigate(Target::Container(id.clone())))).ok();
                                })))
                        .child(div().text_color(theme.text_muted).child(container.state.clone()))
                })),
        )
    }

    fn layers(
        &self,
        data: &ImageDetails,
        theme: &Theme,
        cx: &Context<ImagesView>,
    ) -> impl IntoElement + use<> {
        let layers = Rc::new(data.history.clone());
        let expanded = self.expanded_layer.filter(|ix| *ix < layers.len());
        let split = expanded.map(|ix| ix + 1).unwrap_or(layers.len());
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_color(theme.text_muted)
                    .child("Newest first · Click a layer to expand its command and exact date."),
            )
            .child(layer_table(layers.clone(), 0, split, expanded, cx))
            .when_some(expanded, |el, ix| {
                let layer = &layers[ix];
                el.child(
                    div()
                        .p_2()
                        .bg(theme.surface_background)
                        .border_b_1()
                        .border_color(theme.border_variant)
                        .child(
                            DetailCard::new(IconName::Layers, "Layer details")
                                .child(
                                    Field::new("Created", layer.created_at.clone())
                                        .mono()
                                        .copyable(),
                                )
                                .child(
                                    Field::new("Command", layer.created_by.clone())
                                        .mono()
                                        .wrap()
                                        .copyable(),
                                )
                                .when(!layer.comment.is_empty(), |card| {
                                    card.child(
                                        Field::new("Comment", layer.comment.clone()).mono().wrap(),
                                    )
                                }),
                        ),
                )
            })
            .when(split < layers.len(), |el| {
                el.child(layer_table(
                    layers.clone(),
                    split,
                    layers.len(),
                    expanded,
                    cx,
                ))
            })
    }
}

fn placeholder(value: &str) -> String {
    if value.is_empty() {
        "—".into()
    } else {
        value.into()
    }
}

/// The shared table virtualizes fixed-height rows. Split it around the expanded row
/// so the full command flows inline at its natural height without changing the widget.
fn layer_table(
    layers: Rc<Vec<HistoryLayer>>,
    start: usize,
    end: usize,
    expanded: Option<usize>,
    cx: &Context<ImagesView>,
) -> impl IntoElement + use<> {
    let view = cx.entity().downgrade();
    let count = end - start;
    // The rows after an expanded layer continue the table above, so they skip the header.
    let header = start == 0;
    div()
        .h(px(if header { 30. } else { 0. } + count.max(1) as f32 * 36.))
        .flex_none()
        .child(
            Table::new(
                ("image-layers", start),
                vec![
                    Column::flex("Instruction", 1.),
                    Column::fixed("Size", px(90.)).right(),
                    Column::fixed("Age", px(120.)),
                ],
                count,
                move |ix, _, _| {
                    let layer = &layers[start + ix];
                    let marker = if expanded == Some(start + ix) {
                        "▾"
                    } else {
                        "▸"
                    };
                    vec![
                        TextCell::new(
                            ("layer-instruction", start + ix),
                            format!("{marker} {}", layer.created_by.replace(['\n', '\r'], " ")),
                        )
                        .mono()
                        .into_any_element(),
                        TextCell::new(("layer-size", start + ix), layer.size.clone())
                            .mono()
                            .into_any_element(),
                        TextCell::new(
                            ("layer-age", start + ix),
                            if layer.created_since.is_empty() {
                                layer.created_at.clone()
                            } else {
                                layer.created_since.clone()
                            },
                        )
                        .muted()
                        .into_any_element(),
                    ]
                },
            )
            .selected(
                expanded
                    .filter(|ix| *ix >= start && *ix < end)
                    .map(|ix| ix - start),
            )
            .on_row_click(move |ix, _, cx| {
                view.update(cx, |this, cx| this.toggle_layer(start + ix, cx))
                    .ok();
            })
            .empty(div().p_3().child("No layer history."))
            .when(!header, |table| table.without_header()),
        )
}
