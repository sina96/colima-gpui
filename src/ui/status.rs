//! Docker Desktop–style status dots: filled when active, hollow when idle, dashed when
//! in transition. The state's name is the tooltip, so tables don't need a text column.
//!
//! Use the mapping constructors (`for_container`, `for_image`, `for_volume`) so every
//! table shows the same states the same way.

use gpui::{App, ElementId, SharedString, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;
use crate::ui::tooltip::tooltip;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotShape {
    Filled,
    Hollow,
    Dashed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotTone {
    Success,
    Warning,
    Error,
    Muted,
}

/// What a dot shows, independent of rendering, so the mappings are unit-testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotState {
    pub shape: DotShape,
    pub tone: DotTone,
    pub label: SharedString,
    /// Drawn at reduced opacity (dangling images).
    pub dimmed: bool,
}

impl DotState {
    fn new(shape: DotShape, tone: DotTone, label: impl Into<SharedString>) -> Self {
        Self {
            shape,
            tone,
            label: label.into(),
            dimmed: false,
        }
    }

    /// Container from `docker ps` fields: `state` (`running`, `exited`, …) and the human
    /// `status` text, which carries the exit code (`Exited (1) 2 hours ago`).
    pub fn container(state: &str, status: &str) -> Self {
        match state {
            "running" => Self::new(DotShape::Filled, DotTone::Success, "Running"),
            "paused" => Self::new(DotShape::Filled, DotTone::Warning, "Paused"),
            "exited" => match exit_code(status) {
                Some(0) | None => Self::new(DotShape::Hollow, DotTone::Muted, "Exited"),
                Some(code) => Self::new(
                    DotShape::Hollow,
                    DotTone::Error,
                    format!("Exited with code {code}"),
                ),
            },
            "dead" => Self::new(DotShape::Dashed, DotTone::Error, "Dead"),
            "restarting" => Self::new(DotShape::Dashed, DotTone::Warning, "Restarting"),
            "created" => Self::new(DotShape::Dashed, DotTone::Muted, "Created"),
            "removing" => Self::new(DotShape::Dashed, DotTone::Muted, "Removing"),
            other => Self::new(DotShape::Hollow, DotTone::Muted, capitalize(other)),
        }
    }

    pub fn image(in_use: bool, dangling: bool) -> Self {
        match (in_use, dangling) {
            (true, _) => Self::new(DotShape::Filled, DotTone::Success, "In use"),
            (false, true) => Self {
                dimmed: true,
                ..Self::new(
                    DotShape::Hollow,
                    DotTone::Muted,
                    "Dangling (untagged, unused)",
                )
            },
            (false, false) => Self::new(DotShape::Hollow, DotTone::Muted, "Unused"),
        }
    }

    /// Colima profile from `colima list` (`Running`, `Stopped`, `Broken`); `busy` is the
    /// in-progress label of an action the app is running on it, e.g. "Starting…".
    pub fn profile(status: &str, busy: Option<&str>) -> Self {
        if let Some(label) = busy {
            return Self::new(DotShape::Dashed, DotTone::Warning, label.to_owned());
        }
        match status {
            "Running" => Self::new(DotShape::Filled, DotTone::Success, "Running"),
            "Broken" => Self::new(DotShape::Hollow, DotTone::Error, "Broken"),
            "" => Self::new(DotShape::Hollow, DotTone::Muted, "Unknown"),
            other => Self::new(DotShape::Hollow, DotTone::Muted, other.to_owned()),
        }
    }

    pub fn volume(in_use: bool) -> Self {
        if in_use {
            Self::new(DotShape::Filled, DotTone::Success, "In use")
        } else {
            Self::new(DotShape::Hollow, DotTone::Muted, "Unused")
        }
    }
}

fn exit_code(status: &str) -> Option<i32> {
    let rest = status.strip_prefix("Exited (")?;
    rest.split(')').next()?.trim().parse().ok()
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_else(|| "Unknown".into())
}

#[derive(IntoElement)]
pub struct StatusDot {
    id: ElementId,
    state: DotState,
}

impl StatusDot {
    /// `id` must be unique among siblings (it anchors the tooltip), e.g. `("status", row_ix)`.
    pub fn new(id: impl Into<ElementId>, state: DotState) -> Self {
        Self {
            id: id.into(),
            state,
        }
    }

    pub fn for_container(id: impl Into<ElementId>, state: &str, status: &str) -> Self {
        Self::new(id, DotState::container(state, status))
    }

    pub fn for_image(id: impl Into<ElementId>, in_use: bool, dangling: bool) -> Self {
        Self::new(id, DotState::image(in_use, dangling))
    }

    pub fn for_volume(id: impl Into<ElementId>, in_use: bool) -> Self {
        Self::new(id, DotState::volume(in_use))
    }
}

impl RenderOnce for StatusDot {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let color = match self.state.tone {
            DotTone::Success => theme.success,
            DotTone::Warning => theme.warning,
            DotTone::Error => theme.error,
            DotTone::Muted => theme.text_muted,
        };
        let dot = div().size(px(9.)).rounded_full();
        let dot = match self.state.shape {
            DotShape::Filled => dot.bg(color),
            DotShape::Hollow => dot.border_1().border_color(color),
            DotShape::Dashed => dot.border_1().border_dashed().border_color(color),
        };
        // A 16 px hit area so the tooltip is easy to reach.
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(16.))
            .when(self.state.dimmed, |el| el.opacity(0.5))
            .child(dot)
            .tooltip(tooltip(self.state.label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_states() {
        let running = DotState::container("running", "Up 2 hours");
        assert_eq!(
            (running.shape, running.tone),
            (DotShape::Filled, DotTone::Success)
        );
        let clean = DotState::container("exited", "Exited (0) 3 minutes ago");
        assert_eq!(
            (clean.shape, clean.tone),
            (DotShape::Hollow, DotTone::Muted)
        );
        let failed = DotState::container("exited", "Exited (137) 1 hour ago");
        assert_eq!(
            (failed.shape, failed.tone),
            (DotShape::Hollow, DotTone::Error)
        );
        assert_eq!(failed.label, "Exited with code 137");
        let created = DotState::container("created", "Created");
        assert_eq!(created.shape, DotShape::Dashed);
        assert_eq!(DotState::container("weird", "").label, "Weird");
    }

    #[test]
    fn profile_states() {
        assert_eq!(DotState::profile("Running", None).shape, DotShape::Filled);
        assert_eq!(DotState::profile("Stopped", None).shape, DotShape::Hollow);
        assert_eq!(DotState::profile("Broken", None).tone, DotTone::Error);
        let busy = DotState::profile("Stopped", Some("Starting…"));
        assert_eq!(
            (busy.shape, busy.label.as_ref()),
            (DotShape::Dashed, "Starting…")
        );
    }

    #[test]
    fn image_and_volume_states() {
        assert_eq!(DotState::image(true, true).shape, DotShape::Filled);
        assert!(DotState::image(false, true).dimmed);
        assert!(!DotState::image(false, false).dimmed);
        assert_eq!(DotState::volume(false).shape, DotShape::Hollow);
    }
}
