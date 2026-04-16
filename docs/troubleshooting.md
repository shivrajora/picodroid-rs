# Troubleshooting

Common pitfalls and their solutions.

## `cargo test` fails with target errors

The default Cargo target is `thumbv6m-none-eabi` (bare-metal ARM), so bare `cargo test` will fail. Use the test script instead:

```bash
./scripts/test.sh
```

This runs tests on the host target automatically.

## `./scripts/flash.sh` never exits

This is expected. `flash.sh` flashes the firmware and then streams RTT log output indefinitely. Run it in a separate terminal or in the background:

```bash
./scripts/flash.sh --app helloworld &
```

## `blinky` loops forever in the simulator

The blinky app blinks an LED in an infinite loop, which means the simulator will never exit. Kill it after a timeout:

```bash
# macOS (no built-in timeout command)
perl -e 'alarm 5; exec @ARGV' ./scripts/sim.sh --app blinky

# Linux
timeout 5 ./scripts/sim.sh --app blinky
```

## Clippy fails when run on the host

Bare `cargo clippy` fails because the default target is bare-metal ARM. Use the feature flags:

```bash
# RP2040
PICODROID_APK_PATH=build/apks/helloworld.papk cargo clippy --no-default-features --features board-testbench-rp2040 -- --deny=warnings

# RP2350
PICODROID_APK_PATH=build/apks/helloworld.papk cargo clippy --target thumbv8m.main-none-eabihf --no-default-features --features board-testbench-rp2350 -- --deny=warnings

# Simulator (host)
PICODROID_APK_PATH=build/apks/helloworld.papk cargo clippy --target "$(rustc -vV | awk '/^host:/ { print $2 }')" --no-default-features --features sim,board-testbench-rp2350 -- --deny=warnings
```

Or just run the full pre-commit suite which handles all of this:

```bash
./scripts/pre-commit
```

## UART / COM port issues with pdb

- The default serial port is `/dev/cu.usbmodem102` at 115200 baud
- **Connect your terminal (CoolTerm, screen, etc.) BEFORE flashing** — the USB CDC port enumerates during boot
- Avoid raw `stty` / `echo` commands to the port — they can cause a USB reset and disconnect the device
- If the port disappears, unplug and replug the Pico, then re-run `pdb devices` to find the new port name

## Pre-commit hook not running

The hook must be symlinked after cloning:

```bash
ln -s ../../scripts/pre-commit .git/hooks/pre-commit
```

To verify it is installed: `ls -la .git/hooks/pre-commit` should show it pointing to `../../scripts/pre-commit`.

## `PAPK framework-map-version incompatible with firmware`

The firmware panics at PAPK load with something like:

```
PAPK framework-map-version incompatible with firmware (firmware = 0.0.0):
    FrameworkVersionMismatch
```

The two most common causes:

1. **Firmware and PAPK disagree about `--shrink`.** Shrinking is opt-in
   per build. If you built the firmware without `--shrink` but the
   PAPK with it (or vice versa), load-time linkage would fail — so
   `verify_compat` rejects the combination up front. Rebuild both with
   the same flag:

   ```bash
   # Either both off (default)
   ./scripts/build-apk.sh --app <name>
   ./scripts/flash.sh     --app <name>

   # Or both on
   ./scripts/build-apk.sh --app <name> --shrink
   ./scripts/flash.sh     --app <name> --shrink
   ```

2. **PAPK was packaged against a shrink-map release newer than the
   firmware's** (both sides `--shrink`-on, but PAPK's Cargo.toml
   version bumped past what the firmware knows). Rebuild the PAPK
   against the current source tree.

`FrameworkVersionMissing` means the PAPK predates the manifest key
entirely (legacy, pre-M1). Also fixed by rebuilding. See
[shrinker.md](shrinker.md) for the full compatibility story.

## `pdb install` says "Refusing to install"

`pdb install` runs a host-side compatibility pre-flight against the
device's running firmware before erasing flash. Two messages you may see:

1. **"PAPK is incompatible with running firmware"** — the PAPK and the
   running firmware disagree about `--shrink` (or the PAPK's release map
   version is newer). The on-device PAPK is untouched. Rebuild the PAPK
   with the matching `--shrink` setting and re-run `pdb install`.

2. **"Firmware advertises 'picodroid/2.0', which predates the
   framework-map-version protocol field"** — the firmware was built
   before the compat-check protocol. `pdb install` won't push to it
   over USB. Reflash the firmware via SWD with `./scripts/flash.sh`,
   which brings up a `picodroid/2.1` build that advertises the field.

If `--skip-host-check` is passed (HIL test usage) and the device-side
check still fires, `pdb` reports `device rejected install:
STATUS_INCOMPAT` — same fix as case 1.

## Java formatting check fails

Java sources must follow Google Java Style. Reformat before committing:

```bash
./scripts/format_java.sh format
```

The formatter JAR is downloaded automatically on first use. JDK 11+ is required.
