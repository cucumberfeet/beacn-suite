#!/usr/bin/env bash
# Build the official-look screen kit from YOUR OWN BEACN Windows installer and point
# beacn-utility at it. Nothing is downloaded and nothing is redistributed. No sudo.
#
# Usage: tools/setup-official-artwork.sh "/path/to/BEACN Setup V1.4.2.12.exe" [DEST]
#   DEST defaults to ~/.local/share/beacn-official
# Needs: python3, python3-pillow, rsvg-convert (librsvg2-tools), and 7z (7zip) or py7zr.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
SETUP="${1:?usage: $0 /path/to/BEACN-Setup.exe [DEST]}"
DEST="${2:-${XDG_DATA_HOME:-$HOME/.local/share}/beacn-official}"
CONF="${XDG_CONFIG_HOME:-$HOME/.config}/beacn-utility"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT

echo "== Extracting BEACN.exe and the default theme pack"
python3 "$HERE/extract-beacn-installer.py" "$SETUP" "$WORK" default
mkdir -p "$DEST/MixImages"
mv -f "$WORK/MixImages/default.images" "$DEST/MixImages/default.images"

echo "== Building the screen kit"
python3 "$HERE/make-official-kit.py" "$WORK/BEACN.exe" "$DEST/MixImages/default.images" "$DEST/screen-default"

echo "== Pointing beacn-utility at it"
mkdir -p "$CONF"
printf '%s\n' "$DEST/MixImages/default.images" > "$CONF/official-artwork"
printf '%s\n' "$DEST/screen-default" > "$CONF/official-screen"
echo "Done. Kit: $DEST/screen-default  (restart beacn-utility / beacn.service to pick it up)"
