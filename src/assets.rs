//! Bundled assets served to GPUI (`svg().path("icons/…")`). Everything is compiled in with
//! `include_bytes!`, so nothing is read from disk at runtime. Icons: see `assets/icons/README.md`.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub struct Assets;

pub(crate) const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/arrow-left.svg",
        include_bytes!("../assets/icons/arrow-left.svg"),
    ),
    ("icons/box.svg", include_bytes!("../assets/icons/box.svg")),
    (
        "icons/check.svg",
        include_bytes!("../assets/icons/check.svg"),
    ),
    (
        "icons/chevron-down.svg",
        include_bytes!("../assets/icons/chevron-down.svg"),
    ),
    ("icons/copy.svg", include_bytes!("../assets/icons/copy.svg")),
    ("icons/cpu.svg", include_bytes!("../assets/icons/cpu.svg")),
    (
        "icons/database.svg",
        include_bytes!("../assets/icons/database.svg"),
    ),
    (
        "icons/ellipsis.svg",
        include_bytes!("../assets/icons/ellipsis.svg"),
    ),
    (
        "icons/external-link.svg",
        include_bytes!("../assets/icons/external-link.svg"),
    ),
    (
        "icons/file-text.svg",
        include_bytes!("../assets/icons/file-text.svg"),
    ),
    (
        "icons/hard-drive.svg",
        include_bytes!("../assets/icons/hard-drive.svg"),
    ),
    ("icons/info.svg", include_bytes!("../assets/icons/info.svg")),
    (
        "icons/layers.svg",
        include_bytes!("../assets/icons/layers.svg"),
    ),
    (
        "icons/memory-stick.svg",
        include_bytes!("../assets/icons/memory-stick.svg"),
    ),
    (
        "icons/network.svg",
        include_bytes!("../assets/icons/network.svg"),
    ),
    ("icons/play.svg", include_bytes!("../assets/icons/play.svg")),
    (
        "icons/rotate-cw.svg",
        include_bytes!("../assets/icons/rotate-cw.svg"),
    ),
    (
        "icons/search.svg",
        include_bytes!("../assets/icons/search.svg"),
    ),
    (
        "icons/square.svg",
        include_bytes!("../assets/icons/square.svg"),
    ),
    (
        "icons/trash.svg",
        include_bytes!("../assets/icons/trash.svg"),
    ),
    ("icons/x.svg", include_bytes!("../assets/icons/x.svg")),
    (
        "icons/arrow-up-right.svg",
        include_bytes!("../assets/icons/arrow-up-right.svg"),
    ),
    (
        "icons/chevron-right.svg",
        include_bytes!("../assets/icons/chevron-right.svg"),
    ),
    (
        "icons/clock.svg",
        include_bytes!("../assets/icons/clock.svg"),
    ),
    (
        "icons/download.svg",
        include_bytes!("../assets/icons/download.svg"),
    ),
    (
        "icons/folder.svg",
        include_bytes!("../assets/icons/folder.svg"),
    ),
    (
        "icons/pause.svg",
        include_bytes!("../assets/icons/pause.svg"),
    ),
    (
        "icons/sparkles.svg",
        include_bytes!("../assets/icons/sparkles.svg"),
    ),
    ("icons/tag.svg", include_bytes!("../assets/icons/tag.svg")),
    ("icons/bug.svg", include_bytes!("../assets/icons/bug.svg")),
    (
        "icons/circle-alert.svg",
        include_bytes!("../assets/icons/circle-alert.svg"),
    ),
    (
        "icons/git-pull-request.svg",
        include_bytes!("../assets/icons/git-pull-request.svg"),
    ),
    (
        "icons/terminal.svg",
        include_bytes!("../assets/icons/terminal.svg"),
    ),
];

/// Logos of the tools Settings → Help links to; see `assets/logos/README.md`.
pub(crate) const LOGOS: &[(&str, &[u8])] = &[
    (
        "logos/colima.svg",
        include_bytes!("../assets/logos/colima.svg"),
    ),
    ("logos/lima.svg", include_bytes!("../assets/logos/lima.svg")),
    (
        "logos/docker.svg",
        include_bytes!("../assets/logos/docker.svg"),
    ),
    (
        "logos/containerd.svg",
        include_bytes!("../assets/logos/containerd.svg"),
    ),
    (
        "logos/kubernetes.svg",
        include_bytes!("../assets/logos/kubernetes.svg"),
    ),
    ("logos/k3s.svg", include_bytes!("../assets/logos/k3s.svg")),
    (
        "logos/github.svg",
        include_bytes!("../assets/logos/github.svg"),
    ),
];

fn all() -> impl Iterator<Item = &'static (&'static str, &'static [u8])> {
    ICONS.iter().chain(LOGOS)
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(all()
            .find(|(p, _)| *p == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(all()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| SharedString::from(*p))
            .collect())
    }
}
