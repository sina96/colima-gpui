#!/bin/bash
# Builds the app icon files that bundle.sh copies into the .app, from assets/brand/ColimaGPUI.icon.
# Run it (`just icons`) after changing the icon, and commit the results, so building the app
# never needs Xcode.
#
#   packaging/AppIcon.icns  every macOS version; rendered with Icon Composer's ictool
#   packaging/Assets.car    macOS 26 and later (Liquid Glass); compiled with actool, if available
#   assets/brand/icon.png   512 px render for the README
#
# Needs Xcode 26 or later (it contains Icon Composer and actool). actool also needs, once, the
# Xcode license accepted and Xcode's system components installed: open Xcode and let it
# install them, or run
#   sudo /Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild -license accept
#   sudo /Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild -runFirstLaunch
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_DIR"
ICON="assets/brand/ColimaGPUI.icon"
XCODE="${XCODE:-/Applications/Xcode.app}"
ICTOOL="$XCODE/Contents/Applications/Icon Composer.app/Contents/Executables/ictool"
[[ -x "$ICTOOL" ]] || { echo "ictool not found; install Xcode 26 or set XCODE=/path/to/Xcode.app" >&2; exit 1; }

STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

render() { # render <points> <scale> <output>: the icon at <points>, padded to the macOS grid
    # ictool fills its whole canvas with the rounded square. macOS app icons keep it at 824 of
    # 1024 px with a transparent margin, so render it that much smaller; pad_all adds the margin.
    local pixels=$(($1 * $2)) tile=$((($1 * $2 * 824 + 512) / 1024))
    "$ICTOOL" "$ICON" --export-image --output-file "$3" --platform macOS --rendition Default \
        --width "$tile" --height "$tile" --scale 1 > /dev/null
    echo "$3 $pixels" >> "$STAGING/pad.txt"
}

pad_all() { # centers every rendered tile on a transparent canvas of its final size
    swift - "$STAGING/pad.txt" <<'SWIFT'
import AppKit
for line in try! String(contentsOfFile: CommandLine.arguments[1], encoding: .utf8).split(separator: "\n") {
    let parts = line.split(separator: " ")
    let path = String(parts[0]), size = Int(parts[1])!
    let tile = NSBitmapImageRep(data: try! Data(contentsOf: URL(fileURLWithPath: path)))!
    let canvas = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: canvas)
    let offset = (size - tile.pixelsWide) / 2
    tile.draw(in: NSRect(x: offset, y: offset, width: tile.pixelsWide, height: tile.pixelsHigh))
    NSGraphicsContext.restoreGraphicsState()
    try! canvas.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}
SWIFT
}

# Classic icon set: 16–512 pt at 1x and 2x.
ICONSET="$STAGING/AppIcon.iconset"
mkdir "$ICONSET"
for points in 16 32 128 256 512; do
    render "$points" 1 "$ICONSET/icon_${points}x${points}.png"
    render "$points" 2 "$ICONSET/icon_${points}x${points}@2x.png"
done
render 256 2 "$STAGING/icon.png"
pad_all
iconutil -c icns "$ICONSET" -o packaging/AppIcon.icns
echo "Built packaging/AppIcon.icns"
cp "$STAGING/icon.png" assets/brand/icon.png
echo "Built assets/brand/icon.png"

# Liquid Glass asset catalog for macOS 26. Optional: without it macOS uses AppIcon.icns.
# Call actool by path: xcrun would first run xcodebuild, which fails with only the Command
# Line Tools selected (xcode-select).
if DEVELOPER_DIR="$XCODE/Contents/Developer" "$XCODE/Contents/Developer/usr/bin/actool" "$ICON" --compile "$STAGING" \
    --app-icon ColimaGPUI --enable-on-demand-resources NO --development-region en \
    --target-device mac --platform macosx --minimum-deployment-target 11.0 \
    --output-partial-info-plist "$STAGING/partial.plist" > "$STAGING/actool.log" 2>&1 \
    && [[ -f "$STAGING/Assets.car" ]]; then
    cp "$STAGING/Assets.car" packaging/Assets.car
    echo "Built packaging/Assets.car"
else
    echo "Skipped packaging/Assets.car (actool failed; see below). The app uses AppIcon.icns." >&2
    tail -3 "$STAGING/actool.log" >&2
fi
