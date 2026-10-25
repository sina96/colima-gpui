//! Lucide icons from `assets/icons/`, tinted with a theme colour.

use gpui::{App, Hsla, Pixels, Window, prelude::*, px, svg};

use crate::theme::ActiveTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    ArrowLeft,
    Box,
    Bug,
    Check,
    ChevronDown,
    ChevronRight,
    Copy,
    Cpu,
    Database,
    Ellipsis,
    ExternalLink,
    FileText,
    GitPullRequest,
    HardDrive,
    Info,
    Layers,
    MemoryStick,
    Network,
    Play,
    RotateCw,
    Search,
    Square,
    Trash,
    ArrowUpRight,
    Clock,
    Download,
    Folder,
    Pause,
    Sparkles,
    Tag,
    Terminal,
    X,
}

impl IconName {
    pub const ALL: [IconName; 32] = [
        IconName::ArrowLeft,
        IconName::Box,
        IconName::Bug,
        IconName::Check,
        IconName::ChevronDown,
        IconName::ChevronRight,
        IconName::Copy,
        IconName::Cpu,
        IconName::Database,
        IconName::Ellipsis,
        IconName::ExternalLink,
        IconName::FileText,
        IconName::GitPullRequest,
        IconName::HardDrive,
        IconName::Info,
        IconName::Layers,
        IconName::MemoryStick,
        IconName::Network,
        IconName::Play,
        IconName::RotateCw,
        IconName::Search,
        IconName::Square,
        IconName::Trash,
        IconName::ArrowUpRight,
        IconName::Clock,
        IconName::Download,
        IconName::Folder,
        IconName::Pause,
        IconName::Sparkles,
        IconName::Tag,
        IconName::Terminal,
        IconName::X,
    ];

    pub fn path(self) -> &'static str {
        match self {
            IconName::ArrowLeft => "icons/arrow-left.svg",
            IconName::Box => "icons/box.svg",
            IconName::Check => "icons/check.svg",
            IconName::ChevronDown => "icons/chevron-down.svg",
            IconName::ChevronRight => "icons/chevron-right.svg",
            IconName::Copy => "icons/copy.svg",
            IconName::Cpu => "icons/cpu.svg",
            IconName::Database => "icons/database.svg",
            IconName::Ellipsis => "icons/ellipsis.svg",
            IconName::ExternalLink => "icons/external-link.svg",
            IconName::FileText => "icons/file-text.svg",
            IconName::HardDrive => "icons/hard-drive.svg",
            IconName::Info => "icons/info.svg",
            IconName::Layers => "icons/layers.svg",
            IconName::MemoryStick => "icons/memory-stick.svg",
            IconName::Network => "icons/network.svg",
            IconName::Play => "icons/play.svg",
            IconName::RotateCw => "icons/rotate-cw.svg",
            IconName::Search => "icons/search.svg",
            IconName::Square => "icons/square.svg",
            IconName::Trash => "icons/trash.svg",
            IconName::ArrowUpRight => "icons/arrow-up-right.svg",
            IconName::Clock => "icons/clock.svg",
            IconName::Download => "icons/download.svg",
            IconName::Folder => "icons/folder.svg",
            IconName::Pause => "icons/pause.svg",
            IconName::Sparkles => "icons/sparkles.svg",
            IconName::Tag => "icons/tag.svg",
            IconName::Terminal => "icons/terminal.svg",
            IconName::Bug => "icons/bug.svg",
            IconName::GitPullRequest => "icons/git-pull-request.svg",
            IconName::X => "icons/x.svg",
        }
    }
}

#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    size: Pixels,
    color: Option<Hsla>,
    group_hover: Option<(&'static str, Hsla)>,
}

impl Icon {
    /// 16 px, `text_muted` by default.
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            size: px(16.),
            color: None,
            group_hover: None,
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Recolour while the ancestor with `.group(group)` is hovered.
    pub fn group_hover_color(mut self, group: &'static str, color: Hsla) -> Self {
        self.group_hover = Some((group, color));
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        svg()
            .path(self.name.path())
            .flex_none()
            .size(self.size)
            .text_color(self.color.unwrap_or(cx.theme().text_muted))
            .when_some(self.group_hover, |el, (group, color)| {
                el.group_hover(group, move |s| s.text_color(color))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::IconName;
    use crate::assets::ICONS;

    #[test]
    fn every_icon_is_bundled() {
        for icon in IconName::ALL {
            assert!(
                ICONS.iter().any(|(path, _)| *path == icon.path()),
                "{icon:?} is not in assets::ICONS"
            );
        }
    }
}
