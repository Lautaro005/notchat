#!/bin/sh
# Notchat installer — https://github.com/Lautaro005/notchat
#
#   curl -fsSL https://raw.githubusercontent.com/Lautaro005/notchat/main/install.sh | sh
#
# Downloads the latest release, verifies its SHA-256 against the published
# checksums, and installs Notchat.app into /Applications (or ~/Applications).
# Options (pass after `sh -s --`):
#   --version vX.Y.Z   install a specific release
#   --dir PATH         install into PATH instead of /Applications
#   --source           build from source instead (needs Rust and Node.js)
set -eu

REPO="Lautaro005/notchat"
APP="Notchat.app"
VERSION=""
DEST=""
FROM_SOURCE=0

say() { printf '\033[1m%s\033[0m\n' "$*"; }
fail() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:-}"; shift 2 ;;
    --dir) DEST="${2:-}"; shift 2 ;;
    --source) FROM_SOURCE=1; shift ;;
    -h|--help) sed -n '2,13p' "$0" 2>/dev/null || true; exit 0 ;;
    *) fail "unknown option: $1" ;;
  esac
done

[ "$(uname -s)" = "Darwin" ] || fail "Notchat runs on macOS only."
major="$(sw_vers -productVersion | cut -d. -f1)"
[ "$major" -ge 13 ] || fail "Notchat needs macOS 13 or later."

if [ -z "$DEST" ]; then
  if [ -w /Applications ]; then DEST="/Applications"; else DEST="$HOME/Applications"; fi
fi
mkdir -p "$DEST"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

if [ "$FROM_SOURCE" -eq 1 ]; then
  command -v cargo >/dev/null || fail "Rust is required: https://rustup.rs"
  command -v npm >/dev/null || fail "Node.js is required: https://nodejs.org"
  command -v git >/dev/null || fail "git is required (xcode-select --install)"
  say "Building Notchat from source…"
  git clone --depth 1 ${VERSION:+--branch "$VERSION"} "https://github.com/$REPO.git" "$TMP/src"
  (cd "$TMP/src" && npm --prefix ui ci && npm run build)
  BUILT="$(find "$TMP/src/target" -name "$APP" -type d -path '*bundle/macos*' | head -n 1)"
  [ -n "$BUILT" ] || fail "build finished without an app bundle"
  rm -rf "$DEST/$APP"
  cp -R "$BUILT" "$DEST/"
else
  if [ -z "$VERSION" ]; then
    VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
      | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)"
    [ -n "$VERSION" ] || fail "could not find the latest release"
  fi
  ASSET="Notchat_${VERSION#v}_universal.app.tar.gz"
  BASE="https://github.com/$REPO/releases/download/$VERSION"

  say "Downloading Notchat ${VERSION}…"
  curl -fL --progress-bar -o "$TMP/$ASSET" "$BASE/$ASSET"
  curl -fsSL -o "$TMP/SHA256SUMS" "$BASE/SHA256SUMS"

  say "Verifying checksum…"
  expected="$(grep " $ASSET\$" "$TMP/SHA256SUMS" | cut -d' ' -f1)"
  [ -n "$expected" ] || fail "no checksum published for $ASSET"
  actual="$(shasum -a 256 "$TMP/$ASSET" | cut -d' ' -f1)"
  [ "$expected" = "$actual" ] || fail "checksum mismatch — the download was discarded"

  tar -xzf "$TMP/$ASSET" -C "$TMP"
  [ -d "$TMP/$APP" ] || fail "archive does not contain $APP"
  if pgrep -xq Notchat; then osascript -e 'quit app "Notchat"' >/dev/null 2>&1 || true; sleep 1; fi
  rm -rf "$DEST/$APP"
  mv "$TMP/$APP" "$DEST/"
fi

# Files fetched with curl carry no quarantine flag; clear it in case a
# browser-downloaded copy was replaced.
xattr -dr com.apple.quarantine "$DEST/$APP" 2>/dev/null || true

say "Notchat installed in $DEST/$APP"
open "$DEST/$APP"
