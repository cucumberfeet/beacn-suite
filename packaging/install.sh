#!/usr/bin/env bash
# Beacn suite install (tested on Fedora 44 KDE Plasma). Run as your normal user, NO sudo.
# Usage: ./install.sh            (from dist/beacn-suite, made by packaging/build.sh)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
F="$HERE/files"
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
AUTO="$HOME/.config/autostart"
UNITS="$HOME/.config/systemd/user"
ICONS="$HOME/.local/share/icons/hicolor/256x256/apps"
BK="$HOME/beacn-backup-$(date +%Y%m%d-%H%M%S)"

( cd "$F" && sha256sum -c --quiet "$HERE/SHA256SUMS" ) || { echo "checksum mismatch, stopping"; exit 1; }
[ -s "${XDG_CONFIG_HOME:-$HOME/.config}/beacn-utility/official-artwork" ] || echo "NOTE: no official artwork configured; the stock screen is used until you run tools/setup-official-artwork.sh"

echo "== Backing up to $BK"
mkdir -p "$BK"/{bin,applications,autostart,units,config}
for f in beacn-utility pipeweaver-daemon pipeweaver-client pipeweaver-app; do
  [ -e "$BIN/$f" ] || [ -L "$BIN/$f" ] && cp -a "$BIN/$f" "$BK/bin/" || true
done
cp -a "$APPS"/*pipeweaver*.desktop "$APPS"/*beacn*.desktop "$BK/applications/" 2>/dev/null || true
cp -a "$AUTO"/*pipeweaver*.desktop "$AUTO"/*beacn*.desktop "$BK/autostart/" 2>/dev/null || true
cp -a "$UNITS"/pipeweaver.service "$UNITS"/beacn.service "$BK/units/" 2>/dev/null || true
cp -a "$HOME/.config/beacn-utility" "$HOME/.config/pipeweaver" "$BK/config/" 2>/dev/null || true
systemctl --user is-enabled pipeweaver.service > "$BK/pipeweaver.enabled" 2>/dev/null || true
echo "$BK" > "$HOME/.beacn-last-backup"

echo "== Stopping the old pieces"
systemctl --user stop pipeweaver.service 2>/dev/null || true
pkill -x beacn-utility 2>/dev/null || true
pkill -f "$BIN/pipeweaver-app" 2>/dev/null || true
pkill -x pipeweaver-daem 2>/dev/null || pkill -f "$BIN/pipeweaver-daemon" 2>/dev/null || true
sleep 2

echo "== Installing"
mkdir -p "$BIN" "$APPS" "$UNITS" "$ICONS"
# Replace symlinks/files atomically (an old symlink into target/release must not be written through)
for f in beacn-utility pipeweaver-daemon pipeweaver-client; do
  install -m755 "$F/$f" "$BIN/.$f.new" && mv -f "$BIN/.$f.new" "$BIN/$f"
done
install -m755 "$F/beacn-mixer" "$BIN/.pipeweaver-app.new" && mv -f "$BIN/.pipeweaver-app.new" "$BIN/pipeweaver-app"
install -m644 "$F/io.github.beacn_on_linux.beacn-utility.png" "$ICONS/"
install -m644 "$F/io.github.beacn_on_linux.beacn-utility.desktop" "$APPS/"
install -m644 "$F/beacn.service" "$UNITS/"

# One launcher: hide the old Pipeweaver / duplicate entries from the menu (kept, just hidden)
for d in "$APPS"/*pipeweaver*.desktop "$APPS"/beacn-utility.desktop; do
  [ -f "$d" ] || continue
  grep -q '^NoDisplay=true' "$d" || sed -i '/^\[Desktop Entry\]/a NoDisplay=true' "$d"
done
# One service: no separate autostarts
for d in "$AUTO"/*pipeweaver*.desktop "$AUTO"/*beacn*.desktop; do
  [ -f "$d" ] && rm -f "$d"
done
systemctl --user disable pipeweaver.service 2>/dev/null || true

systemctl --user daemon-reload
systemctl --user enable --now beacn.service
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true

sleep 4
echo "== Status"
systemctl --user --no-pager status beacn.service | head -15 || true
pgrep -a pipeweaver-dae || echo "pipeweaver-daemon NOT running (check: journalctl --user -u beacn -n 50)"
echo "Backup is in $BK  (undo: ./rollback.sh)"
