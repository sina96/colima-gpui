#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Linux ]] || { echo 'AppImage packaging requires Linux.' >&2; exit 1; }
arch="$(uname -m)"
case "$arch" in
    x86_64)
        tool_sum=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
        runtime_id=657374641
        runtime_sum=617aa77314d57c8b8e4fae9c3f0b35af7b30d5529cb69e98f64f92510ed98a46 ;;
    aarch64)
        tool_sum=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
        runtime_id=657373827
        runtime_sum=9a72a744d221576c9347ec074243acc5c3c7568896b12714e0e82e9f0bf47900 ;;
    *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
esac
cargo build --release --locked
version="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"
dist="${APPIMAGE_DIST_DIR:-$PWD/dist}"
mkdir -p "$dist"
stage="$(mktemp -d "$dist/.appimage.XXXXXXXX")"
trap 'rm -rf "$stage"' EXIT
app="$stage/Colima-GPUI.AppDir"
mkdir -p "$app/usr/bin" "$app/usr/share/licenses/colima-gpui"
install -m 755 "${CARGO_TARGET_DIR:-target}/release/colima-gpui" "$app/usr/bin/colima-gpui"
install -m 755 packaging/linux/AppRun "$app/AppRun"
cp packaging/linux/colima-gpui.desktop "$app/"
cp assets/brand/icon.png "$app/colima-gpui.png"
cp assets/fonts/ibm-plex-sans/license.txt "$app/usr/share/licenses/colima-gpui/IBM-Plex-Sans.txt"
cp assets/fonts/lilex/OFL.txt "$app/usr/share/licenses/colima-gpui/Lilex.txt"
cp assets/icons/LICENSE "$app/usr/share/licenses/colima-gpui/Lucide.txt"
# Official appimagetool 1.9.1 and type2 runtime fedfad51, pinned by SHA-256.
# The release asset ID stays tied to these bytes even when continuous moves.
curl -fL --retry 3 "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-$arch.AppImage" -o "$stage/tool"
curl -fL --retry 3 -H "Accept: application/octet-stream" "https://api.github.com/repos/AppImage/type2-runtime/releases/assets/$runtime_id" -o "$stage/runtime"
printf '%s  %s\n' "$tool_sum" "$stage/tool" "$runtime_sum" "$stage/runtime" | sha256sum -c -
chmod +x "$stage/tool"
# Extract the tool instead of requiring FUSE in builders/containers.
(cd "$stage" && ./tool --appimage-extract >/dev/null)
ARCH="$arch" "$stage/squashfs-root/AppRun" --runtime-file "$stage/runtime" "$app" "$stage/result.AppImage"
chmod +x "$stage/result.AppImage"
mv "$stage/result.AppImage" "$dist/Colima-GPUI-$version-linux-$arch.AppImage"
echo "Built $dist/Colima-GPUI-$version-linux-$arch.AppImage"
