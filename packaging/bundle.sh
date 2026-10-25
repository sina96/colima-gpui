#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_DIR"
if [[ "$(uname -s)" != Darwin ]]; then
    echo "Packaging requires macOS and Xcode Command Line Tools." >&2
    exit 1
fi

# Match the minimum OS recorded in Info.plist. Dependencies retain their pins.
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release --locked
BUNDLE_VERSION="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"
[[ -n "$BUNDLE_VERSION" ]]
DIST_DIR="$PROJECT_DIR/dist"
mkdir -p "$DIST_DIR"
STAGING_DIR="$(mktemp -d "$DIST_DIR/.colima-build.XXXXXX")"
trap 'rm -rf "$STAGING_DIR"' EXIT
APP_DIR="$STAGING_DIR/Colima GPUI.app"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources/licenses"
install -m 755 target/release/colima-gpui "$APP_DIR/Contents/MacOS/colima-gpui"
cp packaging/Info.plist "$APP_DIR/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $BUNDLE_VERSION" "$APP_DIR/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $BUNDLE_VERSION" "$APP_DIR/Contents/Info.plist"
# App icon (built by packaging/make-icons.sh): AppIcon.icns for every macOS version, plus the
# Liquid Glass asset catalog for macOS 26 when it has been built.
cp packaging/AppIcon.icns "$APP_DIR/Contents/Resources/AppIcon.icns"
if [[ -f packaging/Assets.car ]]; then
    cp packaging/Assets.car "$APP_DIR/Contents/Resources/Assets.car"
fi
# Fonts are embedded in the executable; carry their licenses with the app.
cp assets/fonts/ibm-plex-sans/license.txt "$APP_DIR/Contents/Resources/licenses/IBM-Plex-Sans.txt"
cp assets/fonts/lilex/OFL.txt "$APP_DIR/Contents/Resources/licenses/Lilex.txt"
cp assets/icons/LICENSE "$APP_DIR/Contents/Resources/licenses/Lucide.txt"
printf 'APPL????' > "$APP_DIR/Contents/PkgInfo"
plutil -lint "$APP_DIR/Contents/Info.plist"
codesign --force --sign - "$APP_DIR"
codesign --verify --strict --verbose=2 "$APP_DIR"
# Replace an older generated bundle only after the new bundle verifies.
if [[ -e "$DIST_DIR/Colima GPUI.app" ]]; then
    mv "$DIST_DIR/Colima GPUI.app" "$STAGING_DIR/previous.app"
fi
mv "$APP_DIR" "$DIST_DIR/Colima GPUI.app"
echo "Built $DIST_DIR/Colima GPUI.app ($BUNDLE_VERSION, $(uname -m), ad-hoc signed)"
