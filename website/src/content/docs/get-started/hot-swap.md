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
```

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

The device stops the running JVM (including any sleeping child threads), writes the new PAPK to flash, and restarts execution — typically in under a second.

## Compatibility checks

Before flashing, `pdb install` runs two compatibility gates so a bad install never reboots the device:

1. **Host pre-flight** — parses the PAPK manifest for `framework-map-version`, compares it to the firmware's version learned from PING, and exits with a clear error if they don't match.
2. **Device-side check** — after parking core 0 but before erasing flash, the device peeks the install header and refuses with `STATUS_INCOMPAT` on mismatch.

Mismatches mean your PAPK was built against a different release than the firmware currently running. Rebuild the APK, or reflash a matching firmware. See [Class-name shrinker → Diagnosing version mismatch](/reference/shrinker/#diagnosing-version-mismatch).

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
