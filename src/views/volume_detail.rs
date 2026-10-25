//! Body of the volume detail page; the owning tab manages polling and actions.
use crate::ui::ErrorNote;
use gpui::{Context, SharedString, div, prelude::*};

use super::navigate::{Navigate, Target};
use super::volumes::VolumesView;
use crate::docker::volumes::{Volume, VolumeDetails, format_size};
use crate::theme::ActiveTheme;
use crate::ui::{CardGrid, DetailCard, Field, IconName, Link};

pub(super) fn body(
    volume: Option<&Volume>,
    details: Option<&VolumeDetails>,
    error: Option<SharedString>,
    cx: &Context<VolumesView>,
) -> impl IntoElement + use<> {
    let theme = cx.theme().clone();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_2()
        .text_sm()
        .when(volume.is_none(), |el| {
            el.child(
                div()
                    .text_color(theme.text_muted)
                    .child("Volume no longer exists."),
            )
        })
        .when_some(error, |el, error| {
            el.child(ErrorNote::new("volume-detail-error", error))
        })
        .when(volume.is_some() && details.is_none(), |el| {
            el.child(
                div()
                    .text_color(theme.text_muted)
                    .child("Metadata unavailable; refresh to retry."),
            )
        })
        .when_some(volume, |el, volume| {
            let summary = DetailCard::new(IconName::Database, "Summary")
                .child(Field::new("Driver", volume.driver.clone()))
                .child(match details {
                    Some(details) => Field::new("Mountpoint", details.mountpoint.clone())
                        .mono()
                        .copyable(),
                    None => Field::new("Mountpoint", "Unavailable").muted(),
                })
                .child(
                    match details.filter(|details| !details.created_at.is_empty()) {
                        Some(details) => Field::new("Created", details.created_at.clone()).mono(),
                        None => Field::new("Created", "Unavailable").muted(),
                    },
                )
                .child(match volume.size_bytes {
                    Some(bytes) => Field::new("Size", format_size(bytes)),
                    None => Field::new("Size", "Unavailable").muted(),
                });
            let containers = DetailCard::new(IconName::Box, "Containers")
                .empty(volume.usage_description())
                .children(
                    volume
                        .containers
                        .iter()
                        .enumerate()
                        .map(|(index, container)| {
                            let id = container.id.clone();
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .min_w_0()
                                .child(
                                    Link::new(("volume-container", index), container.name.clone())
                                        .tooltip(format!("Open container {}", container.name))
                                        .on_click(cx.listener(move |_, _, _, cx| {
                                            cx.emit(Navigate(Target::Container(id.clone())))
                                        })),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .child(container.state.clone()),
                                )
                        }),
                );
            let labels = DetailCard::new(IconName::Tag, "Labels")
                .empty(if details.is_some() {
                    "No labels"
                } else {
                    "Labels unavailable"
                })
                .children(
                    details
                        .and_then(|details| details.labels.as_ref())
                        .into_iter()
                        .flat_map(|labels| labels.iter())
                        .map(|(key, value)| Field::new(key.clone(), value.clone())),
                );
            el.child(CardGrid::new().card(summary).card(containers).card(labels))
        })
}
