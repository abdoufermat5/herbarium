#!/bin/sh
# Herbarium installer.
#   curl -fsSL https://raw.githubusercontent.com/abdoufermat5/herbarium/main/install.sh | sh
# Run with --help for the options.
set -eu

REPO="abdoufermat5/herbarium"
VERSION="${HERBARIUM_VERSION:-}"
FORCE_APPIMAGE=0
UNINSTALL=0

say() { printf '%s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
  cat <<'USAGE'
Herbarium installer.
  curl -fsSL https://raw.githubusercontent.com/abdoufermat5/herbarium/main/install.sh | sh

Options (pass after `sh -s --` when piping):
  --version X.Y.Z   install a specific version (default: latest)
  --appimage        install the AppImage to ~/.local even on Debian/Ubuntu
  --uninstall       remove Herbarium
  -h, --help        show this help
Env: HERBARIUM_VERSION=X.Y.Z is equivalent to --version.

Supported: Linux on x86_64 and aarch64. Downloads are verified against the
release's SHA256SUMS before anything is installed.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) [ $# -ge 2 ] || die "--version needs a value"; VERSION="$2"; shift ;;
    --appimage) FORCE_APPIMAGE=1 ;;
    --uninstall) UNINSTALL=1 ;;
    help|-h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (see --help)" ;;
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
# Asset arch suffixes as produced by the Tauri bundler: .deb uses Debian names,
# the AppImage uses amd64 / aarch64.
case "$(uname -m)" in
  x86_64|amd64) DEB_ARCH="amd64"; APPIMAGE_ARCH="amd64" ;;
  aarch64|arm64) DEB_ARCH="arm64"; APPIMAGE_ARCH="aarch64" ;;
  *) die "only x86_64 and aarch64 builds are published (found $(uname -m))" ;;
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

if have sha256sum; then
  sha256_of() { sha256sum "$1" | cut -d ' ' -f 1; }
elif have shasum; then
  sha256_of() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
  die "sha256sum or shasum is required to verify the download"
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

# download_verified ASSET DEST: downloads a release asset and checks it against
# the release's SHA256SUMS; aborts (leaving nothing installed) on any mismatch.
download_verified() {
  if [ ! -f "$TMP/SHA256SUMS" ]; then
    download "$BASE/SHA256SUMS" "$TMP/SHA256SUMS" \
      || die "could not download SHA256SUMS for v$VERSION; refusing to install unverified files"
  fi
  expected="$(awk -v f="$1" '$2 == f || $2 == "*" f { print $1; exit }' "$TMP/SHA256SUMS")"
  [ -n "$expected" ] || die "$1 is not listed in SHA256SUMS for v$VERSION"
  download "$BASE/$1" "$2"
  actual="$(sha256_of "$2")"
  if [ "$actual" != "$expected" ]; then
    rm -f "$2"
    die "checksum mismatch for $1 (expected $expected, got $actual); aborting"
  fi
  say "Verified $1 (sha256 $actual)."
}

mcp_hint() {
  say "To let AI agents use your vault over MCP, point them at the binary, e.g. in Claude Code:"
  say "  claude mcp add herbarium -- $1 mcp"
}

if [ "$FORCE_APPIMAGE" -eq 0 ] && have apt-get && have dpkg; then
  say "Installing Herbarium $VERSION (.deb, $DEB_ARCH)..."
  download_verified "herbarium_${VERSION}_${DEB_ARCH}.deb" "$TMP/herbarium.deb"
  as_root apt-get install -y "$TMP/herbarium.deb"
  BIN_PATH=""
  if have herbarium; then
    RESOLVED="$(command -v herbarium 2>/dev/null || true)"
    case "$RESOLVED" in
      /*) BIN_PATH="$RESOLVED" ;;
    esac
  fi
  [ -n "$BIN_PATH" ] || BIN_PATH="/usr/bin/herbarium"
  say "Done. Launch Herbarium from your app menu or run: herbarium"
  mcp_hint "$BIN_PATH"
  exit 0
fi

say "Installing Herbarium $VERSION (AppImage, $APPIMAGE_ARCH) to $BIN_DIR..."
mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"
download_verified "herbarium_${VERSION}_${APPIMAGE_ARCH}.AppImage" "$TMP/herbarium.AppImage"
chmod +x "$TMP/herbarium.AppImage"
mv "$TMP/herbarium.AppImage" "$BIN_DIR/herbarium"
# Older releases keep the app in src-tauri/ instead of desktop/.
download "https://raw.githubusercontent.com/$REPO/v$VERSION/desktop/icons/128x128.png" "$ICON_DIR/herbarium.png" \
  || download "https://raw.githubusercontent.com/$REPO/v$VERSION/src-tauri/icons/128x128.png" "$ICON_DIR/herbarium.png" \
  || say "warning: could not fetch the icon"

cat > "$APP_DIR/herbarium.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Version=1.0
Name=Herbarium
Comment=Keep generated HTML pages in a vault and review them on a schedule
Exec=$BIN_DIR/herbarium
Icon=herbarium
Terminal=false
Categories=Education;
StartupWMClass=herbarium
DESKTOP

have update-desktop-database && update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) say "note: add $BIN_DIR to your PATH to run \`herbarium\` from a terminal." ;;
esac
say "Done. Launch Herbarium from your app menu."
say "(AppImages need FUSE; if it won't start, install libfuse2 or run it with --appimage-extract-and-run.)"
ABS_BIN="$BIN_DIR/herbarium"
case "$ABS_BIN" in
  /*) ;;
  *) ABS_BIN="$(cd "$BIN_DIR" 2>/dev/null && pwd)/herbarium" ;;
esac
mcp_hint "$ABS_BIN"
