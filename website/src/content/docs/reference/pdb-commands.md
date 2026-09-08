---
title: "pdb command reference"
description: "Every pdb subcommand — devices, ping, install, list, uninstall, sysmon, input, logcat — with flags, semantics, and the sim control-channel equivalents."
---

`pdb` is the Picodroid Debug Bridge CLI: it talks to a flashed device over USB CDC. Build and run it with `cargo run -p pdb --`, or use the `./scripts/pdb.sh` wrapper. Every device command takes the serial port via `-s`:

```bash
cargo run -p pdb -- -s /dev/ttyACM1 <command>   # Linux
cargo run -p pdb -- -s /dev/cu.usbmodem102 <command>   # macOS
```

Running `pdb` with no arguments prints the up-to-date usage text.

## devices

```bash
pdb devices
```

Lists available serial ports so you can find the device's CDC port.

## ping

```bash
pdb -s <port> ping
```

Round-trips a greeting to confirm the device is alive and the protocol versions match. The greeting carries the largest PAPK the app region could hold, the firmware's `framework-map-version`, and — on multi-app firmware (`picodroid/2.2`, every RP2350 board) — the state of the package directory: `apps 3/8, free 1428 KB (largest 1024 KB)`. A single-app board (RP2040) reports `single-app`.

## install

```bash
pdb -s <port> install build/apks/<app>.papk
```

Hot-swaps an app: writes the PAPK to flash and restarts the JVM, without reflashing firmware. The walkthrough lives in [Hot-swap with pdb](/get-started/hot-swap/).

A PAPK is checked for compatibility before install: its `framework-map-version` must be less than or equal to the firmware's active version (see the [shrinker reference](/reference/shrinker/)).

On multi-app firmware the device also *places* the app before it erases anything. The PAPK's `package-name` (the manifest's `package=`) is its identity: a package that is already installed is upgraded — the new copy lands beside the old one and the old one is erased only after the new one commits, or, when there is no room beside it, over it — and a new package takes the first free run of the app region. If the free space is there but not in one piece the device compacts the region first, which can take up to half a minute; `pdb` waits. When even that cannot make room, or the directory is full, the device answers `STATUS_NO_ROOM` with the numbers and nothing has been erased:

```text
Refusing to install: device rejected install: STATUS_NO_ROOM — no room: need 380 KB, largest free 220 KB, total free 300 KB, apps 5/8
  Free room with `pdb uninstall <package>`; `pdb list` shows what is installed.
```

A PAPK without a `package-name` is refused on the host before the device is asked (`papk-pack --repack <file> --package-name <name>` adds one). On a single-app board the install replaces whatever is installed, as before.

| Flag | Effect |
|------|--------|
| `--skip-host-check` | Skip the host-side compat pre-flight (HIL test knob — exercises the device-side rejection path) |
| `--expect-rejected` | Invert exit codes: success when the install is rejected. Used by HIL `install-reject-*` test rows |

## list

```bash
pdb -s <port> list
```

Prints the package directory of a multi-app device — every installed app's sector, package, `version-code`, version, size, whether it is the boot app, and its label — followed by the free space:

```text
SECTOR PACKAGE      CODE VERSION     SIZE  BOOT LABEL
0      helloworld      1 1.0            1 KB  yes  helloworld
2      imagedemo       1 1.0           11 KB  -    Image Demo
free: largest 1480 KB, total 1480 KB, apps 2/8
```

The boot app is the one `flash.sh --app` baked in (a reinstall of the same package keeps the flag); it is what the device runs at power-up until the launcher lands. On single-app firmware the command reports that there is no directory to list.

## uninstall

```bash
pdb -s <port> uninstall <package>
```

Erases an installed app's whole run — its boot-meta sector and image — and reboots the device, then waits for it to come back. `NOT_FOUND` if no such package is installed; a system app cannot be uninstalled. Uninstalling the boot app leaves the device waiting for the next `pdb install`.

## sysmon

```bash
pdb -s <port> sysmon
```

Shows live system stats — heap usage, the FreeRTOS task table, CPU% — followed by the JVM heap block. With the memory-diagnostics build it also carries the `[memmon]` counters; see [Debugging](/guides/debugging/).

## input

```bash
pdb -s <port> input keyevent <KEYCODE|number>   # e.g. KEYCODE_DPAD_UP or 19
pdb -s <port> input dpad <up|down|left|right|center>
pdb -s <port> input back
pdb -s <port> input tap <x> <y>
pdb -s <port> input swipe <x1> <y1> <x2> <y2> [ms]   # default 300 ms
```

Injects synthetic input, Android-`adb`-style. Keycode names are case-insensitive and the `KEYCODE_` prefix is optional; bare integers are forwarded verbatim. The keycode→pin mapping is resolved **on the device** against the board's button table, and injection happens at the HAL layer — so focus navigation, BACK routing, and `MotionEvent` dispatch all behave exactly as they would for physical input.

Errors: `ERR (no such key)` for a keycode the board doesn't map; `ERR (no touch panel)` for `tap`/`swipe` on a board without touch.

The simulator's control channel accepts the **same verbs** (`input tap 40 60`, `input dpad down`, … via `./scripts/sim-ctrl.sh`), so an input sequence rehearsed headlessly in the sim replays verbatim on hardware. Details in [Debugging](/guides/debugging/).

## logcat

```bash
./scripts/sim.sh --app foo | pdb logcat --stdin --tag Foo --level W
```

Filters already-decoded Picodroid log text on stdin by tag and level.

| Flag | Effect |
|------|--------|
| `--stdin` | Read log text from stdin (required) |
| `--tag <T>` | Keep only lines tagged `<T>` (`[T]` sim format or `T:` RTT format) |
| `--level <V\|D\|I\|W\|E>` | Keep only lines at this level or higher |
