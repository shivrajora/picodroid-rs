---
title: "Troubleshooting"
description: "Common error messages and their fixes when working with Picodroid."
---

Common pitfalls and their solutions.

## `cargo test` fails with target errors

The `picodroid` firmware crate is bare-metal and the workspace sets **no default Cargo target**, so bare `cargo test` can't pick a host triple and fails. Use the test script instead:

```bash
./scripts/test.sh
```

This runs tests on the host target automatically.

## `./scripts/flash.sh` never exits

This is expected. `flash.sh` flashes the firmware and then streams RTT log output indefinitely. Run it in a separate terminal or in the background:

```bash
./scripts/flash.sh --app helloworld &
```

## `device lock: busy -- held by ...` (exit code 75)

The board is a single shared resource, and every script that touches it (`flash.sh`, `power-cycle.sh`, `pdb.sh`, `parity-bench.sh --hil`, `hil-run.sh`) takes a machine-wide lease through `scripts/device-lock.sh` first. A free board is acquired automatically for your session and kept until you give it back; a busy one makes the script exit 75 and name the holder.

```bash
./scripts/device-lock.sh status           # who holds it, since when, who is queued
./scripts/device-lock.sh acquire --wait   # queue (FIFO) until the board is yours
./scripts/device-lock.sh release          # when you are done; also kills a lingering probe-rs
./scripts/device-lock.sh break --force    # evict a holder who is really gone
```

A lease dies with the process that took it (your shell, or your Claude Code session), so a closed session never wedges the board. Long unattended runs that must survive their launcher take a pinned lease instead: `PICODROID_DEVICE_OWNER=soak ./scripts/device-lock.sh acquire --pin`, and release it at teardown.

If probe-rs itself reports `Failed to open probe` while the lock says the board is free, a stale `probe-rs` is still holding the USB interface: `./scripts/device-lock.sh release` kills it (never `pkill -f probe-rs`, which also kills any shell whose command line mentions it).

## `blinky` loops forever in the simulator

The blinky app blinks an LED in an infinite loop, which means the simulator will never exit. Kill it after a timeout:

```bash
# macOS (no built-in timeout command)
perl -e 'alarm 5; exec @ARGV' ./scripts/sim.sh --app blinky

# Linux
timeout 5 ./scripts/sim.sh --app blinky
```

## Clippy fails when run on the host

Bare `cargo clippy` fails because there's no default target set and the firmware crate needs an explicit target plus board feature flags. Use the feature flags:

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

```text
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

3. **PAPK was shrunk before method/field names were.** Since map
   v0.17.0 the firmware's own dispatch uses the mapped member names, so
   a PAPK shrunk with an older map is refused by a v0.17.0-or-later
   firmware even though older maps are otherwise accepted (the member
   floor). Rebuild the PAPK; `pdb install` names this reason
   explicitly.

`--shrink-app` never causes a mismatch: the per-app map extends the
release map without changing `framework-map-version`, so a
`--shrink-app` PAPK installs on any `--shrink` firmware of the same
release.

`FrameworkVersionMissing` means the PAPK predates the manifest key
entirely (legacy, pre-M1). Also fixed by rebuilding. See
[Shrinker](/reference/shrinker/) for the full compatibility story.

## `api contract: FAILED` — the app build stops in `verifyApiContract`

Apps compile against the host JDK's full `java.*`, but pico-jvm implements a
subset; `verifyApiContract` (part of `assemblePapk`) rejects any `java.*`
class or member the runtime does not serve *before* it can die on device as
`NoSuchMethod`. The report (`examples/<app>/build/reports/api-contract.txt`)
lists each reference with the reason, the call sites and a hint — e.g.
`java/util/LinkedList` → use `ArrayList`, `String.matches` → no regex,
`System.out` → `picodroid.util.Log`. Consult the
[compatibility matrix](/reference/compatibility-matrix/) for the supported
surface. An `EXCLUDED ON BOARD` section means the target board drops that
class from its framework (`framework_class_excludes` in its `board.toml`);
build for a larger board or probe-and-degrade.

`-Ppicodroid.apiContract=warn` (or `off`) bypasses the check while
experimenting, e.g. `./gradlew :examples:myapp:assemblePapk -Ppicodroid.apiContract=warn`.
Do not edit `sdk/api-contract.tsv` — it is generated from the runtime's
tables; to support a new member add the builtin arm and its
`BUILTIN_METHODS` row, then run `scripts/gen-api-contract.sh`.

## `pdb install` says "Refusing to install"

`pdb install` runs a host-side compatibility pre-flight against the
device's running firmware before erasing flash. Two messages you may see:

1. **"PAPK is incompatible with running firmware"** — the PAPK and the
   running firmware disagree about `--shrink` (or the PAPK's release map
   is newer than the firmware's, or older than its member floor). The
   on-device PAPK is untouched. Rebuild the PAPK
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

## Gradle build fails with "JAVA_HOME is not set" or "no Java runtime"

Java compilation runs through the Gradle wrapper (`./gradlew`) in-tree — no separate Gradle install is needed, but a **JDK 11+** must be on `PATH`. Install one (see [getting-started.md → JDK](/get-started/build/)) and verify with `javac --version`. If `JAVA_HOME` isn't set, point it at your JDK install root before rebuilding.

## `registerListener` returns `false` / sensor event never fires

Three common causes:

1. **No `[[sensor]]` entry in `board.toml`.** `SensorManager.getDefaultSensor(type)` returns `null` if the board doesn't declare a matching sensor. Add an entry — see [porting-guide.md → board.toml reference](/reference/porting-guide/#boardtoml-reference).
2. **I2C wiring mismatch.** The BME688 driver uses the `bus` + `addr` from `board.toml`. Verify the sensor ACKs on that bus with [`examples/i2cdemo`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/i2cdemo).
3. **Registration cap.** `SensorManager` allows up to 8 concurrent registrations. Call `unregisterListener()` from `onPause()` / `onDestroy()`-equivalent paths to avoid leaking slots across app swaps.

## Networking

### Build fails with `third_party/cyw43-driver is the unpatched upstream`

The `third_party/cyw43-driver` submodule moved to the patched picodroid fork. A checkout cloned before the switch still points at upstream, and the network build fails early rather than producing broken WiFi firmware. Re-sync the submodule:

```bash
git submodule sync && git submodule update --init third_party/cyw43-driver
```

### RTT shows `wifi: no SSID configured (PICODROID_WIFI_SSID) — not joining`

WiFi credentials are **build-time** environment variables, baked into the image — setting them at flash or run time does nothing. Rebuild (and reflash) with them set; until then the stack starts but stays offline:

```bash
PICODROID_WIFI_SSID='MyAP' PICODROID_WIFI_PASS='secret' ./scripts/flash.sh --board testbench_rp2350w --app netdemo --release
```

See [WiFi & networking setup](/get-started/networking/).

### Sockets fail immediately after boot

The WiFi join takes ~6 s and DHCP completes around 10 s after boot, so an app that opens a socket in its first moments races the link and loses. Poll `NetworkInfo.isConnected()` against a deadline (the example apps wait up to 30 s) before opening sockets — see [WiFi & networking setup](/get-started/networking/).

### Repeated `net: down` lines over RTT

The join is retrying. This is a known issue — see [Known issues & current limits](/reference/known-issues/). Once the join succeeds you'll see `net: up, ip a.b.c.d`.

## `HttpURLConnection` hangs or throws at `connect()`

- `HTTPS URLs are rejected` — `HttpURLConnection` is HTTP/1.1 only; no TLS. Use the raw socket API if you need TLS and are willing to bundle it.
- `setFixedLengthStreamingMode() required for output` — for POST/PUT, call `setDoOutput(true)` **and** `setFixedLengthStreamingMode(n)` with the exact body byte count before `connect()`.
- Hangs are usually DNS-resolution failures against an unreachable host. There is no per-operation timeout parameter yet; check that the `Host` header resolves from the device's network.
- `Connection: close` is always sent — keep-alive / pipelining is not supported, so one `HttpURLConnection` = one request.
