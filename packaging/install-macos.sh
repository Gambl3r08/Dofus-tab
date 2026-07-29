#!/usr/bin/env bash
# Installs Dofus-tab into ~/Applications as a macOS .app bundle.
set -euo pipefail

APP_ID="io.github.gambl3r08.DofusTab"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$HERE")"

DEST="$HOME/Applications/DofusTab.app"
ICONS_DIR="$HERE/icons"

echo "Building..."
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo "Creating .app bundle at $DEST"
rm -rf "$DEST"
mkdir -p "$DEST/Contents/MacOS"
mkdir -p "$DEST/Contents/Resources"

echo "Copying binaries …"
install -m755 "$ROOT/target/release/dofus-tab-bar" "$DEST/Contents/MacOS/"
install -m755 "$ROOT/target/release/dofus-tab" "$DEST/Contents/MacOS/"

echo "Generating AppIcon.icns …"
ICONSET_DIR="$(mktemp -d)"
ICONSET="${ICONSET_DIR}.iconset"
mv "$ICONSET_DIR" "$ICONSET"

# Map PNG sizes to .iconset filenames (1x and @2x pairs)
# Standard iconset: 16, 32, 64, 128, 256, 512 (and optionally 1024)
cp "$ICONS_DIR/dofus-tab-16.png"  "$ICONSET/icon_16x16.png"
cp "$ICONS_DIR/dofus-tab-32.png"  "$ICONSET/icon_16x16@2x.png"
cp "$ICONS_DIR/dofus-tab-32.png"  "$ICONSET/icon_32x32.png"
cp "$ICONS_DIR/dofus-tab-64.png"  "$ICONSET/icon_32x32@2x.png"
cp "$ICONS_DIR/dofus-tab-128.png" "$ICONSET/icon_128x128.png"
cp "$ICONS_DIR/dofus-tab-256.png" "$ICONSET/icon_128x128@2x.png"
cp "$ICONS_DIR/dofus-tab-256.png" "$ICONSET/icon_256x256.png"
cp "$ICONS_DIR/dofus-tab-512.png" "$ICONSET/icon_256x256@2x.png"
cp "$ICONS_DIR/dofus-tab-512.png" "$ICONSET/icon_512x512.png"

iconutil -c icns "$ICONSET" --output "$DEST/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"

echo "Copying Info.plist …"
install -m644 "$HERE/Info.plist" "$DEST/Contents/"

echo
echo "✅ Done. App bundle created at:"
echo "   $DEST"
echo
echo "You can open it from Spotlight (Cmd+Space, type \"Dofus-tab\")"
echo "or from Launchpad."
echo
echo "If you want to launch it right now from the terminal:"
echo "   open $DEST"
