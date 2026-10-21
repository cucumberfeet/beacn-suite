# beacn-suite

An official-look Mix Create screen and a single, merged **Beacn** app for Linux:
[beacn-utility](https://github.com/beacn-on-linux/beacn-utility) (device control) and
[Pipeweaver](https://github.com/pipeweaver/pipeweaver) (audio routing/mixer) running as one
service with one tray icon and a native mixer window.

> Not affiliated with, endorsed by, or supported by BEACN. "BEACN" and its artwork belong to
> BEACN. This repository contains **no BEACN artwork, firmware, or installer files**; the
> optional official look is generated on your own machine from your own copy of the BEACN
> Windows installer.

This repository is a fork of beacn-utility (full upstream history preserved) with extra
pieces in subfolders. The upstream README is kept as [UPSTREAM_README.md](UPSTREAM_README.md).

## What it does

- **Official-look Mix / Mix Create screen** (`src/integrations/pipeweaver/official*.rs`):
  header bar and background, four channel cards with names and colour bands, the devices/apps
  attached to each channel, volume dial tiles that follow the dials, AUDIENCE / MUTE ALL
  buttons, the Mix A / Mix B toggle, page dots for more than four channels, and the boot logo.
  Uses the artwork kit you generate locally (see below); without a kit, the stock
  beacn-utility screen is used.
- **One app instead of two:**
  - **One service:** `beacn.service` runs `beacn-utility --background --with-pipeweaver`.
    beacn-utility starts `pipeweaver-daemon --background --no-tray` as a supervised child,
    restarts it with backoff if it crashes, and stops it on exit (SIGTERM + PDEATHSIG). An
    already running Pipeweaver daemon is left alone.
  - **One tray icon:** "Beacn" with Devices, Mixer, Open Logs and Quit. Pipeweaver's own tray
    is disabled by a small patch (`patches/pipeweaver/`).
  - **Native mixer window** (`beacn-mixer/`): a GTK 3 + WebKitGTK window titled "Beacn Mixer"
    that shows Pipeweaver's web UI without a browser. Single instance, remembers its size,
    opens external links in your browser, waits and retries while the daemon starts.
    Right-click or Ctrl+D opens the device window; Ctrl+R / F5 reloads.
  - **One launcher and app identity:** both windows use the app id
    `io.github.beacn_on_linux.beacn-utility`, so Plasma groups them under one taskbar icon.
    The launcher's "Mixer" action opens the mixer.

## Requirements

- Tested on **Fedora 44 KDE Plasma 6 (Wayland)**, x86_64, with a BEACN Mix Create and
  BEACN Studio. Other distributions and desktops should work but are untested.
- PipeWire + WirePlumber.
- beacn-utility's udev rule for device access (one-time, needs root):
  `sudo install -m644 50-beacn.rules /etc/udev/rules.d/ && sudo udevadm control --reload`
- Build dependencies (Fedora names):
  ```
  sudo dnf install cargo rust gcc gcc-c++ pkgconf-pkg-config git nodejs-npm \
    libxkbcommon-devel wayland-devel libX11-devel fontconfig-devel freetype-devel dbus-devel \
    systemd-devel pipewire-devel clang-devel lilv-devel alsa-lib-devel openssl-devel \
    webkit2gtk4.1-devel gtk3-devel libsoup3-devel libayatana-appindicator-gtk3-devel
  ```
  Or build in a clean container with `contrib/Containerfile` (no host packages needed).
- For the official artwork kit: `python3-pillow`, `librsvg2-tools` (`rsvg-convert`) and
  `7zip` (or the `py7zr` Python module).

## Build

```sh
packaging/build.sh
```

This builds beacn-utility (this repo), `beacn-mixer`, and Pipeweaver at a pinned upstream
commit (`006b045`) with `patches/pipeweaver/*.patch` applied, then assembles an installable
bundle in `dist/beacn-suite/` with checksums. Containerised alternative:

```sh
podman build -t beacn-build:f44 -f contrib/Containerfile .
podman run --rm -v "$PWD":/src:Z -w /src beacn-build:f44 packaging/build.sh
```

## Install (per user, no sudo)

```sh
cd dist/beacn-suite && ./install.sh
```

`install.sh` verifies the checksums, backs up your current binaries, launchers, autostart
entries, user units and the beacn-utility / Pipeweaver config to `~/beacn-backup-<timestamp>/`
(path saved in `~/.beacn-last-backup`), then:

- installs `beacn-utility`, `pipeweaver-daemon`, `pipeweaver-client` to `~/.local/bin` and
  `beacn-mixer` as `~/.local/bin/pipeweaver-app` (what Pipeweaver's "Open Interface" runs),
- installs the "Beacn" launcher, icon and `beacn.service` (systemd user unit),
- hides (does not delete) old Pipeweaver / beacn-utility launchers, removes their autostart
  entries, disables `pipeweaver.service`, and enables `beacn.service`.

Check it: `systemctl --user status beacn`, `pgrep -a pipeweaver-dae` (its parent should be
beacn-utility), `journalctl --user -u beacn`. If the mixer window can't open from the service,
check `systemctl --user show-environment | grep WAYLAND`.

## Official artwork kit (from your own installer)

You need the BEACN Windows installer you downloaded from BEACN (`BEACN Setup V1.4.2.12.exe`).
Nothing is downloaded or run; the installer's embedded archive is read directly.

```sh
tools/setup-official-artwork.sh "/path/to/BEACN Setup V1.4.2.12.exe"
systemctl --user restart beacn
```

This extracts `BEACN.exe` and the default `MixImages/default.images` pack, carves the screen
components out of `BEACN.exe` (matched by content hash, so another BEACN version fails loudly
rather than producing a wrong kit), and writes the kit to `~/.local/share/beacn-official/`.
It then points beacn-utility at it via `~/.config/beacn-utility/official-artwork` and
`official-screen`. You can also set `BEACN_OFFICIAL_ARTWORK` / `BEACN_OFFICIAL_SCREEN`.
The lower-level tools are `tools/extract-beacn-installer.py` and `tools/make-official-kit.py`.

**Do not share or commit the generated kit or extracted files.** `.gitignore` excludes them.

## Rollback

```sh
dist/beacn-suite/rollback.sh                 # uses the last backup
dist/beacn-suite/rollback.sh ~/beacn-backup-YYYYMMDD-HHMMSS
```

It disables and removes `beacn.service` and the new launcher, restores the backed-up binaries
(including symlinks), launchers, autostart entries and units, re-enables `pipeweaver.service`
if it was enabled, and starts the old beacn-utility. Generated artwork kits are left in place.

## Known limits

- **Two windows on Wayland.** The device window (iced/winit) can't embed a web view on
  Wayland (child web views only work on X11), so the mixer is its own window. Both are one
  app (one service, tray, launcher and taskbar group). A truly single window would need a
  native mixer page inside beacn-utility.
- **Default BEACN theme only.** The other official themes (analogue, neumorphic, ...) are not
  mapped yet.
- Kit generation is pinned to BEACN app **1.4.2.12** (by asset hashes).
- WebKitGTK rendering problems on some GPUs can be worked around with
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`.

## Pipeweaver patch: not submitted upstream

`patches/pipeweaver/0001-daemon-add-no-tray-option.patch` adds `--no-tray` /
`PIPEWEAVER_NO_TRAY=1` to the daemon. Pipeweaver's [AI policy](https://github.com/pipeweaver/pipeweaver/blob/main/AI_POLICY.md)
does not accept AI-generated contributions, and this patch was written with AI assistance, so
it is kept here only as a patch file and **has not been and should not be submitted upstream**
in this form. The same applies to the beacn-utility changes in this fork; please don't file
them upstream as-is.

## Credits

- **beacn-utility** by Craig McLure ([FrostyCoolSlug](https://github.com/FrostyCoolSlug)) and
  contributors, <https://github.com/beacn-on-linux/beacn-utility>. MIT License,
  Copyright 2025-2026 Craig McLure. This repository is a fork of it.
- **Pipeweaver** by Craig McLure ([FrostyCoolSlug](https://github.com/FrostyCoolSlug)) and contributors, <https://github.com/pipeweaver/pipeweaver>.
  MIT License, Copyright 2025 Craig McLure. Built from upstream source; only a patch is stored here.
- **Roboto** font (`resources/fonts/roboto/`), Apache License 2.0 (see its LICENSE).
- **BEACN** makes the Mix, Mix Create, Mic and Studio hardware and the official Windows app.
  This project is not affiliated with BEACN.
- The beacn-suite additions were developed with AI assistance (Grok Bot).

## License

MIT, see [LICENSE](LICENSE) (the upstream beacn-utility license and copyright notice are kept
unchanged). The beacn-suite additions (`beacn-mixer/`, `packaging/`, `tools/`, `contrib/`,
`patches/`, and the changes in `src/`) are also released under the MIT License.
Pipeweaver is MIT-licensed by its authors; the patch in `patches/pipeweaver/` is offered under
the same license.
