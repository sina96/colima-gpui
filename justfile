# Task runner for Colima GPUI. Install just with `brew install just`, then run `just` to
# list the recipes. Every recipe is a thin wrapper, so the commands also work without just.

# List the recipes.
default:
    @just --list --unsorted

# Run the app (debug build).
run:
    cargo run

# Open the UI gallery: every widget in src/ui/ in one window.
gallery:
    cargo run --example ui_gallery

# Build the app (debug).
build:
    cargo build

# Run the tests.
test:
    cargo test

# Format the code in place.
fmt:
    cargo fmt

# Check formatting and lints, as CI does.
lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings

# Everything CI runs: lint, then test. Run it before you open a pull request.
check: lint test

# Build dist/Colima GPUI.app (release build for this Mac's architecture, ad-hoc signed).
app:
    ./packaging/bundle.sh

# Rebuild the app icon files from assets/brand/ColimaGPUI.icon (needs Xcode 26).
icons:
    ./packaging/make-icons.sh

# Build dist/Colima GPUI.dmg from a fresh app bundle.
dmg: app
    ./packaging/dmg.sh

# Remove build output (target/ and dist/).
clean:
    cargo clean
    rm -rf dist
