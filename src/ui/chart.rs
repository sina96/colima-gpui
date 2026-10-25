//! Storage visuals: `StackedBar` (share of a total, with a reclaimable part per segment),
//! `SizeBar` (thin per-row bar scaled to the largest row) and `Meter` (used vs total).
//! Colours come from `theme.chart[CHART_*]` so every chart agrees on categories.

use gpui::{App, Hsla, SharedString, Window, div, prelude::*, px, relative};

use crate::theme::ActiveTheme;

#[derive(Clone)]
pub struct Segment {
    pub label: SharedString,
    /// Shown in the legend, already formatted (e.g. "20.7 GB").
    pub value_label: SharedString,
    pub value: f64,
    /// Part of `value` that could be freed; drawn lighter. 0 for none.
    pub reclaimable: f64,
    pub color: Hsla,
}

/// One bar split into segments proportional to their values, plus a legend row.
#[derive(IntoElement)]
pub struct StackedBar {
    segments: Vec<Segment>,
    legend: bool,
}

impl StackedBar {
    pub fn new(segments: Vec<Segment>) -> Self {
        Self {
            segments,
            legend: true,
        }
    }

    pub fn without_legend(mut self) -> Self {
        self.legend = false;
        self
    }
}

/// Fractions of the bar per segment: (used, reclaimable), both of the whole bar.
fn fractions(segments: &[Segment]) -> Vec<(f32, f32)> {
    let total: f64 = segments.iter().map(|s| s.value.max(0.)).sum();
    if total <= 0. {
        return segments.iter().map(|_| (0., 0.)).collect();
    }
    segments
        .iter()
        .map(|s| {
            let value = s.value.max(0.);
            let reclaimable = s.reclaimable.clamp(0., value);
            (
                ((value - reclaimable) / total) as f32,
                (reclaimable / total) as f32,
            )
        })
        .collect()
}

impl RenderOnce for StackedBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let parts = fractions(&self.segments);
        let bar = div()
            .flex()
            .w_full()
            .h(px(14.))
            .rounded_sm()
            .overflow_hidden()
            .bg(theme.element_background)
            .children(
                self.segments
                    .iter()
                    .zip(&parts)
                    .flat_map(|(segment, &(used, free))| {
                        [
                            div().h_full().w(relative(used)).bg(segment.color),
                            div()
                                .h_full()
                                .w(relative(free))
                                .bg(segment.color.opacity(0.35)),
                        ]
                    }),
            );
        let any_reclaimable = self.segments.iter().any(|s| s.reclaimable > 0.);

        div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .child(bar)
            .when(self.legend, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x_4()
                        .gap_y_1()
                        .text_xs()
                        .children(self.segments.iter().map(|segment| {
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(div().size(px(8.)).rounded_sm().bg(segment.color))
                                .child(
                                    div()
                                        .text_color(theme.text_muted)
                                        .child(segment.label.clone()),
                                )
                                .child(segment.value_label.clone())
                        }))
                        .when(any_reclaimable, |el| {
                            el.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .size(px(8.))
                                            .rounded_sm()
                                            .bg(theme.text_muted.opacity(0.35)),
                                    )
                                    .child(
                                        div()
                                            .text_color(theme.text_muted)
                                            .child("lighter = reclaimable"),
                                    ),
                            )
                        }),
                )
            })
    }
}

/// Thin bar for a table cell, scaled against the largest value in the table.
#[derive(IntoElement)]
pub struct SizeBar {
    fraction: f32,
    color: Option<Hsla>,
}

impl SizeBar {
    pub fn new(value: f64, max: f64) -> Self {
        let fraction = if max > 0. {
            (value / max).clamp(0., 1.) as f32
        } else {
            0.
        };
        Self {
            fraction,
            color: None,
        }
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl RenderOnce for SizeBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .w_full()
            .h(px(4.))
            .rounded_full()
            .bg(theme.element_background)
            .child(
                div()
                    .h_full()
                    .rounded_full()
                    .w(relative(self.fraction))
                    .bg(self.color.unwrap_or(theme.accent)),
            )
    }
}

/// "Used of total" gauge: a label row and a bar that turns warning/error as it fills.
#[derive(IntoElement)]
pub struct Meter {
    label: SharedString,
    value_label: SharedString,
    fraction: f32,
    color: Option<Hsla>,
}

impl Meter {
    /// `value_label` is the formatted reading, e.g. "1.2 of 7.7 GiB".
    pub fn new(
        label: impl Into<SharedString>,
        value_label: impl Into<SharedString>,
        used: f64,
        total: f64,
    ) -> Self {
        let fraction = if total > 0. {
            (used / total).clamp(0., 1.) as f32
        } else {
            0.
        };
        Self {
            label: label.into(),
            value_label: value_label.into(),
            fraction,
            color: None,
        }
    }

    /// Fixed colour instead of the success/warning/error thresholds.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl RenderOnce for Meter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let color = self.color.unwrap_or(match self.fraction {
            f if f >= 0.9 => theme.error,
            f if f >= 0.75 => theme.warning,
            _ => theme.success,
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_xs()
                    .child(div().text_color(theme.text_muted).child(self.label))
                    .child(self.value_label),
            )
            .child(
                div()
                    .w_full()
                    .h(px(6.))
                    .rounded_full()
                    .bg(theme.element_background)
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .w(relative(self.fraction))
                            .bg(color),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::black;

    fn seg(value: f64, reclaimable: f64) -> Segment {
        Segment {
            label: "x".into(),
            value_label: "x".into(),
            value,
            reclaimable,
            color: black(),
        }
    }

    #[test]
    fn fractions_split_used_and_reclaimable() {
        let parts = fractions(&[seg(75., 25.), seg(25., 0.)]);
        assert_eq!(parts, vec![(0.5, 0.25), (0.25, 0.)]);
    }

    #[test]
    fn fractions_handle_empty_and_bad_input() {
        assert_eq!(fractions(&[seg(0., 0.)]), vec![(0., 0.)]);
        // Reclaimable larger than the value is clamped; negatives count as zero.
        assert_eq!(
            fractions(&[seg(10., 50.), seg(-5., 0.)]),
            vec![(0., 1.), (0., 0.)]
        );
    }
}
