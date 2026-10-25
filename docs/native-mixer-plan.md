# Native mixer page: plan and feature inventory

Goal: one window. The Pipeweaver web mixer (shown today by `beacn-mixer`, a GTK/WebKit
wrapper) is rebuilt as a native **Mixer** page inside beacn-utility's own iced window. Wayland
can't embed another app's window, so the page is a reimplementation that talks to the
pipeweaver daemon over the same websocket API the web mixer uses.

The old `beacn-mixer` window stays available as a fallback (see "Fallback" below) until the
native page has been tested and approved.

Source of truth for the inventory: Pipeweaver at the pinned commit `006b045`
(`web/src/**`, `ipc/src/commands/mod.rs`, `profile/src/lib.rs`, `shared/src/lib.rs`,
`daemon/src/servers/http_server.rs`). Nothing here is contributed upstream.

## Daemon API used

| What | How |
|---|---|
| Control socket | `ws://localhost:14565/api/websocket`. Requests are `{"id": n, "data": <DaemonRequest>}`, replies are `{"id": n, "data": <DaemonResponse>}`. |
| Initial state | `"GetStatus"` returns `{"Status": DaemonStatus}` (`config` + `audio`). |
| Live updates | The daemon pushes `{"Patch": [RFC 6902 ops]}` to every client after any change, whoever made it (web UI, Mix Create dials and buttons, pipeweaver-client, this page). The page keeps the raw status JSON, applies each patch, and re-derives its typed model, so changes from the hardware show up live. |
| Mixer commands | `{"Pipewire": <APICommand>}` returns `"Ok"`/`{"Pipewire":"Ok"}`, `{"Pipewire":{"Id":ulid}}` or `{"Pipewire":{"Err":msg}}`. |
| Daemon commands | `{"Daemon": <DaemonCommand>}` (`SetAudioQuantum`, `SetAutoStart`, `SetUseBrowser`, `OpenInterface`, `ResetAudio`, `SetMetering`). |
| Meters | `ws://localhost:14565/api/websocket/meter` pushes `{"id": node, "percent": 0-100}`. The daemon starts metering when the first meter client connects and stops when the last one leaves, so the page only holds this socket open while the Mixer page is on screen. |

## Feature inventory (web mixer -> native page)

Status: **done** = implemented natively in this branch, **gap** = not native yet (reachable
through "Open web mixer" in the page's Settings tab while the fallback exists).

### Channel strips (Sources and Targets sections)
| Feature | Web UI | Command(s) | Native |
|---|---|---|---|
| Sources / Targets sections, pinned group first, then default order | `Mixer.vue`, `DeviceList.vue` | `device_order` in profile | done |
| Channel name, colour band, colour-tinted header | `ChannelColumn.vue` | - | done |
| Source volume for Mix A and Mix B (two faders, %) | `ChannelColumnVolume.vue` | `SetSourceVolume(id, mix, v)` | done |
| Link Mix A/B volumes | link icon | `SetSourceVolumeLinked(id, bool)` | done |
| Target volume (one fader) | | `SetTargetVolume(id, v)` | done |
| Target's mix: A or B (fader colour follows) | `MixAssignment.vue` | `SetTargetMix(id, mix)` | done |
| Live level meters on faders | `meter_scheduler.js` | meter socket | done |
| Source Mute A / Mute B buttons | | `AddSourceMuteTarget` / `DelSourceMuteTarget` | done |
| Mute target selector: "Mute to All" (empty list) or a set of targets | `MuteTargetSelector.vue` | `ClearMuteTargetNodes`, `AddMuteTargetNode`, `DelMuteTargetNode` | done. An empty Mute B list still means **MUTE ALL**; the button reads "Mute All" and a set list reads "Mute: Audience" etc. |
| Target mute | | `SetTargetMuteState(id, Muted/Unmuted)` | done |
| Warning when a physical channel has no device attached | glowing menu button | - | done |
| Channel menu: attach / detach physical devices (shows missing ones from the profile) | `DevicePopup.vue` | `AttachPhysicalNode(id, node_id)`, `RemovePhysicalNode(id, index)` | done |
| Rename (20 chars, letters/numbers/space/-/_, unique) | modal | `RenameNode(id, name)` | done (same validation) |
| Colour | colour picker | `SetNodeColour(id, {red,green,blue})` | done (debounced) |
| Pin / unpin, hide, show hidden (eye button) | | `SetOrderGroup(id, Default/Pinned/Hidden)` | done |
| Reorder | drag and drop | `SetOrder(id, index)` | done as Move left / Move right in the channel menu (no drag and drop) |
| Remove channel (confirm) | modal | `RemoveNode(id)` | done |
| Add virtual / physical source or target | "+" menu + name modal | `CreateNode(type, name)` | done |
| Filters (effects) per channel: list, add, remove, reorder, enable, edit parameters | `FilterView.vue` + 25 bespoke filter panels | `AddFilterToNode`, `RemoveFilter`, `MoveFilter`, `SetFilterValue` | done with a **generic** parameter editor (sliders, toggles, choice lists from the filter's own parameter metadata). **gap:** the bespoke graphical panels (EQ curve, compressor graphs, multiband views). |

### Routing tab
| Feature | Command | Native |
|---|---|---|
| Matrix of sources (columns) x targets (rows), hidden channels included, coloured by channel, check colour by the target's mix | `SetRoute(src, tgt, bool)` | done |

### Applications tab
| Feature | Command | Native |
|---|---|---|
| Default output and input device (physical devices plus Pipeweaver virtual channels) | `SetDefaultOutput(id)`, `SetDefaultInput(id)` | done |
| Playback apps (Output) and recording apps (Input), grouped by process | `applications` in status | done |
| Per app: remembered route (Default or a virtual channel) | `SetApplicationRoute(def, id)`, `ClearApplicationRoute(def)` | done |
| Per stream (node): temporary route, volume, mute | `SetTransientApplicationRoute`, `ClearTransientApplicationRoute`, `SetApplicationVolume`, `SetApplicationMute` | done |
| Apps that closed stay listed for 10 s with a countdown | client-side cache | gap (closed apps disappear at once; remembered routes still show under "Remembered") |

### Devices tab
| Feature | Command | Native |
|---|---|---|
| Physical output and input devices with volume and mute | `SetPhysicalDeviceVolume`, `SetPhysicalDeviceMute` | done |

### Settings tab
| Feature | Command | Native |
|---|---|---|
| Audio buffer (quantum): PipeWire default or 8 to 4096 | `SetAudioQuantum(Option<Quantum>)` | done |
| Restart audio engine (confirm) | `ResetAudio` | done |
| Pipeweaver auto start | `SetAutoStart` | not shown on purpose: `beacn.service` starts the daemon |
| "Use browser instead of app" | `SetUseBrowser` | not shown: only affects the fallback window |
| Open the web mixer (fallback window) | `OpenInterface` | done (button) |

### Not in scope
- Mobile portrait/landscape layouts of the web UI.
- Port maps for multi-channel devices (`CreatePhysicalNodePortMap` etc.): the web UI has no
  screen for them either at `006b045`.

## Where it lives
- `src/ui/pages/app/mixer/` - the page:
  - `client.rs` websocket worker (iced subscription): connect, GetStatus, patches, replies,
    reconnect every second while the daemon is down; separate meter subscription.
  - `model.rs` typed view of the status JSON (lenient serde, ids as strings).
  - `mod.rs` page state, messages and update logic (commands, optimistic fader values while
    dragging, debounced colour changes, error toasts).
  - `view.rs`, `strip.rs`, `routing.rs`, `apps.rs`, `devices.rs`, `settings.rs`, `filters.rs`,
    `style.rs` - the UI.
- Sidebar: the top button opens the Mixer page (a neutral sliders icon, no third-party logo).
- Tray "Mixer", the launcher's "Mixer" action (`beacn-utility --mixer`) and a second launch
  with `--mixer` all open the device window on the Mixer page.

## Fallback
The old window is still built and installed as `~/.local/bin/pipeweaver-app`. To use it instead
of the native page, either set `BEACN_LEGACY_MIXER=1` in the service environment or create
`~/.config/beacn-utility/legacy-mixer` (any content), then `systemctl --user restart beacn`.
With the fallback on, the sidebar button, tray "Mixer" and `--mixer` open the old window exactly
as before. The native page also offers "Open web mixer" in its Settings tab for the gaps above.

## Look
Same palette as beacn-utility (`#1B1B1B` background, `#3C3C3C` controls, `#005C80` accent)
and the mixer colours already used on the Mix Create screen: Mix A cyan `#59B1B6`, Mix B
orange `#F47C24`, mute red. Channel headers carry the channel colour. No third-party branding.

## Testing

Headless test bed (box/CI container, never a real desktop): `contrib/tests/native-mixer-env.sh`
starts PipeWire, WirePlumber, two null devices, pipeweaver-daemon with a demo profile,
three looping "apps" and Xvfb. `contrib/tests/pw-ws.py` sends API commands, which is how a
dial or button on the Mix Create reaches the daemon, so it doubles as a hardware simulator.

Checked on the box (Oct 20, 2026, Fedora 44 container, debug build):

- every tab renders; channel, mute-target, add and hidden-channel menus anchor correctly
  (and flip above the trigger near the bottom edge);
- UI -> daemon: fader drag (linked B follows), Mute A, "All outputs (Mute All)" clears the
  Mute B target list, route grid toggle, app route, effect enable toggle, colour preset,
  rename (with validation), create, hide/unhide;
- daemon -> UI without reload: volume, Mute B and linked volume changes sent from another
  client appear at once;
- daemon stopped -> "Starting the audio mixer…" screen; daemon back -> reconnects by itself.

Not verifiable headless: physical device volume (null sinks report 0 from the daemon, so
the Devices sliders read 0 there), real Mix Create input, Wayland/KDE window behaviour and
meter CPU cost on real hardware.
