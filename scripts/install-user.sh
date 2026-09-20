#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${SHADOW_CONVERT_BIN:-$ROOT/target/release/shadow-convert}"
if [[ ! -x "$BIN" ]]; then
  echo "Building release binary…"
  (cd "$ROOT" && cargo build --release)
  BIN="$ROOT/target/release/shadow-convert"
fi
PREFIX="${XDG_DATA_HOME:-$HOME/.local/share}"
BINDIR="${HOME}/.local/bin"
mkdir -p "$BINDIR" \
  "$PREFIX/applications" \
  "$PREFIX/icons/hicolor/scalable/apps"
install -m 0755 "$BIN" "$BINDIR/shadow-convert"
install -m 0644 "$ROOT/data/com.shadowfetch.Convert.desktop" \
  "$PREFIX/applications/com.shadowfetch.Convert.desktop"
install -m 0644 "$ROOT/data/icons/hicolor/scalable/apps/shadow-convert.svg" \
  "$PREFIX/icons/hicolor/scalable/apps/shadow-convert.svg"
for size in 16 24 32 48 64 128 256 512; do
  dest="$PREFIX/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$dest"
  install -m 0644 "$ROOT/data/icons/hicolor/${size}x${size}/apps/shadow-convert.png" \
    "$dest/shadow-convert.png"
done
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$PREFIX/applications" || true
fi
if command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache -f -t "$PREFIX/icons/hicolor" >/dev/null 2>&1 || true
fi
desktop-file-validate "$PREFIX/applications/com.shadowfetch.Convert.desktop"
test -x "$BINDIR/shadow-convert"
echo "Installed $BINDIR/shadow-convert"
echo "Desktop: $PREFIX/applications/com.shadowfetch.Convert.desktop"
