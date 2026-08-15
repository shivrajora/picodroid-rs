---
title: "pdb command reference"
description: "Every pdb subcommand — devices, ping, install, sysmon, input, logcat — with flags, semantics, and the sim control-channel equivalents."
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

Round-trips a greeting to confirm the device is alive and the protocol versions match.

## install

```bash
pdb -s <port> install build/apks/<app>.papk
```

Hot-swaps an app: writes the PAPK to flash and restarts the JVM, without reflashing firmware. The walkthrough lives in [Hot-swap with pdb](/get-started/hot-swap/).

A PAPK is checked for compatibility before install: its `framework-map-version` must be less than or equal to the firmware's active version (see the [shrinker reference](/reference/shrinker/)).

| Flag | Effect |
|------|--------|
| `--skip-host-check` | Skip the host-side compat pre-flight (HIL test knob — exercises the device-side rejection path) |
| `--expect-rejected` | Invert exit codes: success when the install is rejected. Used by HIL `install-reject-*` test rows |

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
