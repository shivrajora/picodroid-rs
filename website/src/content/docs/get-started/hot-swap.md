---
title: "Hot-swap with pdb"
description: "Push a new PAPK to a running device over USB CDC without reflashing the firmware."
---

The **Picodroid Debug Bridge** (`pdb`) lets you push a new app to a running device over USB CDC without reflashing the firmware. The firmware exposes a USB CDC serial port (e.g. `/dev/cu.usbmodem102` on macOS, `/dev/ttyACM0` on Linux) that pdb talks to — no extra wiring required beyond the USB cable.

## Quick access via script

```bash
./scripts/pdb.sh devices
./scripts/pdb.sh -s /dev/cu.usbmodem102 ping
./scripts/pdb.sh -s /dev/cu.usbmodem102 install build/apks/blinky.papk
./scripts/pdb.sh -s /dev/cu.usbmodem102 sysmon
./scripts/pdb.sh -s /dev/cu.usbmodem102 input tap 120 80
./scripts/sim.sh --app foo | ./scripts/pdb.sh logcat --stdin --tag Foo
```

The subcommands are `devices`, `ping`, `install`, `list`, `uninstall`,
`sysmon`, `input` (tap/swipe/keyevent injection), and `logcat` (tag/level log
filtering) — see the [pdb command reference](/reference/pdb-commands/) for
every flag. Without `-s`, `pdb` picks the device itself when exactly one
device or simulator answers.

`pdb.sh` is a launcher for the `pdb` binary that also takes the bench lease
`flash.sh` takes, so two sessions never drive one board at once. On a bench
with several boards (`~/.config/picodroid/fleet.conf`) name the board instead
of the port, and the wrapper looks the port up:

```bash
./scripts/pdb.sh --board pico_enviro_mon_w ping
```

See [Sharing the bench](/project/contributing/#sharing-the-bench).

## Try it on the simulator first

No board needed: the [simulator](/get-started/simulator/) is a `pdb` device
too. It listens on a socket, and `-s sim` finds it:

```bash
./scripts/sim.sh --app blinky --system-apps      # one terminal: blinky + the system apps
./scripts/pdb.sh devices                         # another: the sim's row ends in [sim]
./scripts/pdb.sh -s sim install build/apks/helloworld.papk
./scripts/pdb.sh -s sim list
./scripts/pdb.sh -s sim uninstall helloworld
```

The install runs the same code it runs on a device — the JVM parks, the app
region is written, and the simulator reboots itself (a warm boot of its own
process) before `pdb` reports `Install complete.` Use `--system-apps` or a
looping app: a simulator with nothing left to run exits, unless
`PICODROID_SIM_WAIT_FOR_INSTALL=1` tells it to wait for an install as a
device does.

## Install the host tool globally (optional)

```bash
cargo install --path tools/pdb
```

## Push an app

```bash
# Build the app first
./scripts/build-apk.sh --app blinky

# Find the serial port
pdb devices

# Push the PAPK
pdb -s /dev/cu.usbmodem102 install build/apks/blinky.papk
```

The device stops the running JVM (including any sleeping child threads), writes the new PAPK to flash, and reboots; `pdb` waits for it to answer again and prints `Install complete.`

What runs after the reboot is what the device boots. On a single-app board that is the app just installed. On a multi-app board it is, unless `flash.sh --boot` chose otherwise, the boot app — the one `flash.sh --app` baked in — so reinstalling *that* package (the edit, build, install loop) runs the new copy at once, while a different package lands beside it and is started from the launcher.

## Several apps on one device

RP2350 boards keep an *app region* of flash that holds up to eight installed apps at once (sixteen on `pico_touch_kit`; `max_installed_apps` in `board.toml`). `pdb install` places a new package beside the ones already there (upgrading a package that is already installed) and refuses, without erasing anything, when there is no room; `pdb list` shows what is installed and how much is free; `pdb uninstall <package>` erases one. The device boots the app `flash.sh --app` baked in; when that app finishes, the launcher built into the firmware shows what is installed and starts the one you pick. `flash.sh --boot launcher` boots the launcher first. The settings app, also built in, uninstalls apps from the device itself (Settings → Apps). See the [launcher guide](/guides/launcher/).

```bash
pdb install build/apks/imagedemo.papk     # beside blinky
pdb list
pdb uninstall imagedemo
```

RP2040 boards keep a single app, which every install replaces.

## Compatibility checks

Before flashing, `pdb install` runs two compatibility gates so a bad install never reboots the device:

1. **Host pre-flight** — checks that the file is a well-formed PAPK no larger than the device can hold, then parses its manifest for `framework-map-version`, compares it to the firmware's version learned from PING, and exits with a clear error if the two are incompatible. For a multi-app device the PAPK must also carry a `package-name` (every PAPK the build produces does: it is the manifest's `package=`).
2. **Device-side check** — after stopping the JVM but before erasing flash, the device peeks the install header and refuses with `STATUS_INCOMPAT` on mismatch. (During the flash writes themselves the JVM stays blocked on core 0 and core 1 is parked by the `flashpark` task.)

A mismatch means the PAPK and the firmware disagree about `--shrink`, or the PAPK was built from a newer release than the firmware that is running. Rebuild the PAPK, or reflash a matching firmware. See [Class-name shrinker → Diagnosing version mismatch](/reference/shrinker/#diagnosing-version-mismatch).

## Verify connectivity

```bash
pdb -s /dev/cu.usbmodem102 ping
```

## System monitor

Query heap usage, task states, stack high-water marks, and per-task CPU usage:

```bash
pdb -s /dev/cu.usbmodem102 sysmon
```

CPU % is computed from the delta between consecutive queries. The first query reports CPU % as N/A; run it again after a few seconds to see actual per-task CPU usage.

## Inspect a PAPK file

```bash
cargo run -p papk-info -- build/apks/blinky.papk
```

Prints the manifest, class list, bytecode size of each class, and (if present) the bundled-asset section.
