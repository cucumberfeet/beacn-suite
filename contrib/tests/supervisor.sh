#!/usr/bin/env bash
# Container smoke test (run from the repo root inside contrib/Containerfile + Xvfb, ImageMagick, xdotool, dbus-x11).
set -u
dnf -yq --disablerepo=fedora-cisco-openh264 install procps-ng libxkbcommon-x11 mesa-vulkan-drivers mesa-dri-drivers >/dev/null 2>&1
B="${B:-dist/beacn-suite/files}"
$B/pipeweaver-daemon --help | grep -A1 no-tray
mkdir -p /tmp/bin; cat > /tmp/bin/pipeweaver-daemon <<'X'
#!/bin/bash
echo "fake daemon pid $$ args: $* NO_TRAY=$PIPEWEAVER_NO_TRAY" >> /tmp/fake.log
trap 'echo "fake got TERM" >> /tmp/fake.log; exit 0' TERM
while true; do sleep 1; done
X
chmod +x /tmp/bin/pipeweaver-daemon
export XDG_RUNTIME_DIR=/tmp/rt; mkdir -p -m700 $XDG_RUNTIME_DIR
dbus-uuidgen > /etc/machine-id; Xvfb :9 -screen 0 1600x1000x24 >/dev/null 2>&1 & export DISPLAY=:9; sleep 1; eval $(dbus-launch --sh-syntax)
BEACN_PIPEWEAVER_DAEMON=/tmp/bin/pipeweaver-daemon $B/beacn-utility --background --with-pipeweaver > /tmp/u.log 2>&1 &
U=$!; sleep 4
echo "--- running:"; pgrep -af fake; pgrep -af 'bin/pipeweaver-daemon'
pkill -f /tmp/bin/pipeweaver-daemon; sleep 3   # crash -> restart
echo "--- after child killed:"; pgrep -af 'bin/pipeweaver-daemon'
kill -TERM $U; sleep 8
echo "--- after utility TERM:"; pgrep -af 'bin/pipeweaver-daemon' || echo "no daemon left (good)"
kill -0 $U 2>/dev/null && echo "utility still alive" || echo "utility exited"
cat /tmp/fake.log; grep -iE 'pipeweaver|supervis' /tmp/u.log | head
echo ---; grep -iE "panic|error|warn" /tmp/u.log | head; tail -5 /tmp/u.log
