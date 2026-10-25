//! Reusable widgets. Official GPUI ships none, so they live here.
//! `cargo run --example ui_gallery` shows every one of them.

pub mod badge;
pub mod button;
pub mod card;
pub mod chart;
pub mod detail_card;
pub mod detail_page;
pub mod dialog;
pub mod icon;
pub mod icon_button;
pub mod link;
pub mod menu;
pub mod select;
pub mod status;
pub mod stepper;
pub mod switch;
pub mod table;
pub mod text_input;
pub mod tooltip;

pub use button::{Button, ButtonStyle};
pub use chart::{Meter, Segment, SizeBar, StackedBar};
pub use detail_card::{CardGrid, DetailCard, Field};
pub use detail_page::DetailPage;
pub use icon::{Icon, IconName};
pub use icon_button::{IconButton, InfoTip};
pub use link::Link;
pub use menu::{MenuButton, MenuItem};
pub use status::StatusDot;
pub use switch::Switch;
pub use table::{Column, Table, TextCell, size_cell};

/// Registers key bindings for every widget that has them. Call once at startup.
pub fn init(cx: &mut gpui::App) {
    text_input::init(cx);
    detail_page::init(cx);
}
