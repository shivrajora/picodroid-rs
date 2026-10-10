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

Every board on the bench is a shared resource, and every script that touches one (`flash.sh`, `power-cycle.sh`, `pdb.sh`, `parity-bench.sh --hil`, `hil-run.sh`) takes a lease on that board through `scripts/device-lock.sh` first. A free board is acquired automatically for your session and kept until you give it back; a busy one makes the script exit 75 and name the holder (the message names the slot, e.g. `device lock [pico_enviro_mon_w]: busy`). With several boards configured, say which one with `--board NAME`; a script that cannot tell stops and lists the slots.

```bash
./scripts/device-lock.sh status                     # every slot: holder, since when, queue
./scripts/device-lock.sh acquire --board X --wait   # queue (FIFO) until that board is yours
./scripts/device-lock.sh release                    # everything you hold; also kills a lingering probe-rs
./scripts/device-lock.sh break --slot X --force     # evict a holder who is really gone
```

`busy -- the whole bench is held by ... (a session without the fleet code)` means a checkout on a branch from before the fleet holds the old single machine-wide lease; it power-cycles the whole hub, so every slot waits for it. Merge `main` into that branch, or wait for it to finish.

A lease dies with the process that took it (your shell, or your Claude Code session), so a closed session never wedges a board. Long unattended runs that must survive their launcher take a pinned lease instead: `PICODROID_DEVICE_OWNER=soak ./scripts/device-lock.sh acquire --board X --pin`, and release it at teardown.

If probe-rs itself reports `Failed to open probe` while the lock says the board is free, a stale `probe-rs` is still holding the USB interface: `./scripts/device-lock.sh release` kills it (never `pkill -f probe-rs`, which also kills any shell whose command line mentions it).

## `pdb: board of slot ... is not enumerated` / `no picodroid devices found (slot ...)`

On a bench with several boards `pdb.sh` and the HIL runner find the board's serial port by its USB position from `~/.config/picodroid/fleet.conf`, never by scanning (the pdb device has no serial number, and a scan could land on a neighbour). This message means nothing is plugged into that position right now: the board is still rebooting, its cable moved, or the config line is stale. `./scripts/fleet.sh discover` shows what is actually on USB and where.

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
./scripts/build-apk.sh --app helloworld    # the PAPK the firmware crate embeds

# RP2040
PICODROID_APK_PATH=$(pwd)/build/apks/helloworld.papk cargo clippy -p picodroid --target thumbv6m-none-eabi --no-default-features --features board-testbench-rp2040 -- --deny=warnings

# RP2350
PICODROID_APK_PATH=$(pwd)/build/apks/helloworld.papk cargo clippy -p picodroid --target thumbv8m.main-none-eabihf --no-default-features --features board-testbench-rp2350 -- --deny=warnings

# Simulator (host)
PICODROID_APK_PATH=$(pwd)/build/apks/helloworld.papk cargo clippy -p picodroid --target "$(rustc -vV | awk '/^host:/ { print $2 }')" --no-default-features --features sim,board-testbench-rp2350,line-numbers -- --deny=warnings
```

The local pre-commit hook does not run clippy (only the source guards and formatters); GitHub CI runs these legs for every board on every push, and `./scripts/pre-commit --full` adds the `pico_enviro_mon_w` and `legacy-handle-cast` legs:

```bash
./scripts/pre-commit --full
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

The most common causes:

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

A `callback retired: the framework never calls it` entry is an `Activity`
that still declares the no-arg `onCreate()`. Declare
`protected void onCreate(Bundle savedInstanceState)` (import
`picodroid.os.Bundle`) and call `super.onCreate(savedInstanceState)`;
`Application` and `Service` keep their no-arg `onCreate()`.

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
   which brings up a current (`picodroid/2.2`) build that advertises the field.

3. **"PAPK file is not a valid PAPK"** — the file is truncated or
   malformed, or it was packed before PAPK v2 (`PAPK format version is
   not the one this build reads (a v1 file: re-pack it with the current
   toolchain)`). Rebuild it with `./scripts/build-apk.sh --app <name>`.

4. **"PAPK has no package-name; a multi-app device cannot place it"** —
   the manifest has no `package`. Add one, or repack the file with the
   `papk-pack --repack` command the message prints.

If `--skip-host-check` is passed (HIL test usage) and the device-side
check still fires, `pdb` reports `device rejected install:
STATUS_INCOMPAT` — same fix as case 1.

`device rejected install: STATUS_NO_ROOM` is not a compatibility
problem: a multi-app board's app region has no contiguous run for the
package even after compaction, or its directory is full. Nothing was
erased. Free room with `pdb uninstall <package>`; `pdb list` shows what
is installed.

## Java formatting check fails

Java sources must follow Google Java Style. Reformat before committing:

```bash
./scripts/format_java.sh format
```

The formatter JAR is downloaded automatically on first use. It is a Java 21 jar: when the `java` on your `PATH` is older, point `JAVA_HOME` at a JDK 21 and the script uses that one.

## Gradle build fails with "JAVA_HOME is not set" or "no Java runtime"

Java compilation runs through the Gradle wrapper (`./gradlew`) in-tree — no separate Gradle install is needed, but a **JDK** must be on `PATH`. Install JDK 21, which the Java formatter needs as well (see [getting-started.md → JDK](/get-started/build/)) and verify with `javac --version`. If `JAVA_HOME` isn't set, point it at your JDK install root before rebuilding.

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

<a id="rtt-shows-wifi-no-ssid-configured-picodroid_wifi_ssid--not-joining"></a>

### RTT shows `wifi: no network configured (Settings > Wi-Fi, or PICODROID_WIFI_SSID) — not joining`

The board has no saved network and none was built in, so the stack starts but stays offline. There are two ways to give it one:

- **On the device:** open Settings → Wi-Fi from the launcher, scan, pick the network and type its password. The network is saved on the storage volume (`/system/wifi`) and rejoined at every boot. See [Joining a network from Settings](/get-started/networking/#joining-a-network-from-settings).
- **At build time:** `PICODROID_WIFI_SSID` / `PICODROID_WIFI_PASS` are baked into the image — setting them at flash or run time does nothing — and take precedence over a saved network:

  ```bash
  PICODROID_WIFI_SSID='MyAP' PICODROID_WIFI_PASS='secret' ./scripts/flash.sh --board testbench_rp2350w --app netdemo --release
  ```

See [WiFi & networking setup](/get-started/networking/).

### Sockets fail immediately after boot

The WiFi join takes ~6 s and DHCP completes around 10 s after boot, so an app that opens a socket in its first moments races the link and loses. Open sockets from a `ConnectivityManager.NetworkCallback`'s `onAvailable` (Android's shape; see [Network status](/api/networking/#network-status)), or, in an app with no Activity, poll `NetworkInfo.isConnected()` against a deadline (the `netdemo` and `http_get` examples wait up to 30 s) — see [WiFi & networking setup](/get-started/networking/).

### `net: down` over RTT with no `net: up` after it

The join is failing and the stack is retrying it every 3 s. The line is logged once per change of state, so silence after it means the retries are still failing, not that they stopped. The WiFi driver's own lines say why — `wifi: join failed: bad password`, `wifi: join failed: no such network` — so check the network saved in Settings → Wi-Fi, or the SSID and password the firmware was built with (`PICODROID_WIFI_SSID` / `PICODROID_WIFI_PASS`), which override it. Once a join succeeds you'll see `net: up, ip a.b.c.d`.

## `HttpURLConnection` hangs or throws at `connect()`

- `UnsupportedOperationException` on an `https` URL — the board was built without TLS. HTTPS needs `has_tls = true` in `board.toml`, which the RP2350 WiFi boards set.
- `SSLHandshakeException: wall clock not set; the network time service has not synced yet` — the certificate's validity cannot be checked until the clock is set. The platform's time service sets it from `pool.ntp.org` once the link is up and the handshake waited 8 s for that, so the network has no route to the pool, or Settings → Date & time has automatic time off; see [Wall clock](/api/networking/#wall-clock-the-time-service-and-sntpclient). The log says why: `time: pool.ntp.org did not resolve`, `time: no usable reply from pool.ntp.org: timed out`. In the simulator `PICODROID_SIM_WALL_CLOCK=1` anchors the clock to the host's.
- `SSLHandshakeException: certificate chain of <host> is not issued by a known root` — the server's chain does not end in one of the roots compiled into the firmware. An app cannot add one.
- `setFixedLengthStreamingMode() required for output` — for POST/PUT, call `setDoOutput(true)` **and** `setFixedLengthStreamingMode(n)` with the exact body byte count before `connect()`.
- Hangs: both timeouts default to infinite. Set `setConnectTimeout(ms)` and `setReadTimeout(ms)` before `connect()`; on expiry they throw `SocketTimeoutException`. A host that does not resolve throws `UnknownHostException`, and a refused connection `ConnectException`.
- `Connection: close` is always sent — keep-alive / pipelining is not supported, so one `HttpURLConnection` = one request.
- An HTTPS `connect()` takes one to two seconds on an RP2350 and needs a 40 KB stack from the heap arena for that long; call it from a background thread, never the main thread.

## The screen freezes, or a screen comes back empty

- **Frozen, no exception, no log line** after a screen built many widgets: LVGL's pool is full. See [A full LVGL pool freezes the UI](/guides/embedded-gotchas/#a-full-lvgl-pool-freezes-the-ui-without-a-log-line).
- **A screen the user returns to has lost its state**, with `activity: reclaim <class>` in the log: the covered Activity was destroyed to free memory and re-created. Save state in `onSaveInstanceState`; see [A covered Activity can be destroyed and rebuilt](/guides/embedded-gotchas/#a-covered-activity-can-be-destroyed-and-rebuilt).
- **`slow handler: <span> took N ms`**: one handler held the UI tick for 50 ms or more. Build big screens a few rows per tick and move work to `Executors.backgroundExecutor()`; see the [slow-handler watchdog](/get-started/simulator/#slow-handler-watchdog).
