# Colima GPUI: notes for contributors and coding agents

Native GUI for managing [Colima](https://github.com/abiosoft/colima), built with Zed's official GPUI. This file covers the stack, the look, the architecture rules and the checks every change must pass.

## Stack

- `gpui` + `gpui_platform` from the Zed repo, pinned to one git `rev` in `Cargo.toml`. Both entries must always share the same rev.
- Rust toolchain pinned in `rust-toolchain.toml` to match that Zed rev. Bump both together, and only in a change dedicated to the bump.
- Dependencies are official only: GPUI from Zed, plus `serde`/`serde_json`. The one exception is `tray-icon` (menu bar icon). Discuss any other crate before adding it. Build widgets yourself in `src/ui/`.
- Colima and Docker are driven through their CLIs with JSON output (`colima list --json`, `colima status --json -p <profile>`, `docker ps --format json`).

## Look and feel: Zed

The app should feel like a sibling of Zed: dense, flat, quiet, keyboard-friendly. Zed's own `ui` crate is the reference; read it in the pinned checkout (`crates/ui/src/components/`) and match its proportions rather than inventing new ones.

- **Colors:** two color profiles, both following the system appearance (or the user's Light/Dark choice):
  - **Colima** (the default): colima.run's colors with a green accent.
  - **Zed One:** Zed's One Dark and One Light.
- **Color tokens:** live in `src/theme.rs` under Zed's token names (`surface_background`, `border_variant`, `element_hover`, `text_muted`, ...).
  - Every token needs a value in all four themes.
  - Status maps to `success`, `warning`, `error` and `text_muted`.
  - A chosen item uses `ButtonStyle::Selected` or `element_selected`, and its hover uses `element_selected_hover`, never a grey hover.
- **Fonts:** IBM Plex Sans for UI text and Lilex for monospace (logs, IDs, IPs). These are Zed's defaults, bundled in `assets/fonts/` and loaded at startup.
- **Sizing:**
  - Body text is `text_sm`.
  - Button heights are 22 px by default, 28 px medium and 32 px large.
  - Corners are `rounded_sm`.
  - Borders are 1 px: `border_variant` for dividers and `border` for controls.
- **Structure:**
  - A 32 px custom title bar with transparent macOS chrome and no title text. The traffic lights sit at (9, 9) and the title bar has 71 px of left padding to clear them.
  - Panels sit on `surface_background` with the main content on `editor_background`.
  - A thin status bar at the bottom.
- **Controls:**
  - Buttons are ghost buttons with hover fills by default. Use the filled accent only for the one primary action in view.
  - Icons and labels are `text_muted` until hovered or active.
  - On/off settings use `ui::Switch`.
  - The focused element gets a `border_focused` ring.

## GPUI API: look it up in the pinned source

GPUI is pre-1.0 and breaks often, so remembered APIs are usually stale. The source of truth is the pinned checkout:

```
~/.cargo/git/checkouts/zed-*/<rev>/crates/gpui/            # src/ and examples/
~/.cargo/git/checkouts/zed-*/<rev>/crates/gpui/examples/   # input.rs, uniform_list.rs, set_menus.rs, popover.rs, scrollable.rs, ...
```

Before using an API you are unsure of, grep for it there and copy the pattern from an example.

## Architecture rules

- `src/colima/` and `src/docker/` are plain Rust with no `gpui` import, so they stay unit-testable.
- Every CLI call goes through the shared runner in `src/colima/cli.rs`.
  - It resolves the binary and the child's `PATH`. A `.app` launched from Finder gets a bare `PATH` with no `/opt/homebrew/bin`, and colima itself shells out to `limactl`/`docker`.
  - It checks `$COLIMA_BIN`, then `/opt/homebrew/bin`, `/usr/local/bin`, `~/.nix-profile/bin`, then `PATH`, and prepends those directories to the child's `PATH`.
- Run CLI calls on `cx.background_executor()`, then apply the result with `this.update(cx, ...)` and `cx.notify()`. The UI thread only renders.
- Commands that prompt get non-interactive flags (`colima delete --force`).
- Colors and spacing come from `src/theme.rs`. Views use theme tokens rather than inline `rgb(...)` literals.
- The UI gallery (`cargo run --example ui_gallery`) shows every widget in `src/ui/`. Add new widgets to it.

## Verify before calling a change done

```
just check   # cargo fmt --check, cargo clippy --all-targets -- -D warnings, cargo test
```

CI (`.github/workflows/ci.yml`) runs the same checks and builds the app bundle on every pull request.

- **UI changes:** also run `cargo run` and check the feature by hand.
- **Tests:** they parse captured CLI output stored in `tests/fixtures/`.
- **Profiles:** manual start/stop/delete checks use a throwaway profile named `gpui-test`. Never touch real profiles such as `default`.
