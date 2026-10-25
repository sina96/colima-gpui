//! Placeholder for an "Update vX.Y.Z" badge in the status bar, shown once automatic
//! updates exist. Renders nothing for now.

use gpui::{Context, Window, div, prelude::*};

pub struct UpdateBadge;

impl UpdateBadge {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for UpdateBadge {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
