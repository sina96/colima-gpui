//! Data table with aligned columns: one `Column` spec drives both the header and every
//! row, so they can't drift apart. Rows are virtualized (`uniform_list`) and stretch to the
//! full table width.
//!
//! ```ignore
//! let images = Rc::new(self.images.clone());
//! let view = cx.entity().downgrade();
//! Table::new(
//!     "images",
//!     vec![
//!         Column::fixed("", px(16.)),                        // status dot
//!         Column::flex("Repository:tag", 1.).min(px(160.)),
//!         Column::fixed("Size", px(110.)).right(),
//!         Column::fixed("Created", px(140.)),
//!         Column::actions(),                                 // ⋯ menu; clicks don't open the row
//!     ],
//!     images.len(),
//!     move |ix, _, _| {
//!         let image = &images[ix];
//!         vec![
//!             StatusDot::for_image(("status", ix), …).into_any_element(),
//!             TextCell::new(("name", ix), image.name()).into_any_element(),
//!             size_cell(image.size_label(), image.bytes(), max).into_any_element(),
//!             TextCell::new(("created", ix), …).muted().into_any_element(),
//!             MenuButton::new(("menu", ix)).item(…).into_any_element(),
//!         ]
//!     },
//! )
//! .on_row_click(move |ix, window, cx| { view.update(cx, |this, cx| this.open(ix, window, cx)).ok(); })
//! ```

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, MouseButton, Pixels, SharedString, Window, div, prelude::*, px,
    uniform_list,
};

use crate::theme::{ActiveTheme, MONO_FONT};
use crate::ui::chart::SizeBar;
use crate::ui::icon_button::InfoTip;
use crate::ui::tooltip::tooltip;

const ROW_HEIGHT: f32 = 36.;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Width {
    Fixed(Pixels),
    /// Share of the remaining width, like CSS `flex-grow`.
    Flex(f32),
}

#[derive(Clone)]
pub struct Column {
    header: SharedString,
    width: Width,
    min: Pixels,
    right: bool,
    info: Option<SharedString>,
    /// Clicks inside don't reach the row (for menus and buttons).
    actions: bool,
}

impl Column {
    pub fn fixed(header: impl Into<SharedString>, width: Pixels) -> Self {
        Self {
            header: header.into(),
            width: Width::Fixed(width),
            min: px(0.),
            right: false,
            info: None,
            actions: false,
        }
    }

    pub fn flex(header: impl Into<SharedString>, weight: f32) -> Self {
        Self {
            width: Width::Flex(weight),
            ..Self::fixed(header, px(0.))
        }
    }

    /// Trailing column for a `MenuButton` or `IconButton`s.
    pub fn actions() -> Self {
        Self {
            actions: true,
            right: true,
            ..Self::fixed("", px(32.))
        }
    }

    /// Minimum width for a flex column before the table starts clipping.
    pub fn min(mut self, min: Pixels) -> Self {
        self.min = min;
        self
    }

    /// Right-align header and cells (numbers, sizes).
    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }

    /// ⓘ next to the header.
    pub fn info(mut self, text: impl Into<SharedString>) -> Self {
        self.info = Some(text.into());
        self
    }

    fn cell(&self) -> gpui::Div {
        let cell = div().flex().items_center().min_w_0().overflow_hidden();
        let cell = match self.width {
            Width::Fixed(width) => cell.flex_none().w(width),
            Width::Flex(weight) => cell.flex_grow(weight).flex_shrink_1().flex_basis(px(0.)),
        };
        cell.min_w(self.min).when(self.right, |el| el.justify_end())
    }
}

type RowBuilder = Rc<dyn Fn(usize, &mut Window, &mut App) -> Vec<AnyElement>>;
type RowClick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Table {
    id: ElementId,
    columns: Rc<Vec<Column>>,
    row_count: usize,
    rows: RowBuilder,
    on_row_click: Option<RowClick>,
    selected: Option<usize>,
    empty: Option<AnyElement>,
    show_header: bool,
}

impl Table {
    /// `rows(ix, …)` returns one element per column, in column order.
    pub fn new(
        id: impl Into<ElementId>,
        columns: Vec<Column>,
        row_count: usize,
        rows: impl Fn(usize, &mut Window, &mut App) -> Vec<AnyElement> + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            columns: Rc::new(columns),
            row_count,
            rows: Rc::new(rows),
            on_row_click: None,
            selected: None,
            empty: None,
            show_header: true,
        }
    }

    pub fn on_row_click(
        mut self,
        handler: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_row_click = Some(Rc::new(handler));
        self
    }

    pub fn selected(mut self, row: Option<usize>) -> Self {
        self.selected = row;
        self
    }

    /// Hides the header row, for a table continuing one above it (e.g. split around an
    /// expanded row).
    pub fn without_header(mut self) -> Self {
        self.show_header = false;
        self
    }

    /// Shown under the header when there are no rows.
    pub fn empty(mut self, empty: impl IntoElement) -> Self {
        self.empty = Some(empty.into_any_element());
        self
    }
}

impl RenderOnce for Table {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let columns = self.columns.clone();

        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .gap_3()
            .w_full()
            .h(px(30.))
            .px_3()
            .bg(theme.surface_background)
            .border_b_1()
            .border_color(theme.border_variant)
            .text_xs()
            .text_color(theme.text_muted)
            .children(columns.iter().enumerate().map(|(ix, column)| {
                column
                    .cell()
                    .gap_1()
                    .child(div().truncate().child(column.header.clone()))
                    .when_some(column.info.clone(), |el, info| {
                        el.child(InfoTip::new(("column-info", ix), info))
                    })
            }));

        let body = if self.row_count == 0 {
            div().flex_1().children(self.empty).into_any_element()
        } else {
            let rows = self.rows.clone();
            let on_click = self.on_row_click.clone();
            let selected = self.selected;
            let columns = self.columns.clone();
            uniform_list(self.id, self.row_count, move |range, window, cx| {
                let theme = cx.theme().clone();
                range
                    .map(|ix| {
                        let cells = rows(ix, window, cx);
                        let on_click = on_click.clone();
                        div()
                            .id(ix)
                            .flex()
                            .items_center()
                            .gap_3()
                            .w_full()
                            .h(px(ROW_HEIGHT))
                            .px_3()
                            .text_sm()
                            .border_b_1()
                            .border_color(theme.border_variant)
                            .when(selected == Some(ix), |el| el.bg(theme.element_selected))
                            .when(selected != Some(ix), |el| {
                                el.hover(|s| s.bg(theme.ghost_element_hover))
                            })
                            .children(columns.iter().zip(cells).map(|(column, cell)| {
                                column.cell().child(cell).when(column.actions, |el| {
                                    el.on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                })
                            }))
                            .when_some(on_click, |el, on_click| {
                                el.cursor_pointer()
                                    .on_click(move |_, window, cx| on_click(ix, window, cx))
                            })
                    })
                    .collect::<Vec<_>>()
            })
            .flex_1()
            .w_full()
            .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .when(self.show_header, |el| el.child(header))
            .child(body)
    }
}

/// Text that truncates with "…" and shows the full value on hover.
#[derive(IntoElement)]
pub struct TextCell {
    id: ElementId,
    text: SharedString,
    mono: bool,
    muted: bool,
}

impl TextCell {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            mono: false,
            muted: false,
        }
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn muted(mut self) -> Self {
        self.muted = true;
        self
    }
}

impl RenderOnce for TextCell {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .id(self.id)
            .min_w_0()
            .truncate()
            .when(self.mono, |el| el.font_family(MONO_FONT).text_xs())
            .when(self.muted, |el| el.text_color(theme.text_muted))
            .tooltip(tooltip(self.text.clone()))
            .child(self.text)
    }
}

/// Right-aligned size with a `SizeBar` underneath, for size columns.
pub fn size_cell(label: impl Into<SharedString>, value: f64, max: f64) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_end()
        .gap_1()
        .w_full()
        .child(div().text_xs().child(label.into()))
        .child(SizeBar::new(value, max))
}
