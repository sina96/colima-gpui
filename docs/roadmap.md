# Roadmap

What's planned for Colima GPUI, roughly in order. Plans change; open a [feature request](https://github.com/sina96/colima-gpui/issues/new?template=feature_request.yml) to suggest something, or comment on an existing one.

## Next

- **Doctor:** checks for common problems (a full data disk, the wrong Docker context, a missing `colima` or `docker` binary), with one-click fixes where they're safe.
- **Container shell:** open a terminal inside a running container.
- **Compose projects:** group containers by Docker Compose project.
- **Log search:** find text in container logs, with optional timestamps.
- **Launch at login** (off by default).
- **Profile defaults from Colima's template:** the new-profile form starts from Colima's built-in defaults and sends every value, so your own defaults in `colima template` aren't used yet. Planned: fill the form from the template.

## Later

- **Kubernetes:** start, stop and reset already work from the profile page. Planned:
  - A Kubernetes tab with nodes, namespaces and pods, their status and logs.
  - Show which kubectl context the profile uses.
- **containerd runtime:** containerd profiles can be created, started, stopped and edited, and their disk usage is shown. Planned: containers, images and volumes through `nerdctl`, like the Docker tabs.
- **More Colima options in the form:** today they're set through **Open colima.yaml**. Candidates: DNS host entries, SSH port, binfmt, the gateway address and the k3s listen port.
- **AI models:** `colima model run` and `serve` for profiles on the krunkit VM type.
- **MCP:** setup instructions for a Colima MCP server in the AI tab, once one exists.

## Not planned

- **Windows:** Colima runs only on macOS and Linux.
