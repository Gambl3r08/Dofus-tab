#!/usr/bin/env bash
# Installs Dofus-tab into the current user's home.
#
# The desktop entry matters beyond the application menu: the XDG desktop portal
# derives our application identity from the systemd scope the launcher puts us
# in, and that scope is named after this file. Started from a terminal instead,
# the global shortcuts end up filed under the terminal's name.
set -euo pipefail

APP_ID="io.github.gambl3r08.DofusTab"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$HERE")"

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"

echo "Building..."
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo "Installing binaries into $BIN_DIR"
mkdir -p "$BIN_DIR"
install -m755 "$ROOT/target/release/dofus-tab" "$BIN_DIR/"
install -m755 "$ROOT/target/release/dofus-tab-bar" "$BIN_DIR/"

echo "Installing icons into $DATA_DIR/icons"
mkdir -p "$DATA_DIR/icons"
cp -r "$HERE/icons/hicolor" "$DATA_DIR/icons/"

echo "Installing the desktop entry into $DATA_DIR/applications"
mkdir -p "$DATA_DIR/applications"
install -m644 "$HERE/$APP_ID.desktop" "$DATA_DIR/applications/"

command -v update-desktop-database >/dev/null && \
  update-desktop-database "$DATA_DIR/applications" || true
command -v gtk-update-icon-cache >/dev/null && \
  gtk-update-icon-cache -f -t "$DATA_DIR/icons/hicolor" 2>/dev/null || true

echo
echo "Done. Launch Dofus-tab from your application menu so the desktop portal"
echo "sees it as $APP_ID rather than as your terminal."
echo "Make sure $BIN_DIR is on your PATH."
