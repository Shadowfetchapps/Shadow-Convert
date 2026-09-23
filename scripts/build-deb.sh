#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)"
ARCH="$(dpkg --print-architecture)"
NAME="shadow-convert_${VERSION}_${ARCH}.deb"
cd "$ROOT"
cargo build --release
mkdir -p "$ROOT/dist"
if command -v cargo-deb >/dev/null; then
  cargo deb --no-build --output "$ROOT/dist/$NAME"
  echo "Wrote $ROOT/dist/$NAME"
  exit 0
fi
STAGE="$ROOT/dist/deb-root"
rm -rf "$STAGE"
mkdir -p "$STAGE/DEBIAN" \
  "$STAGE/usr/bin" \
  "$STAGE/usr/share/applications" \
  "$STAGE/usr/share/doc/shadow-convert" \
  "$STAGE/usr/share/icons/hicolor/scalable/apps"
install -m 0755 "$ROOT/target/release/shadow-convert" "$STAGE/usr/bin/shadow-convert"
install -m 0644 "$ROOT/data/com.shadowfetch.Convert.desktop" \
  "$STAGE/usr/share/applications/com.shadowfetch.Convert.desktop"
install -m 0644 "$ROOT/data/icons/hicolor/scalable/apps/shadow-convert.svg" \
  "$STAGE/usr/share/icons/hicolor/scalable/apps/shadow-convert.svg"
for size in 16 24 32 48 64 128 256 512; do
  dest="$STAGE/usr/share/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$dest"
  install -m 0644 "$ROOT/data/icons/hicolor/${size}x${size}/apps/shadow-convert.png" \
    "$dest/shadow-convert.png"
done
install -m 0644 "$ROOT/README.md" "$STAGE/usr/share/doc/shadow-convert/README.md"
install -m 0644 "$ROOT/LICENSE" "$STAGE/usr/share/doc/shadow-convert/copyright"
SIZE="$(du -sk "$STAGE" | cut -f1)"
cat > "$STAGE/DEBIAN/control" <<EOF
Package: shadow-convert
Version: ${VERSION}
Section: video
Priority: optional
Architecture: ${ARCH}
Maintainer: Shadow Convert contributors <209457103+Shadowfetchapps@users.noreply.github.com>
Depends: ffmpeg, libgtk-4-1, libadwaita-1-0
Installed-Size: ${SIZE}
Homepage: https://github.com/Shadowfetchapps/Shadow-Convert
Description: Convert video, audio, and images locally
 Native GTK4 / libadwaita desktop application that converts
 video, audio, and images with FFmpeg. No account or telemetry.
EOF
dpkg-deb --build "$STAGE" "$ROOT/dist/$NAME"
echo "Wrote $ROOT/dist/$NAME"
