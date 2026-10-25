# Roadmap

What's planned for Colima GPUI, roughly in order. Plans change; open a [feature request](https://github.com/sina96/colima-gpui/issues/new?template=feature_request.yml) to suggest something, or comment on an existing one.

## Next

- **Releases and updates:**
  - Prebuilt downloads on GitHub Releases.
  - In-app update notifications, with an "Update" badge and one-click install.
- **Doctor:** checks for common problems (a full data disk, the wrong Docker context, a missing `colima` or `docker` binary), with one-click fixes where they're safe.
- **Container shell:** open a terminal inside a running container.
- **Compose projects:** group containers by Docker Compose project.
- **Log search:** find text in container logs, with optional timestamps.
- **Launch at login** (off by default).
- **App icon and branding.**

## Later

- **Kubernetes:** profiles with Kubernetes enabled already work: their Docker tabs, start/stop and editing behave as usual. Planned:
  - Group or hide the Kubernetes system containers (`k8s_…`) in the Containers tab.
  - A Kubernetes tab with nodes, namespaces and pods, their status and logs.
  - Show which kubectl context the profile uses.
- **containerd runtime:** containerd profiles can be created, started, stopped and edited, and their disk usage is shown. Planned: containers, images and volumes through `nerdctl`, like the Docker tabs.
- **Linux:** a Linux build (AppImage) with a tray icon.
- **MCP:** setup instructions for a Colima MCP server in the AI tab, once one exists.

## Not planned

- **Windows:** Colima runs only on macOS and Linux.
