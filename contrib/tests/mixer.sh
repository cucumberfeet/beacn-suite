#!/usr/bin/env bash
# Container smoke test (run from the repo root inside contrib/Containerfile + Xvfb, ImageMagick, xdotool, dbus-x11).
set -u
B="${B:-dist/beacn-suite/files}"; O="${O:-/tmp}"
dbus-uuidgen > /etc/machine-id
Xvfb :9 -screen 0 1400x860x24 >/dev/null 2>&1 & export DISPLAY=:9; sleep 1
eval $(dbus-launch --sh-syntax)
export XDG_RUNTIME_DIR=/tmp/rt; mkdir -p -m700 $XDG_RUNTIME_DIR
export WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1
$B/beacn-mixer > /tmp/m.log 2>&1 &
sleep 8; import -window root $O/mixer-waiting.png
# Now bring the "daemon" up: serve the built Pipeweaver UI on 14565 (no backend, so it shows its own disconnected state)
cd build/pipeweaver/daemon/web-content && python3 -m http.server 14565 >/dev/null 2>&1 &
sleep 6; import -window root $O/mixer-loaded.png
# second launch should not open a second window
$B/beacn-mixer; echo "second launch exit: $?"
sleep 2; xdotool search --name "Beacn Mixer" | wc -l | sed 's/^/windows titled Beacn Mixer: /'
xprop -id $(xdotool search --name "Beacn Mixer" | head -1) WM_CLASS _GTK_APPLICATION_ID 2>/dev/null
cat /tmp/m.log | head -20
