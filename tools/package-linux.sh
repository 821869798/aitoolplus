#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

ARCH="${1:-$(uname -m)}"
case "$ARCH" in
  x86_64|amd64)
    TAR_ARCH="x86_64"
    DEB_ARCH="amd64"
    TARGET_TRIPLE="x86_64-unknown-linux-gnu"
    ;;
  aarch64|arm64)
    TAR_ARCH="aarch64"
    DEB_ARCH="arm64"
    TARGET_TRIPLE="aarch64-unknown-linux-gnu"
    ;;
  *)
    echo "Unsupported Linux architecture: $ARCH" >&2
    exit 1
    ;;
esac

VERSION="${2:-}"
VERSION="${VERSION#v}"
if [[ ! "$VERSION" =~ ^[0-9] ]]; then
  VERSION="$(awk -F'"' '/^version = "/ { print $2; exit }' Cargo.toml)"
fi

BIN_PATH="${3:-}"
if [[ -z "$BIN_PATH" ]]; then
  if [[ -f "target/$TARGET_TRIPLE/release/aitoolplus" ]]; then
    BIN_PATH="target/$TARGET_TRIPLE/release/aitoolplus"
  else
    BIN_PATH="target/release/aitoolplus"
  fi
fi

if [[ ! -f "$BIN_PATH" ]]; then
  echo "Error: compiled binary not found at $BIN_PATH" >&2
  exit 1
fi

DIST_DIR="target/dist"
TAR_NAME="aitoolplus-linux-${TAR_ARCH}.tar.gz"
TAR_PATH="$DIST_DIR/$TAR_NAME"
DEB_NAME="aitoolplus-linux-${DEB_ARCH}.deb"
DEB_PATH="$DIST_DIR/$DEB_NAME"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

mkdir -p "$DIST_DIR"
rm -f "$TAR_PATH" "$TAR_PATH.sha256" "$DEB_PATH" "$DEB_PATH.sha256"

echo "Packaging Linux ($TAR_ARCH / $DEB_ARCH) v$VERSION from $BIN_PATH..."

# 1. Universal .tar.gz archive (binary at root for seamless in-app tar self-update)
TAR_STAGE="$WORK_DIR/tar"
mkdir -p "$TAR_STAGE"
cp "$BIN_PATH" "$TAR_STAGE/aitoolplus"
chmod 755 "$TAR_STAGE/aitoolplus"
cp tools/linux/aitoolplus.desktop "$TAR_STAGE/aitoolplus.desktop"
cp assets/aitoolplus_icon.png "$TAR_STAGE/aitoolplus.png"

cat > "$TAR_STAGE/install.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${HOME}/.local"
mkdir -p "$PREFIX/bin" "$PREFIX/share/applications" "$PREFIX/share/icons/hicolor/256x256/apps"
install -m 755 "$SCRIPT_DIR/aitoolplus" "$PREFIX/bin/aitoolplus"
install -m 644 "$SCRIPT_DIR/aitoolplus.png" "$PREFIX/share/icons/hicolor/256x256/apps/aitoolplus.png"
install -m 644 "$SCRIPT_DIR/aitoolplus.desktop" "$PREFIX/share/applications/aitoolplus.desktop"
update-desktop-database "$PREFIX/share/applications" >/dev/null 2>&1 || true
echo "Installed aitoolplus to $PREFIX/bin/aitoolplus"
EOF
chmod 755 "$TAR_STAGE/install.sh"

tar -czf "$TAR_PATH" -C "$TAR_STAGE" .
sha256sum "$TAR_PATH" | awk '{print tolower($1)}' > "$TAR_PATH.sha256"

# 2. Debian/Ubuntu .deb package
DEB_STAGE="$WORK_DIR/deb"
mkdir -p \
  "$DEB_STAGE/DEBIAN" \
  "$DEB_STAGE/usr/bin" \
  "$DEB_STAGE/usr/share/applications" \
  "$DEB_STAGE/usr/share/icons/hicolor/256x256/apps"

install -m 755 "$BIN_PATH" "$DEB_STAGE/usr/bin/aitoolplus"
install -m 644 tools/linux/aitoolplus.desktop "$DEB_STAGE/usr/share/applications/aitoolplus.desktop"
install -m 644 assets/aitoolplus_icon.png "$DEB_STAGE/usr/share/icons/hicolor/256x256/apps/aitoolplus.png"

cat > "$DEB_STAGE/DEBIAN/control" <<EOF
Package: aitoolplus
Version: $VERSION
Section: utils
Priority: optional
Architecture: $DEB_ARCH
Maintainer: aitoolplus contributors <https://github.com/821869798/aitoolplus>
Depends: libgtk-3-0 | libgtk-3-0t64, libayatana-appindicator3-1, libfontconfig1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libvulkan1, libzstd1, libssl3 | libssl3t64
Homepage: https://github.com/821869798/aitoolplus
Description: Native GPU-accelerated AI coding assistant workbench on GPUI
 Manage Claude Code, Codex, Gemini CLI, Pi, Grok, OpenCode, MCP servers,
 skills, sessions, and Antigravity multi-account quotas in a pure Rust app.
EOF

cat > "$DEB_STAGE/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
exit 0
EOF
chmod 755 "$DEB_STAGE/DEBIAN/postinst"

dpkg-deb --root-owner-group --build "$DEB_STAGE" "$DEB_PATH"
sha256sum "$DEB_PATH" | awk '{print tolower($1)}' > "$DEB_PATH.sha256"

echo "Packaged $TAR_PATH (sha256: $(cat "$TAR_PATH.sha256"))"
echo "Packaged $DEB_PATH (sha256: $(cat "$DEB_PATH.sha256"))"
