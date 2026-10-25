//! Cards for detail pages (container, image, volume).
//!
//! - [`CardGrid`] wraps cards into one or two columns from the space it has (no width
//!   query): each card wants ~320 px, so two fit from ~650 px. `.wide(card)` spans the row.
//! - [`DetailCard`] is one card: icon + title, an optional header action, then rows.
//! - [`Field`] is one label/value row. Values truncate; `.wrap()` lets long text (commands)
//!   wrap instead. `.mono()` for IDs and paths, `.copyable()` adds a copy button on hover,
//!   `.link(...)` turns the value into a [`Link`] to another detail page.
//!
//! ```ignore
//! CardGrid::new()
//!     .card(DetailCard::new(IconName::Box, "Summary")
//!         .child(Field::new("Status", "running"))
//!         .child(Field::new("Image", image).link("image-link", on_click)))
//!     .wide(DetailCard::new(IconName::FileText, "Environment").empty("No variables"))
//! ```

use gpui::{
    AnyElement, App, ClickEvent, ClipboardItem, SharedString, Window, div, prelude::*, px, relative,
};

use crate::theme::{ActiveTheme, MONO_FONT};
use crate::ui::icon::{Icon, IconName};
use crate::ui::icon_button::IconButton;
use crate::ui::link::Link;

/// Width a card asks for before the grid wraps it onto its own row.
const CARD_BASIS: f32 = 320.;
const LABEL_WIDTH: f32 = 96.;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement, Default)]
pub struct CardGrid {
    cards: Vec<(bool, AnyElement)>,
}

impl CardGrid {
    pub fn new() -> Self {
        Self::default()
    }

    /// A card that shares its row when there's room.
    pub fn card(mut self, card: impl IntoElement) -> Self {
        self.cards.push((false, card.into_any_element()));
        self
    }

    /// A card that always takes the full row, e.g. Environment or a table.
    pub fn wide(mut self, card: impl IntoElement) -> Self {
        self.cards.push((true, card.into_any_element()));
        self
    }
}

impl RenderOnce for CardGrid {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .w_full()
            .min_w_0()
            .children(self.cards.into_iter().map(|(wide, card)| {
                div()
                    .flex()
                    .flex_col()
                    .flex_grow(1.)
                    .flex_shrink(1.)
                    .min_w_0()
                    .map(|el| {
                        if wide {
                            el.flex_basis(relative(1.))
                        } else {
                            el.flex_basis(px(CARD_BASIS))
                        }
                    })
                    .child(card)
            }))
    }
}

#[derive(IntoElement)]
pub struct DetailCard {
    icon: IconName,
    title: SharedString,
    action: Option<AnyElement>,
    rows: Vec<AnyElement>,
    empty: Option<SharedString>,
}

impl DetailCard {
    pub fn new(icon: IconName, title: impl Into<SharedString>) -> Self {
        Self {
            icon,
            title: title.into(),
            action: None,
            rows: Vec::new(),
            empty: None,
        }
    }

    /// Small control on the right of the title, e.g. a Reveal toggle or an `IconButton`.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    /// Muted text shown when the card has no rows ("No mounts").
    pub fn empty(mut self, text: impl Into<SharedString>) -> Self {
        self.empty = Some(text.into());
        self
    }
}

impl ParentElement for DetailCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.rows.extend(elements);
    }
}

impl RenderOnce for DetailCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let empty = self.rows.is_empty();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap_1()
            .min_w_0()
            .p_2()
            .rounded_sm()
            .border_1()
            .border_color(theme.border_variant)
            .bg(theme.surface_background)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .h(px(22.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .min_w_0()
                            .child(Icon::new(self.icon).size(px(14.)))
                            .child(div().truncate().child(self.title)),
                    )
                    .children(self.action),
            )
            .children(self.rows)
            .when(empty, |el| {
                el.children(
                    self.empty
                        .map(|text| div().text_xs().text_color(theme.text_muted).child(text)),
                )
            })
    }
}

#[derive(IntoElement)]
pub struct Field {
    label: SharedString,
    value: SharedString,
    mono: bool,
    wrap: bool,
    muted: bool,
    copyable: bool,
    link: Option<(SharedString, ClickHandler)>,
}

impl Field {
    pub fn new(label: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            mono: false,
            wrap: false,
            muted: false,
            copyable: false,
            link: None,
        }
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// Wrap long values instead of truncating them.
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    /// For placeholder values such as "—" or "none".
    pub fn muted(mut self) -> Self {
        self.muted = true;
        self
    }

    /// Copy button on hover; copies the full value.
    pub fn copyable(mut self) -> Self {
        self.copyable = true;
        self
    }

    /// Renders the value as a [`Link`]. `id` must be unique on the page.
    pub fn link(
        mut self,
        id: impl Into<SharedString>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.link = Some((id.into(), Box::new(on_click)));
        self
    }
}

impl RenderOnce for Field {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let group = SharedString::from(format!("field-{}", self.label));
        let value = match self.link {
            Some((id, on_click)) => {
                let link = Link::new(id, self.value.clone()).on_click(on_click);
                if self.mono {
                    link.mono().into_any_element()
                } else {
                    link.into_any_element()
                }
            }
            None => div()
                .min_w_0()
                .when(!self.wrap, |el| el.truncate())
                .when(self.mono, |el| el.font_family(MONO_FONT).text_xs())
                .when(self.muted, |el| el.text_color(theme.text_muted))
                .child(self.value.clone())
                .into_any_element(),
        };
        div()
            .group(group.clone())
            .flex()
            .items_start()
            .gap_2()
            .min_w_0()
            .min_h(px(22.))
            .child(
                div()
                    .flex_none()
                    .w(px(LABEL_WIDTH))
                    .pt(px(3.))
                    .text_xs()
                    .text_color(theme.text_muted)
                    .truncate()
                    .child(self.label.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h(px(22.))
                    .items_center()
                    .child(value),
            )
            .when(self.copyable, |el| {
                let text = self.value.clone();
                el.child(
                    div()
                        .flex_none()
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(1.))
                        .child(
                            IconButton::new(
                                SharedString::from(format!("copy-{}", self.label)),
                                IconName::Copy,
                            )
                            .tooltip(format!("Copy {}", self.label.to_lowercase()))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()))
                            }),
                        ),
                )
            })
    }
}
