#!/usr/bin/env bash
# Headless test bed for the native Mixer page (container only, never on a real desktop):
# a private PipeWire + WirePlumber with two null devices, pipeweaver-daemon, a demo profile,
# two playing "apps", and Xvfb. Source this from the repo root inside contrib/Containerfile.test.
#   PW_DAEMON=/path/to/pipeweaver-daemon  (default: pipeweaver-daemon on PATH)
set -u
T="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WS="python3 $T/pw-ws.py"
dbus-uuidgen > /etc/machine-id 2>/dev/null || true
export XDG_RUNTIME_DIR=/tmp/rt HOME=/tmp/home; mkdir -p -m700 $XDG_RUNTIME_DIR; mkdir -p $HOME
eval "$(dbus-launch --sh-syntax)"
pipewire >/tmp/pw.log 2>&1 & sleep 1
wireplumber >/tmp/wp.log 2>&1 & sleep 1
pipewire-pulse >/tmp/pp.log 2>&1 & sleep 1
pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=test_speakers node.description="Studio Speakers" media.class=Audio/Sink object.linger=true audio.position=[FL FR] }' >/dev/null
pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=test_mic node.description="USB Microphone" media.class=Audio/Source/Virtual object.linger=true audio.position=[FL FR] }' >/dev/null
sleep 1
"${PW_DAEMON:-pipeweaver-daemon}" --background --no-tray >/tmp/pwd.log 2>&1 &
for i in $(seq 1 40); do (exec 3<>/dev/tcp/127.0.0.1/14565) 2>/dev/null && break; sleep 0.5; done
sleep 4

# --- Demo profile (all through the same API the mixer uses) ---
st() { $WS '"GetStatus"'; }
id_of() { st | python3 -c "
import json,sys; d=json.load(sys.stdin)['audio']['profile']['devices']
for g in ('sources','targets'):
  for k in ('physical_devices','virtual_devices'):
    for x in d[g][k]:
      if x['description']['name']=='$1': print(x['description']['id'])"; }
dev_node() { st | python3 -c "
import json,sys; d=json.load(sys.stdin)['audio']['devices']
for k in ('Source','Target'):
  for x in d[k]:
    if x['name']=='$1': print(x['node_id'], x['id'])"; }
pw() { $WS "{\"Pipewire\": $1}" >/dev/null; }
pw '{"CreateNode": ["VirtualSource", "Game"]}'
pw '{"CreateNode": ["VirtualSource", "Music"]}'
pw '{"CreateNode": ["VirtualTarget", "Audience"]}'
sleep 2
MIC=$(id_of Microphone); SYS=$(id_of System); BRO=$(id_of Browser); GAME=$(id_of Game); MUS=$(id_of Music)
HP=$(id_of Headphones); CHAT=$(id_of "Chat Mic"); AUD=$(id_of Audience)
read SPK_NODE SPK_ID < <(dev_node test_speakers); read MIC_NODE MIC_ID < <(dev_node test_mic)
pw "{\"AttachPhysicalNode\": [\"$HP\", $SPK_NODE]}"
pw "{\"AttachPhysicalNode\": [\"$MIC\", $MIC_NODE]}"
pw "{\"SetPhysicalDeviceVolume\": [\"$SPK_ID\", 80]}"
pw "{\"SetPhysicalDeviceVolume\": [\"$MIC_ID\", 65]}"
pw "{\"SetTargetMix\": [\"$AUD\", \"B\"]}"
pw "{\"SetNodeColour\": [\"$GAME\", {\"red\": 108, \"green\": 192, \"blue\": 74}]}"
pw "{\"SetNodeColour\": [\"$MUS\", {\"red\": 155, \"green\": 89, \"blue\": 208}]}"
pw "{\"SetNodeColour\": [\"$AUD\", {\"red\": 244, \"green\": 124, \"blue\": 36}]}"
for s in $MIC $SYS $BRO $GAME $MUS; do pw "{\"SetRoute\": [\"$s\", \"$HP\", true]}"; pw "{\"SetRoute\": [\"$s\", \"$AUD\", true]}"; done
pw "{\"SetRoute\": [\"$MIC\", \"$HP\", false]}"
pw "{\"SetSourceVolumeLinked\": [\"$GAME\", false]}"
pw "{\"SetSourceVolume\": [\"$GAME\", \"A\", 72]}"
pw "{\"SetSourceVolume\": [\"$GAME\", \"B\", 45]}"
pw "{\"SetSourceVolume\": [\"$MUS\", \"A\", 38]}"
pw "{\"SetSourceVolume\": [\"$BRO\", \"A\", 85]}"
pw "{\"SetTargetVolume\": [\"$CHAT\", 90]}"
# Game: Mute B only to the Audience output (AUDIENCE); Music: Mute B to all (MUTE ALL), active
pw "{\"AddMuteTargetNode\": [\"$GAME\", \"TargetB\", \"$AUD\"]}"
pw "{\"AddSourceMuteTarget\": [\"$MUS\", \"TargetB\"]}"
pw "{\"AddFilterToNode\": [\"$MIC\", {\"filter\": {\"PWV\": {\"plugin_type\": \"Gain\", \"values\": {}}}}]}"
pw "{\"AddFilterToNode\": [\"$MIC\", {\"filter\": {\"PWV\": {\"plugin_type\": \"Bleep\", \"values\": {}}}}]}"
export MIC SYS BRO GAME MUS HP CHAT AUD

# --- "Apps" playing tones (meters and the Applications tab) ---
# Each file lasts a minute so the apps stay listed instead of blinking between loops.
python3 - <<'PY'
import math, struct, wave, random
for name, f in (("/tmp/tone-a.wav", 220.0), ("/tmp/tone-b.wav", 330.0)):
    w = wave.open(name, "wb"); w.setnchannels(2); w.setsampwidth(2); w.setframerate(48000)
    frames = bytearray()
    for i in range(48000 * 4):
        env = 0.5 + 0.5 * math.sin(2 * math.pi * 0.7 * i / 48000)
        v = int(9000 * env * math.sin(2 * math.pi * f * i / 48000) + random.randint(-600, 600))
        frames += struct.pack("<hh", v, v)
    w.writeframes(bytes(frames) * 15); w.close()
PY
( while true; do pw-play -P '{ application.name="Firefox" media.name="YouTube - Lofi" }' --target pipeweaver_browser /tmp/tone-a.wav; done ) >/dev/null 2>&1 &
( while true; do pw-play -P '{ application.name="Steam Game" media.name="Game audio" }' --target pipeweaver_game /tmp/tone-b.wav; done ) >/dev/null 2>&1 &
( while true; do pw-play -P '{ application.name="Spotify" media.name="Spotify" }' --target pipeweaver_music /tmp/tone-a.wav; done ) >/dev/null 2>&1 &
sleep 2

Xvfb :9 -screen 0 1400x860x24 >/dev/null 2>&1 & export DISPLAY=:9; sleep 1
export WINIT_UNIX_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1
