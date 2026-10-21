#!/usr/bin/env bash
# Upgrade an existing beacn-suite install (made by install.sh) to the native Mixer page.
# Only beacn-utility and the launcher change; Pipeweaver, beacn-mixer (the old window, kept as
# the fallback) and beacn.service stay as they are. Run as your normal user, NO sudo.
#
# Usage: packaging/native-mixer-install.sh [path/to/beacn-utility] [--yes]
#   The binary defaults to the bundle's files/beacn-utility (packaging/build.sh), then
#   target/release/beacn-utility. Undo with packaging/native-mixer-rollback.sh.
#
# Restarting beacn.service also restarts pipeweaver-daemon, so PC audio drops for a few
# seconds. The script asks first unless --yes is given.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
DESKTOP="io.github.beacn_on_linux.beacn-utility.desktop"
BK="$HOME/beacn-native-mixer-backup-$(date +%Y%m%d-%H%M%S)"

NEW="" YES=0
for a in "$@"; do
  case "$a" in
    --yes|-y) YES=1 ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) NEW="$a" ;;
  esac
done
if [ -z "$NEW" ]; then
  for c in "$ROOT/files/beacn-utility" "$ROOT/dist/beacn-suite/files/beacn-utility" \
           "$ROOT/target/release/beacn-utility"; do
    [ -x "$c" ] && { NEW="$c"; break; }
  done
fi
[ -n "$NEW" ] && [ -x "$NEW" ] || { echo "No beacn-utility binary found; build it first (packaging/build.sh)"; exit 1; }
head -c4 "$NEW" | grep -q 'ELF' || { echo "$NEW is not an executable"; exit 1; }
[ -e "$BIN/beacn-utility" ] || { echo "No existing install in $BIN; run packaging/install.sh instead"; exit 1; }

echo "Installing $NEW"
echo "  (sha256 $(sha256sum "$NEW" | cut -c1-16)…)"
if [ "$YES" != 1 ]; then
  read -r -p "This restarts beacn.service, so audio stops for a few seconds. Continue? [y/N] " ok
  [[ "$ok" =~ ^[Yy] ]] || { echo "Nothing changed."; exit 0; }
fi

echo "== Backing up to $BK"
mkdir -p "$BK"
cp -a "$BIN/beacn-utility" "$BK/"
[ -f "$APPS/$DESKTOP" ] && cp -a "$APPS/$DESKTOP" "$BK/"
echo "$BK" > "$HOME/.beacn-native-mixer-last-backup"

echo "== Installing"
install -m755 "$NEW" "$BIN/.beacn-utility.new" && mv -f "$BIN/.beacn-utility.new" "$BIN/beacn-utility"
install -m644 "$ROOT/packaging/$DESKTOP" "$APPS/.$DESKTOP.new" && mv -f "$APPS/.$DESKTOP.new" "$APPS/$DESKTOP"
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true

echo "== Restarting beacn.service"
systemctl --user restart beacn.service
sleep 4
systemctl --user --no-pager status beacn.service | head -8 || true
pgrep -a pipeweaver-dae >/dev/null && echo "pipeweaver-daemon is running" \
  || echo "pipeweaver-daemon NOT running (journalctl --user -u beacn -n 50)"

cat <<MSG

Done. Open the mixer from the tray (Beacn > Mixer) or the launcher's "Mixer" action.
Old window instead:  touch ~/.config/beacn-utility/legacy-mixer && systemctl --user restart beacn
Undo everything:     $ROOT/packaging/native-mixer-rollback.sh
Backup:              $BK
MSG
