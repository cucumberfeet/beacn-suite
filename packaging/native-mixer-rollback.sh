#!/usr/bin/env bash
# Undo native-mixer-install.sh: puts back the previous beacn-utility and launcher from the
# last backup (or the backup directory given) and restarts beacn.service. No sudo.
# Audio stops for a few seconds while the service restarts.
set -euo pipefail
BK="${1:-$(cat "$HOME/.beacn-native-mixer-last-backup" 2>/dev/null || true)}"
[ -n "$BK" ] && [ -f "$BK/beacn-utility" ] || { echo "No native-mixer backup found (${BK:-none})"; exit 1; }
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
DESKTOP="io.github.beacn_on_linux.beacn-utility.desktop"

echo "Restoring from $BK"
cp -a "$BK/beacn-utility" "$BIN/.beacn-utility.old" && mv -f "$BIN/.beacn-utility.old" "$BIN/beacn-utility"
[ -f "$BK/$DESKTOP" ] && cp -a "$BK/$DESKTOP" "$APPS/$DESKTOP"
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
systemctl --user restart beacn.service
sleep 3
systemctl --user --no-pager status beacn.service | head -5 || true
echo "Rolled back to the previous beacn-utility (old Beacn Mixer window)."
