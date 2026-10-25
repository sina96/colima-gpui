# Logos

Logos of the tools that Settings → Help links to. The app draws them in one color, like its other icons, only to link to each project's documentation. The names and logos are trademarks of their owners; Colima GPUI isn't affiliated with or endorsed by them.

| File | Source | License |
|---|---|---|
| `colima.svg` | `favicon.svg` from [colima.run](https://colima.run) ([abiosoft/colima](https://github.com/abiosoft/colima)) | MIT |
| `lima.svg` | `website/assets/icons/logo.svg` from [lima-vm/lima](https://github.com/lima-vm/lima); viewBox made square to center the wordmark | Apache-2.0 |
| `docker.svg`, `containerd.svg`, `kubernetes.svg`, `k3s.svg`, `github.svg` | [Simple Icons](https://simpleicons.org) 16.35.0 | CC0-1.0 |

To add a logo: put the SVG here, add it to `LOGOS` in `src/assets.rs`, and use its path in `src/links.rs`.
