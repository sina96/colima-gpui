# Contributing to Colima GPUI

Thank you for your help. This guide tells you how to build, test and change Colima GPUI.

All contributors must follow the [Code of Conduct](CODE_OF_CONDUCT.md). To report a security problem, read [SECURITY.md](SECURITY.md). Don't open a public issue for it.

## Requirements

- A Mac with macOS 11 or later.
- The Xcode Command Line Tools: `xcode-select --install`.
- [Rust](https://rustup.rs) through `rustup`. The exact version is pinned in `rust-toolchain.toml`, and `rustup` installs it, with `rustfmt` and `clippy`, on first use.
- [just](https://github.com/casey/just), the task runner: `brew install just`.
- [Colima](https://github.com/abiosoft/colima) and the Docker CLI, for trying changes by hand: `brew install colima docker`.

The first build compiles GPUI from the Zed repository and takes a few minutes. Later builds are incremental.

## Build, run and test

| Command | What it does |
|---|---|
| `just` | Lists the recipes. |
| `just run` | Runs the app (debug build). |
| `just gallery` | Opens the UI gallery: every widget in `src/ui/` in one window. |
| `just test` | Runs the tests (`cargo test`). |
| `just fmt` | Formats the code in place (`cargo fmt`). |
| `just lint` | Checks formatting and runs clippy with warnings as errors. |
| `just check` | Runs `just lint`, then `just test`. CI runs the same checks. |
| `just app` | Builds `dist/Colima GPUI.app`: a release build, ad-hoc signed. |
| `just dmg` | Builds the app and `dist/Colima GPUI.dmg`. |
| `just icons` | Rebuilds the app icon files from `assets/brand/ColimaGPUI.icon`. Needs Xcode 26. |
| `just clean` | Removes `target/` and `dist/`. |

Each recipe wraps a `cargo` command or a script in `packaging/`, so you can also run those directly.

### Trying changes safely

The app drives your real Colima installation. For start, stop and delete tests, create a throwaway profile and use only that one:

```sh
colima start --profile gpui-test --cpus 1 --memory 1 --disk 10
```

Never test destructive actions on a profile you rely on, such as `default`.

## Project layout

| Path | What it holds |
|---|---|
| `src/colima/`, `src/docker/` | The CLI calls and the parsing of their JSON output. Plain Rust with no GPUI, so they are unit-tested. |
| `src/views/` | The app's screens: profiles, overview, containers, images, volumes, cleanup, AI and settings. |
| `src/ui/` | Reusable widgets (buttons, tables, cards, switches, …). GPUI ships none, so they live here. |
| `src/tray/` | The macOS menu bar icon and the Dock menu. |
| `src/theme.rs` | The color profiles and every color token. |
| `tests/fixtures/` | Captured CLI output that the tests parse. |
| `examples/` | The UI gallery and small widget demos. |
| `packaging/` | The scripts and files that build the `.app` and the `.dmg`, and the generated app icon. |
| `assets/brand/` | The app icon (Icon Composer), the logo and the menu bar glyph. See its README. |

## Coding conventions

[AGENTS.md](AGENTS.md) has the full rules for humans and coding agents. In short:

- **Dependencies:** only GPUI from the Zed repository, plus `serde`/`serde_json` and `tray-icon`. Open an issue before you add another crate.
- **CLI calls:** every call goes through the shared runner in `src/colima/cli.rs`, and runs on the background executor, never on the UI thread.
- **Colors:** come from theme tokens in `src/theme.rs`, never inline. A new token needs a value in all four themes (Colima and Zed One, light and dark).
- **New widgets:** go in `src/ui/` and in the UI gallery.
- **GPUI is pre-1.0:** check APIs in the pinned Zed checkout under `~/.cargo/git/checkouts/`, not from memory.
- **Tests:** a new parser gets a test against captured output in `tests/fixtures/`. Remove personal data from captures.

## Pull requests

1. Fork the repository and create a branch from `main`.
2. Make your change and keep it focused: one feature or fix per pull request.
3. Run `just check`, and try the change in the app.
4. If users see the change, add a line under `[Unreleased]` in [CHANGELOG.md](CHANGELOG.md).
5. Open the pull request and fill in the template. Add screenshots for UI changes.

CI runs `just lint` and `just test` on macOS, builds the examples and checks the packaging files. It doesn't build the app bundle, so run `just app` yourself if you change anything in `packaging/`. A pull request needs a green CI run before it's merged.

## Releasing

Releases are made by the maintainer from a version tag. `.github/workflows/release.yml` does the rest:
- It checks that the tag matches `Cargo.toml` and that `CHANGELOG.md` has notes for the version.
- It builds the app on an Apple silicon and an Intel runner.
- It publishes a GitHub Release with each architecture's `.dmg` and `.zip` and a `SHA256SUMS` file, using that version's changelog section as the notes.

1. On `main`, set `version` in `Cargo.toml`, e.g. `0.2.0`, and run `cargo build` so `Cargo.lock` follows.
2. In `CHANGELOG.md`:
   1. Move the `[Unreleased]` entries under a new `## [0.2.0] - YYYY-MM-DD` heading, and leave `## [Unreleased]` empty above it.
   2. Update the links at the bottom.
   3. Preview the notes with `packaging/release-notes.sh 0.2.0`.
3. Commit, push, and wait for CI to pass.
4. Tag and push the tag:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

5. Follow the **Release** workflow in the Actions tab. After both builds, it waits for approval: click **Review deployments**, tick **release** and **Approve and deploy**. A minute later the release is on the Releases page.

Only the repository owner can release: a tag ruleset limits `v*` tags to the owner, the workflow stops if anyone else pushed the tag, and publishing needs the owner's approval in the `release` environment.

If the workflow fails before publishing, fix the problem, then move the tag and push it again:

```sh
git tag -d v0.2.0
git push origin :refs/tags/v0.2.0
git tag v0.2.0
git push origin v0.2.0
```

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
