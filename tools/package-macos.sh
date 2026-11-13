#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="${1:-}"
VERSION="${VERSION#v}"
if [[ ! "$VERSION" =~ ^[0-9] ]]; then
  VERSION="$(awk -F'"' '/^version = "/ { print $2; exit }' Cargo.toml)"
fi

BIN_PATH="${2:-}"
if [[ -z "$BIN_PATH" ]]; then
  if [[ -f "target/aarch64-apple-darwin/release/aitoolplus" ]]; then
    BIN_PATH="target/aarch64-apple-darwin/release/aitoolplus"
  else
    BIN_PATH="target/release/aitoolplus"
  fi
fi

if [[ ! -f "$BIN_PATH" ]]; then
  echo "Error: compiled binary not found at $BIN_PATH" >&2
  exit 1
fi

DIST_DIR="target/dist"
APP_NAME="AIToolPlus.app"
APP_DIR="$DIST_DIR/$APP_NAME"
DMG_NAME="aitoolplus-macos-arm64.dmg"
DMG_PATH="$DIST_DIR/$DMG_NAME"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

mkdir -p "$DIST_DIR"
rm -rf "$APP_DIR" "$DMG_PATH" "$DMG_PATH.sha256"

echo "Building macOS app bundle for v$VERSION from $BIN_PATH..."

mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$BIN_PATH" "$APP_DIR/Contents/MacOS/aitoolplus"
chmod 755 "$APP_DIR/Contents/MacOS/aitoolplus"

sed "s/__VERSION__/$VERSION/g" tools/macos/Info.plist > "$APP_DIR/Contents/Info.plist"

# Build AppIcon.icns from 1024x1024 source PNG using macOS sips + iconutil
ICON_SRC="assets/aitoolplus_icon_full.png"
if [[ ! -f "$ICON_SRC" ]]; then
  ICON_SRC="assets/aitoolplus_icon.png"
fi
ICONSET_DIR="$WORK_DIR/AppIcon.iconset"
mkdir -p "$ICONSET_DIR"

sips -s format png -z 16 16     "$ICON_SRC" --out "$ICONSET_DIR/icon_16x16.png"      >/dev/null
sips -s format png -z 32 32     "$ICON_SRC" --out "$ICONSET_DIR/icon_16x16@2x.png"   >/dev/null
sips -s format png -z 32 32     "$ICON_SRC" --out "$ICONSET_DIR/icon_32x32.png"      >/dev/null
sips -s format png -z 64 64     "$ICON_SRC" --out "$ICONSET_DIR/icon_32x32@2x.png"   >/dev/null
sips -s format png -z 128 128   "$ICON_SRC" --out "$ICONSET_DIR/icon_128x128.png"    >/dev/null
sips -s format png -z 256 256   "$ICON_SRC" --out "$ICONSET_DIR/icon_128x128@2x.png" >/dev/null
sips -s format png -z 256 256   "$ICON_SRC" --out "$ICONSET_DIR/icon_256x256.png"    >/dev/null
sips -s format png -z 512 512   "$ICON_SRC" --out "$ICONSET_DIR/icon_256x256@2x.png" >/dev/null
sips -s format png -z 512 512   "$ICON_SRC" --out "$ICONSET_DIR/icon_512x512.png"    >/dev/null
sips -s format png -z 1024 1024 "$ICON_SRC" --out "$ICONSET_DIR/icon_512x512@2x.png" >/dev/null
iconutil -c icns "$ICONSET_DIR" -o "$APP_DIR/Contents/Resources/AppIcon.icns"

# Clean extended attributes and sign (ad-hoc by default; Developer ID if configured)
xattr -cr "$APP_DIR"
if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  codesign --force --deep --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$APP_DIR"
else
  codesign --force --deep --sign - "$APP_DIR"
fi

# Package drag-and-drop DMG via deterministic hdiutil
DMG_STAGE="$WORK_DIR/dmg-staging"
mkdir -p "$DMG_STAGE"
cp -R "$APP_DIR" "$DMG_STAGE/"
ln -s /Applications "$DMG_STAGE/Applications"

hdiutil create \
  -volname "AI ToolPlus" \
  -srcfolder "$DMG_STAGE" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

shasum -a 256 "$DMG_PATH" | awk '{print tolower($1)}' > "$DMG_PATH.sha256"
echo "Packaged $DMG_PATH (sha256: $(cat "$DMG_PATH.sha256"))"
