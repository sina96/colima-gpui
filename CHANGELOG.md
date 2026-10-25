# Changelog

All notable changes to Colima GPUI are in this file.

The format follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/). Colima GPUI uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The first public version, 0.1.0, is being prepared.

### Added

- **Updates:** release builds check GitHub Releases, show release notes, and install verified updates with restart and rollback.

- **Profiles:**
  - Create, start, stop, restart, edit and delete Colima profiles, with live status and start-up logs.
  - The new-profile form covers CPU, memory, disk, runtime, VM type, architecture, mount type and Kubernetes.
- **Overview:** CPU, memory and disk usage per profile, with quick actions to SSH into the VM and to copy the `docker context` command.
- **Containers:**
  - List, start, stop, restart and remove containers.
  - A detail page in cards: ports, mounts, environment and resource use.
  - Live logs you can follow or pause, select, copy, and save to a file named after the container.
- **Images:** sizes, filters and sorting, and a detail page in tabs with the layers and history.
- **Volumes:** sizes and a detail page in cards.
- **Cross-links:** jump between linked containers, images and volumes.
- **Cleanup:**
  - Disk usage by category, and pruning of unused data.
  - Return the freed space from the VM to macOS, optionally after every prune.
- **AI tab:** download Colima's agent skill and copy a ready-made, profile-specific prompt for coding agents.
- **Menu bar icon** (optional): start, stop and restart each profile, open the app or its settings, and keep running in the background when the window closes.
- **Dock menu:** the same profile actions from the Dock icon.
- **Color profiles:** Colima (default) and Zed One, each in light and dark, following the system or set by hand.
- **Menus and keyboard shortcuts:** ⌘1–⌘6 switch tabs, ⌘R refreshes, ⌘, opens Settings.
- **App icon and menu bar glyph:** three stacked blocks forming a "C" (VM, runtime, containers), with Liquid Glass on macOS 26.
- **Settings:** poll interval, color profile, theme, menu bar and cleanup options.
- **Help:** Settings → Help shows tiles linking to the documentation for Colima, Lima, Docker, Docker Compose, containerd, Kubernetes, k3s and kubectl; the Help menu lists them too. Settings → About has tiles for the source code, issues, contributing and the changelog.
- **Kubernetes and containerd profiles:** profiles with Kubernetes on keep their Docker tabs, and the AI prompt names the kubectl context. containerd profiles show their status and disk usage.
- **Kubernetes containers:** the Containers tab hides the containers Kubernetes runs (`k8s_…`) and says how many are hidden; the **Kubernetes** button shows them.
- **Downloads:** each release has a `.dmg` and a `.zip` for Apple silicon and for Intel Macs, with SHA-256 checksums.
- **Delete with data:** the delete dialog can also remove the profile's container data (`colima delete --data`), which recreating a profile with another runtime needs.

[Unreleased]: https://github.com/sina96/colima-gpui/commits/main
