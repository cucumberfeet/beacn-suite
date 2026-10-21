#!/usr/bin/env bash
# Undo install.sh: restores the exact files from the last backup. No sudo.
set -euo pipefail
BK="${1:-$(cat "$HOME/.beacn-last-backup")}"
[ -d "$BK" ] || { echo "no backup dir $BK"; exit 1; }
BIN="$HOME/.local/bin"; APPS="$HOME/.local/share/applications"; AUTO="$HOME/.config/autostart"; UNITS="$HOME/.config/systemd/user"
echo "Restoring from $BK"
systemctl --user disable --now beacn.service 2>/dev/null || true
pkill -x beacn-utility 2>/dev/null || true
pkill -f "$BIN/pipeweaver-daemon" 2>/dev/null || true
sleep 2
rm -f "$UNITS/beacn.service" "$APPS/io.github.beacn_on_linux.beacn-utility.desktop"
for f in "$BK"/bin/*; do [ -e "$f" ] || [ -L "$f" ] || continue; rm -f "$BIN/$(basename "$f")"; cp -a "$f" "$BIN/"; done
cp -a "$BK"/applications/. "$APPS/" 2>/dev/null || true
mkdir -p "$AUTO"; cp -a "$BK"/autostart/. "$AUTO/" 2>/dev/null || true
cp -a "$BK"/units/. "$UNITS/" 2>/dev/null || true
systemctl --user daemon-reload
if grep -q enabled "$BK/pipeweaver.enabled" 2>/dev/null; then systemctl --user enable --now pipeweaver.service; fi
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
# Old beacn-utility was started by autostart: start it now too
( setsid -f "$BIN/beacn-utility" --background >/dev/null 2>&1 ) || true
echo "Rolled back. (Any official-artwork kit you generated is left in place.)"
