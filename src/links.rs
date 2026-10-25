//! Web links the app opens: the project's own pages, and documentation for Colima and the
//! tools it builds on. Settings (About and Help) shows them as tiles, and the Help menu lists
//! them.

/// One link tile in Settings, and one item in the Help menu.
pub struct Link {
    /// The tile's caption.
    pub title: &'static str,
    /// The Help menu item.
    pub menu: &'static str,
    /// What it is; the tile's tooltip.
    pub summary: &'static str,
    /// An SVG in the app's assets, drawn in one color like the other icons.
    pub logo: &'static str,
    pub url: &'static str,
}

/// The Colima GPUI project: source, feedback, contributing, changes (Settings → About).
pub const PROJECT: &[Link] = &[
    Link {
        title: "Source",
        menu: "Colima GPUI on GitHub",
        summary: "The source code on GitHub.",
        logo: "logos/github.svg",
        url: "https://github.com/sina96/colima-gpui",
    },
    Link {
        title: "Report an issue",
        menu: "Report an Issue or Ask a Question…",
        summary: "Report a bug, request a feature or ask a question.",
        logo: "icons/bug.svg",
        url: "https://github.com/sina96/colima-gpui/issues/new/choose",
    },
    Link {
        title: "Contributing",
        menu: "Contributing",
        summary: "How to build Colima GPUI and open a pull request.",
        logo: "icons/git-pull-request.svg",
        url: "https://github.com/sina96/colima-gpui/blob/main/CONTRIBUTING.md",
    },
    Link {
        title: "Changelog",
        menu: "What's New",
        summary: "What changed in each version.",
        logo: "icons/file-text.svg",
        url: "https://github.com/sina96/colima-gpui/blob/main/CHANGELOG.md",
    },
];

/// Documentation for Colima and what it builds on (Settings → Help), from the VM up.
pub const DOCS: &[Link] = &[
    Link {
        title: "Colima",
        menu: "Colima Documentation",
        summary: "Container runtimes on macOS with minimal setup. This app drives its CLI.",
        logo: "logos/colima.svg",
        url: "https://colima.run/docs/",
    },
    Link {
        title: "Lima",
        menu: "Lima Documentation",
        summary: "Linux virtual machines on macOS. Colima runs each profile's VM with Lima.",
        logo: "logos/lima.svg",
        url: "https://lima-vm.io/docs/",
    },
    Link {
        title: "Docker",
        menu: "Docker Documentation",
        summary: "Images, containers, volumes and the docker CLI (the docker runtime).",
        logo: "logos/docker.svg",
        url: "https://docs.docker.com/",
    },
    Link {
        title: "Docker Compose",
        menu: "Docker Compose Documentation",
        summary: "Multi-container apps defined in a compose.yaml file.",
        logo: "logos/docker.svg",
        url: "https://docs.docker.com/compose/",
    },
    Link {
        title: "containerd",
        menu: "nerdctl (containerd) on GitHub",
        summary: "The containerd runtime and nerdctl, its Docker-compatible CLI.",
        logo: "logos/containerd.svg",
        url: "https://github.com/containerd/nerdctl",
    },
    Link {
        title: "Kubernetes",
        menu: "Kubernetes Documentation",
        summary: "Pods, deployments, services and the rest of Kubernetes.",
        logo: "logos/kubernetes.svg",
        url: "https://kubernetes.io/docs/home/",
    },
    Link {
        title: "k3s",
        menu: "k3s Documentation",
        summary: "The lightweight Kubernetes that Colima runs when Kubernetes is on.",
        logo: "logos/k3s.svg",
        url: "https://docs.k3s.io/",
    },
    Link {
        title: "kubectl",
        menu: "kubectl Quick Reference",
        summary: "The kubectl commands you'll use most, on one page.",
        logo: "icons/terminal.svg",
        url: "https://kubernetes.io/docs/reference/kubectl/quick-reference/",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AssetSource;

    #[test]
    fn every_link_is_https_with_a_bundled_logo() {
        for link in PROJECT.iter().chain(DOCS) {
            assert!(link.url.starts_with("https://"), "{}", link.title);
            assert!(link.summary.ends_with('.'), "{}", link.title);
            assert!(
                crate::assets::Assets.load(link.logo).unwrap().is_some(),
                "{} has no asset {}",
                link.title,
                link.logo
            );
        }
    }
}
