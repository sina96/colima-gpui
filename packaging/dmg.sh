#!/bin/bash
set -euo pipefail
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_DIR"
DIST_DIR="$PROJECT_DIR/dist"
APP_DIR="$DIST_DIR/Colima GPUI.app"
[[ -d "$APP_DIR" ]] || { echo 'Run just app first.' >&2; exit 1; }
codesign --verify --strict "$APP_DIR"
STAGING_DIR="$(mktemp -d "$DIST_DIR/.colima-dmg.XXXXXX")"
trap 'rm -rf "$STAGING_DIR"' EXIT
mkdir "$STAGING_DIR/volume"
ditto "$APP_DIR" "$STAGING_DIR/volume/Colima GPUI.app"
ln -s /Applications "$STAGING_DIR/volume/Applications"
hdiutil create -volname "Colima GPUI" -srcfolder "$STAGING_DIR/volume" -format UDZO -ov "$STAGING_DIR/Colima GPUI.dmg"
hdiutil verify "$STAGING_DIR/Colima GPUI.dmg"
mv -f "$STAGING_DIR/Colima GPUI.dmg" "$DIST_DIR/Colima GPUI.dmg"
echo "Built $DIST_DIR/Colima GPUI.dmg"
