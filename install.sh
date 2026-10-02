#!/bin/sh
# Herbarium installer.
#   curl -fsSL https://raw.githubusercontent.com/abdoufermat5/herbarium/main/install.sh | sh
#
# Options (pass after `sh -s --` when piping):
#   --version X.Y.Z   install a specific version (default: latest)
#   --appimage        install the AppImage to ~/.local even on Debian/Ubuntu
#   --uninstall       remove Herbarium
# Env: HERBARIUM_VERSION=X.Y.Z is equivalent to --version.
set -eu

REPO="abdoufermat5/herbarium"
VERSION="${HERBARIUM_VERSION:-}"
FORCE_APPIMAGE=0
UNINSTALL=0

say() { printf '%s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --version) [ $# -ge 2 ] || die "--version needs a value"; VERSION="$2"; shift ;;
    --appimage) FORCE_APPIMAGE=1 ;;
    --uninstall) UNINSTALL=1 ;;
    -h|--help) sed -n '2,10p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
  shift
done

BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/128x128/apps"

as_root() {
  if [ "$(id -u)" -eq 0 ]; then "$@"
  elif have sudo; then sudo "$@"
  else die "root privileges are required (install sudo or run as root)"
  fi
}

if [ "$UNINSTALL" -eq 1 ]; then
  if have dpkg && dpkg -s herbarium >/dev/null 2>&1; then
    as_root apt-get remove -y herbarium
  fi
  rm -f "$BIN_DIR/herbarium" "$APP_DIR/herbarium.desktop" "$ICON_DIR/herbarium.png"
  say "Herbarium removed. Your vault (your HTML files) was not touched."
  exit 0
fi

[ "$(uname -s)" = "Linux" ] || die "only Linux is supported for now"
case "$(uname -m)" in
  x86_64|amd64) ;;
  *) die "only x86_64 builds are published for now (found $(uname -m))" ;;
esac

if have curl; then
  fetch() { curl -fsSL "$1"; }
  download() { curl -fL --progress-bar -o "$2" "$1"; }
elif have wget; then
  fetch() { wget -qO- "$1"; }
  download() { wget -O "$2" "$1"; }
else
  die "curl or wget is required"
fi

if [ -z "$VERSION" ]; then
  say "Looking up the latest release..."
  VERSION="$(fetch "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' | head -n 1)"
  [ -n "$VERSION" ] || die "could not determine the latest version"
fi
VERSION="${VERSION#v}"
BASE="https://github.com/$REPO/releases/download/v$VERSION"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

if [ "$FORCE_APPIMAGE" -eq 0 ] && have apt-get && have dpkg; then
  say "Installing Herbarium $VERSION (.deb)..."
  download "$BASE/herbarium_${VERSION}_amd64.deb" "$TMP/herbarium.deb"
  as_root apt-get install -y "$TMP/herbarium.deb"
  say "Done. Launch Herbarium from your app menu or run: herbarium"
  exit 0
fi

say "Installing Herbarium $VERSION (AppImage) to $BIN_DIR..."
mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"
download "$BASE/herbarium_${VERSION}_amd64.AppImage" "$TMP/herbarium.AppImage"
chmod +x "$TMP/herbarium.AppImage"
mv "$TMP/herbarium.AppImage" "$BIN_DIR/herbarium"
download "https://raw.githubusercontent.com/$REPO/v$VERSION/src-tauri/icons/128x128.png" \
  "$ICON_DIR/herbarium.png" || say "warning: could not fetch the icon"

cat > "$APP_DIR/herbarium.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Version=1.0
Name=Herbarium
Comment=Keep generated HTML pages in a vault and review them on a schedule
Exec=$BIN_DIR/herbarium
Icon=herbarium
Terminal=false
Categories=Education;Utility;
StartupWMClass=herbarium
DESKTOP

have update-desktop-database && update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) say "note: add $BIN_DIR to your PATH to run \`herbarium\` from a terminal." ;;
esac
say "Done. Launch Herbarium from your app menu."
say "(AppImages need FUSE; if it won't start, install libfuse2 or run it with --appimage-extract-and-run.)"
