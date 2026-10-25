//! Linux title-bar controls and client-side resize hit areas.
//!
//! Kept private to the app title bar: the deferred, window-anchored resize areas
//! cover all eight edges without changing the content layout or macOS chrome.

use gpui::{
    App, Bounds, CursorStyle, Decorations, IntoElement, MouseButton, PathBuilder, Pixels,
    RenderOnce, ResizeEdge, Tiling, Window, anchored, canvas, deferred, div, point, prelude::*, px,
    size,
};

use crate::theme::ActiveTheme;

#[derive(IntoElement)]
pub struct WindowControls;

impl RenderOnce for WindowControls {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Decorations::Client { tiling } = window.window_decorations() else {
            return div().into_any_element();
        };
        if window.is_fullscreen() {
            return div().into_any_element();
        }
        let supported = window.window_controls();
        let maximized = window.is_maximized();
        let controls = div()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .when(supported.minimize, |el| {
                el.child(control(Control::Minimize, window.is_minimizable(), cx))
            })
            .when(supported.maximize, |el| {
                el.child(control(
                    if maximized {
                        Control::Restore
                    } else {
                        Control::Maximize
                    },
                    window.is_resizable(),
                    cx,
                ))
            })
            .child(control(Control::Close, true, cx));

        div()
            .child(controls)
            .when(window.is_resizable() && !maximized, |el| {
                // Like Zed's client-side decoration frame, only untiled edges
                // accept resize input. Deferred drawing keeps these above content.
                el.child(deferred(
                    anchored()
                        .position(point(px(0.), px(0.)))
                        .child(resize_areas(window.viewport_size(), tiling)),
                ))
            })
            .into_any_element()
    }
}

#[derive(Clone, Copy)]
enum Control {
    Minimize,
    Maximize,
    Restore,
    Close,
}

fn control(kind: Control, enabled: bool, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let label = match kind {
        Control::Minimize => "Minimize",
        Control::Maximize => "Maximize",
        Control::Restore => "Restore",
        Control::Close => "Close",
    };
    let color = if enabled {
        theme.text_muted
    } else {
        theme.text_placeholder
    };
    div()
        .id(label)
        .flex()
        .items_center()
        .justify_center()
        .size(px(20.))
        .rounded_sm()
        .when(enabled, |el| {
            el.cursor_pointer()
                .hover(|el| el.bg(theme.ghost_element_hover))
        })
        .tooltip(crate::ui::tooltip::tooltip(label))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            if enabled {
                match kind {
                    Control::Minimize => window.minimize_window(),
                    Control::Maximize | Control::Restore => window.zoom_window(),
                    Control::Close => window.remove_window(),
                }
            }
        })
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let mut path = PathBuilder::stroke(px(1.));
                    let mut line = |points: &[(f32, f32)]| {
                        for (ix, &(x, y)) in points.iter().enumerate() {
                            let p = bounds.origin + point(px(x), px(y));
                            if ix == 0 {
                                path.move_to(p);
                            } else {
                                path.line_to(p);
                            }
                        }
                    };
                    match kind {
                        Control::Minimize => line(&[(3., 8.), (13., 8.)]),
                        Control::Maximize => {
                            line(&[(3., 3.), (13., 3.), (13., 13.), (3., 13.), (3., 3.)])
                        }
                        Control::Restore => {
                            line(&[(5., 5.), (5., 2.), (14., 2.), (14., 11.), (11., 11.)]);
                            line(&[(2., 5.), (11., 5.), (11., 14.), (2., 14.), (2., 5.)]);
                        }
                        Control::Close => {
                            line(&[(4., 4.), (12., 12.)]);
                            line(&[(12., 4.), (4., 12.)]);
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                },
            )
            .size(px(16.)),
        )
}

fn resize_areas(viewport: gpui::Size<Pixels>, tiling: Tiling) -> impl IntoElement {
    let mut areas = div().w(viewport.width).h(viewport.height);
    for (ix, (edge, bounds)) in resize_regions(viewport, tiling).into_iter().enumerate() {
        let cursor = match edge {
            ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
            ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
            ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
            ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
        };
        areas = areas.child(
            div()
                .id(("window-resize", ix))
                .absolute()
                .left(bounds.origin.x)
                .top(bounds.origin.y)
                .w(bounds.size.width)
                .h(bounds.size.height)
                .cursor(cursor)
                .on_mouse_move(|_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    window.start_window_resize(edge);
                }),
        );
    }
    areas
}

fn resize_regions(
    viewport: gpui::Size<Pixels>,
    tiling: Tiling,
) -> Vec<(ResizeEdge, Bounds<Pixels>)> {
    let w = viewport.width;
    let h = viewport.height;
    let edge = px(4.);
    let corner = px(8.);
    let regions = [
        (
            ResizeEdge::Top,
            tiling.top,
            corner,
            px(0.),
            w - corner * 2.,
            edge,
        ),
        (
            ResizeEdge::Bottom,
            tiling.bottom,
            corner,
            h - edge,
            w - corner * 2.,
            edge,
        ),
        (
            ResizeEdge::Left,
            tiling.left,
            px(0.),
            corner,
            edge,
            h - corner * 2.,
        ),
        (
            ResizeEdge::Right,
            tiling.right,
            w - edge,
            corner,
            edge,
            h - corner * 2.,
        ),
        (
            ResizeEdge::TopLeft,
            tiling.top || tiling.left,
            px(0.),
            px(0.),
            corner,
            corner,
        ),
        (
            ResizeEdge::TopRight,
            tiling.top || tiling.right,
            w - corner,
            px(0.),
            corner,
            corner,
        ),
        (
            ResizeEdge::BottomLeft,
            tiling.bottom || tiling.left,
            px(0.),
            h - corner,
            corner,
            corner,
        ),
        (
            ResizeEdge::BottomRight,
            tiling.bottom || tiling.right,
            w - corner,
            h - corner,
            corner,
            corner,
        ),
    ];
    regions
        .into_iter()
        .filter(|(_, tiled, _, _, _, _)| !tiled)
        .map(|(edge, _, x, y, w, h)| (edge, Bounds::new(point(x, y), size(w, h))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_regions_cover_each_edge_and_corner_without_covering_content() {
        let viewport = size(px(1000.), px(640.));
        let regions = resize_regions(viewport, Tiling::default());
        assert_eq!(regions.len(), 8);
        for (pos, expected) in [
            (point(px(1.), px(1.)), ResizeEdge::TopLeft),
            (point(px(500.), px(1.)), ResizeEdge::Top),
            (point(px(999.), px(1.)), ResizeEdge::TopRight),
            (point(px(1.), px(320.)), ResizeEdge::Left),
            (point(px(999.), px(320.)), ResizeEdge::Right),
            (point(px(1.), px(639.)), ResizeEdge::BottomLeft),
            (point(px(500.), px(639.)), ResizeEdge::Bottom),
            (point(px(999.), px(639.)), ResizeEdge::BottomRight),
        ] {
            let hits: Vec<_> = regions.iter().filter(|(_, b)| b.contains(&pos)).collect();
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].0, expected);
        }
        assert!(
            regions
                .iter()
                .all(|(_, b)| !b.contains(&point(px(500.), px(320.))))
        );
    }

    #[test]
    fn tiled_edges_and_their_corners_cannot_be_resized() {
        let viewport = size(px(1000.), px(640.));
        let regions = resize_regions(
            viewport,
            Tiling {
                top: true,
                left: true,
                ..Tiling::default()
            },
        );
        let edges: Vec<_> = regions.iter().map(|(edge, _)| *edge).collect();
        assert_eq!(
            edges,
            [
                ResizeEdge::Bottom,
                ResizeEdge::Right,
                ResizeEdge::BottomRight
            ]
        );
        assert!(
            resize_regions(
                viewport,
                Tiling {
                    top: true,
                    bottom: true,
                    left: true,
                    right: true
                }
            )
            .is_empty()
        );
    }
}
