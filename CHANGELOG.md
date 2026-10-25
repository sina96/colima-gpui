# Changelog

All notable changes to Colima GPUI are in this file.

The format follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/). Colima GPUI uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-10-26

Colima GPUI now runs on Linux too.

### Added

- **Linux:** Colima GPUI runs on Linux (x86_64 and arm64), packaged as an AppImage.
  - Its own title bar with minimise, maximise and close buttons where the desktop doesn't draw one.
  - A tray icon with the same menu as on macOS, and keep-running-in-the-tray (needs a StatusNotifierItem host; on GNOME, the AppIndicator extension).
  - "SSH into VM" opens your terminal (`$TERMINAL`, GNOME Terminal, Konsole, kitty, Alacritty, WezTerm, foot, xterm and more).
  - In-app updates replace the AppImage, with the same checks and rollback as on macOS.
- **Debug log:** start the app with `COLIMA_GPUI_LOG=1` (or `debug`, `trace`) to print what it and GPUI are doing, for bug reports.
- **Release checks:** each release is read back after publishing and checked the way an installed app reads it.

### Changed

- The "update available" panel in Settings → About shows just the new version with **View on GitHub** and **Install and restart**; the release notes are on GitHub.

## [0.1.1] - 2026-10-25

### Changed

- Settings → About shows "Up to date" (or why the check failed) only after you click **Check for updates** or use **Check for Updates…**, and hides it again when you switch sections or close Settings.
- A profile without the Docker runtime shows a short "No Docker tabs" note in the tab row, with the reason on hover, instead of a long sentence.

### Fixed

- The delete dialog's "Also delete container data" text wraps inside the dialog instead of running past its edge, and the dialog's message is shorter.

## [0.1.0] - 2026-10-25

The first public version: a native macOS app for Colima, for Apple silicon and Intel Macs.

### Added

- **Profiles:**
  - Create, start, stop, restart, edit and delete Colima profiles, with live status and start-up logs.
  - The new-profile form covers CPU, memory, disk, runtime, VM type, architecture, mount type and Kubernetes.
  - "Start when Colima GPUI opens": the app starts the profile at launch if it's stopped.
  - Deleting can also remove the profile's container data (`colima delete --data`), which recreating a profile with another runtime needs.
- **Overview:** CPU, memory and disk usage per profile, with quick actions to SSH into the VM and to copy the `docker context` command.
- **Containers:**
  - List, start, stop, restart and remove containers.
  - A detail page in cards: ports, mounts, environment and resource use.
  - Live logs you can follow or pause, select, copy, and save to a file named after the container.
  - Kubernetes' own containers (`k8s_…`) are hidden by default; the **Kubernetes** button shows them.
- **Images:** sizes, filters and sorting, and a detail page in tabs with the layers and history.
- **Volumes:** sizes and a detail page in cards.
- **Cross-links:** jump between linked containers, images and volumes.
- **Cleanup:**
  - Disk usage by category, and pruning of unused data.
  - Return the freed space from the VM to macOS, optionally after every prune.
- **Kubernetes and containerd profiles:** profiles with Kubernetes on keep their Docker tabs. containerd profiles show their status and disk usage.
- **AI tab:** download Colima's agent skill and copy a ready-made, profile-specific prompt for coding agents.
- **Menu bar and Dock:**
  - An optional menu bar icon: start, stop and restart each profile, open the app or its settings, and keep running in the background when the window closes.
  - The same profile actions from the Dock icon.
- **Updates:**
  - The app checks GitHub Releases at launch and every 12 hours, or when you click "Check for updates" in Settings → About.
  - A new version shows as a badge in the status bar.
  - Installing is one click, verified, with restart and rollback.
- **Look and feel:**
  - Two color profiles, Colima (default) and Zed One, each in light and dark, following the system or set by hand.
  - Menus and keyboard shortcuts: ⌘1–⌘6 switch tabs, ⌘R refreshes, ⌘, opens Settings.
  - The app icon and menu bar glyph: three stacked blocks forming a "C" (VM, runtime, containers), with Liquid Glass on macOS 26.
  - Errors are shown as one line; hover for the full error, with a button to copy it.
- **Settings and help:**
  - Settings for the poll interval, colors, theme, menu bar, updates and cleanup.
  - Settings → Help links to the documentation for Colima, Lima, Docker, Docker Compose, containerd, Kubernetes, k3s and kubectl.
- **Downloads:** a `.dmg` and a `.zip` for Apple silicon and for Intel Macs, with SHA-256 checksums.

[Unreleased]: https://github.com/sina96/colima-gpui/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/sina96/colima-gpui/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/sina96/colima-gpui/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/sina96/colima-gpui/releases/tag/v0.1.0
