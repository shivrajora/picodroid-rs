---
title: "Debugging"
description: "Tools and symptom-driven playbooks for debugging Picodroid apps: RTT, the host simulator, pdb sysmon, GDB, GC sweeps, OOM, and headless input."
---

This page has two halves. **Tools** is the standing kit — how to get logs, run the simulator, read system health, and step with GDB. **Playbooks** is symptom-driven: start there when something is already broken.

## Tools

### RTT Logging

`flash.sh` flashes the firmware and streams RTT log output via [defmt](https://defmt.ferrous-systems.com/) and probe-rs. Log levels are controlled by `DEFMT_LOG` (set to `debug` by default in [`.cargo/config.toml`](https://github.com/shivrajora/picodroid-rs/blob/main/.cargo/config.toml)):

```toml
[env]
DEFMT_LOG = "debug"
```

Override it per-invocation by exporting `DEFMT_LOG` before `flash.sh` (e.g. `DEFMT_LOG=trace`). On hardware, `Log.i(TAG, msg)` arrives over RTT as `TAG: msg`; in the simulator the same call prints `[TAG] msg`.

### Host Simulator

The host simulator lets you run apps on your development machine without hardware. Hardware calls are stubbed with logged output, making it useful for testing app logic and debugging JVM behaviour.

```bash
./scripts/sim.sh --app helloworld
./scripts/sim.sh --app blinky          # loops forever — Ctrl-C to stop
./scripts/sim.sh --app benchmark       # JVM performance benchmark (host-only)
./scripts/sim.sh --app gcstress        # GC stress test (host-only)
./scripts/sim.sh --app displaydemo     # opens a 320x240 graphical window
```

For display apps, the simulator opens a graphical window (via minifb) that renders the LVGL widget tree with mouse-as-touch input. Close the window or press Escape to exit.

To drive the UI without a local display (over SSH, in CI, or for scripted QA), see [Driving the simulator headlessly](#driving-the-simulator-headlessly) below.

### System Monitor (pdb sysmon)

The `pdb sysmon` command queries runtime system health over the device's USB CDC port without reflashing or adding debug prints:

```bash
pdb -s /dev/cu.usbmodem102 sysmon
```

This reports:

- **Heap**: free bytes, minimum-ever free bytes (high-water mark)
- **Uptime**: tick count and wall-clock seconds
- **Task table**: every FreeRTOS task with name, state, priority, stack high-water mark, and CPU %

CPU % is computed from the delta between consecutive queries — run it twice with a few seconds in between. The first query shows CPU % as N/A.

Under the hood this uses `xPortGetFreeHeapSize()`, `xPortGetMinimumEverFreeHeapSize()`, and `uxTaskGetSystemState()` from FreeRTOS, with run-time stats driven by the hardware microsecond timer (TIMERAWL register). There is no background sampling task — stats are collected on-demand when the host sends the query, so there is zero impact on power consumption or scheduling.

The min-ever-free figure is your heap high-water mark across the whole run; it is the single most useful number for chasing memory pressure (see [Out of memory / heap exhaustion](#out-of-memory--heap-exhaustion)).

### Injecting input (pdb input)

`pdb input` drives a real device from the host — the picodroid analog of
`adb shell input tap|swipe|keyevent`. It is the hardware counterpart of the sim
[control channel](#driving-the-simulator-headlessly): the same Android verbs,
sent over the device's USB CDC port, so a script or an AI agent can exercise an
app end-to-end without touching the board.

```bash
pdb input keyevent KEYCODE_DPAD_DOWN   # press+release a key (name or number, e.g. 20)
pdb input dpad up                      # up|down|left|right|center — D-pad shorthand
pdb input back                         # KEYCODE_BACK shorthand
pdb input tap 120 80                   # touch tap at (x, y)
pdb input swipe 40 60 200 60 300       # swipe (x1 y1)->(x2 y2) over [ms] (default 300)
```

The injection happens at the HAL layer on the device (a GPIO edge for keys, the
touch sampler for tap/swipe), so the full on-device pipeline runs unchanged — the
EditMode filter, LVGL focus navigation, `KeyEvent`/`MotionEvent` dispatch, and
BACK routing all behave exactly as they do for a physical press. This mirrors
Android's `InputManager.injectInputEvent`, where the on-device program builds the
event and injection is privileged (here, reachable only over PDB).

Keycode→pin resolution happens on the device against the board's button table,
so the host stays board-agnostic — a keycode with no matching button returns
`ERR (no such key)`, and `tap`/`swipe` on a board with no touchscreen returns
`ERR (no touch panel)`. On success the command prints nothing and exits `0`.

### GDB

GDB debugging is a two-terminal workflow. First, start probe-rs as a GDB server (it listens on `localhost:1337` by default):

```bash
# RP2040
probe-rs gdb --chip RP2040

# RP2350 (RP2350/RP2354 boards all use the RP235x probe-rs target)
probe-rs gdb --chip RP235x
```

Then, in a second terminal, launch GDB against the ELF and connect to the server:

```bash
# RP2040
arm-none-eabi-gdb target/thumbv6m-none-eabi/debug/picodroid \
    -ex "target remote localhost:1337"

# RP2350
arm-none-eabi-gdb target/thumbv8m.main-none-eabihf/debug/picodroid \
    -ex "target remote localhost:1337"
```

## Playbooks

### NoSuchMethod / input dies after a while

**Symptom.** An app works for the first few interactions, then breaks. A freshly-opened Activity renders wrong and the log shows:

```text
[sim] Activity lifecycle error: NoSuchMethod
```

On hardware the same line arrives over RTT without the `[sim]` prefix: `Activity lifecycle error: NoSuchMethod`. A second, related line can appear from the framework-default fallback path: `Activity lifecycle fallback error: NoSuchMethod`. These are emitted from the lifecycle dispatcher in [`platforms/rp/src/lifecycle.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/lifecycle.rs).

There is **no "native miss" log line** — ignore any guidance that tells you to grep for one. The real diagnostic surface for this failure is the `Activity lifecycle error: NoSuchMethod` line above.

**What it usually means.** The JVM heap is a non-moving mark-sweep collector with slot reuse; class and method tables are append-only and never collected. So `NoSuchMethod` at runtime almost always means a *still-referenced* object was swept (a missing GC root), its heap slot was reused by a later allocation, and a subsequent method dispatch hit the wrong class's vtable.

The canonical case: the `Display` singleton (returned by `Display.getInstance()`) was swept, its slot reused by a `SensorEvent`, and the next `Activity.setContentView` resolved `setContentView` on a `SensorEvent` — which has no such method. The same failure mode applies to any object held only by Rust-side native state (listener maps, cached singletons) and never by a Java field.

**How to recognise it (timeline).**

- It works as the *first* navigation but breaks on the 2nd or 3rd screen transition. More interaction trips it sooner.
- The breakage lines up exactly with a GC. Correlate the failure with a collection line:

  ```text
  [PicoEnvMon] Live.onCreate                     ← nav #3
  [sim] Activity lifecycle error: NoSuchMethod   ← fails here
  [sim] JVM ... gc: 1 collections, 2327 freed    ← the GC that broke it
  ```

- In isolation each screen works fine — the failure is cumulative, not screen-specific.

There is a **silent variant**: a listener kept alive only by its native map (a `Switch`, `CheckBox`, `ToggleButton`, or `EditText` created as a local in `onCreate` and never stored in a Java field) is swept on the first GC. The widget still moves visually, but `onCheckedChanged` (or the editor-action callback) dispatches onto a wrong-class object and the error is swallowed — no log, the listener just stops firing.

**Prevention.** Keep a Java field reference to anything whose callback must keep firing. The framework now roots Views, dialogs, compound-button and editor listeners, the `Display` singleton, sensors, and bound-service refs through its GC walk, but the cheapest defence in your own app is to never let a listener-bearing widget be reachable only from a local. See the GC-lifetime rules in [Embedded gotchas](/guides/embedded-gotchas/).

**Diagnosing a new instance.** This class of bug is found by *runtime tracing*, not static reading: trace the receiver class and method at the failure (`recv_class=... method=...`) and confirm the receiver is the wrong type for the method being called. If so, the object needs a GC root.

### Out of memory / heap exhaustion

There is **no `OutOfMemoryError`** in Picodroid and no `OutOfMemory` error variant in the JVM. Allocation failure is handled in two different ways depending on what you allocate:

- **Array allocations** (`new int[]`, `new T[]`, multi-dimensional) degrade gracefully: on failure the interpreter rewinds, flags an emergency GC, and re-executes the opcode after collecting. If the GC frees enough, your allocation succeeds and you never see an error.
- **Object allocations** (`new SomeClass()`) do **not** retry. If the threshold GC didn't pre-empt the shortage, the failure surfaces as a fatal error tagged `StackOverflow` — that is the JVM's catch-all for "couldn't allocate", not a stack-depth problem. Treat a surprising `StackOverflow` during object construction as object-heap exhaustion.

GC runs after an opcode once allocations cross the threshold (`GC_ALLOC_THRESHOLD`, default 256) or the emergency flag is set. Tuning that threshold and the heap sizes is documented in [JVM tunables](/reference/jvm-tunables/); the hard ceilings (heap sizes, object/string/array caps) live in [Limits](/reference/limits/). Don't fight OOM by guessing — read those two pages for the actual knobs.

**Introspect from Java.** `picodroid.os.Runtime` exposes the live heap and GC counters so an app can watch its own footprint:

```java
import picodroid.os.Runtime;

long used = Runtime.usedMemory();   // bytes live across object/array/string heaps
long peak = Runtime.peakMemory();   // max usedMemory() since last reset
Runtime.resetPeakMemory();          // snap peak to current used

long gcNs = Runtime.gcTimeNanos();  // cumulative GC time
int  gcN  = Runtime.gcCount();      // collections so far
int  freed = Runtime.gcFreed();     // objects freed
Runtime.resetGcStats();             // zero the gc time/count/freed counters
```

`usedMemory()` is approximate (a sum of the live object, array, and string heaps) and updates the peak as a side effect. The full API is on [Runtime.java](https://github.com/shivrajora/picodroid-rs/blob/main/sdk/java/picodroid/os/Runtime.java).

**Watch the high-water mark.** On hardware, `pdb sysmon` reports the minimum-ever free heap — that is your native-side high-water mark across the whole run. Wrap a suspect screen, sample `pdb sysmon` before and after, and watch the min-free figure fall.

**Stress apps.** Three example apps under [`examples/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples) exercise the allocator and print scores you can baseline against:

- `gcstress` — object churn, linked chains, circular refs, string churn; each phase prints `name: <us> us (gc: <us> us, <n> collections, <freed> freed)` and ends with `=== PASSED ===`.
- `heapstress` — heap-fragmentation stress; same per-phase report format, ends `=== PASSED ===`.
- `perfbench` — speed + memory composite; prints `SUBSCORE ...`, then `SCORE <total>`, then `=== PASSED ===`.

Run any of them in the simulator with `./scripts/sim.sh --app gcstress` (etc.) and compare the totals before and after a change.

### A listener never fires

If a registered callback simply never runs, work through these in order.

**`SensorManager.registerListener` returned `false`.** Always check the return value:

```java
boolean ok = sensorManager.registerListener(this, sensor, rate);
if (!ok) {
    // registration was rejected — nothing will be delivered
}
```

It returns `false` when the listener or sensor argument is not a valid object, when the sensor's type can't be read, or when **all 8 registration slots are already in use** (the cap is 8 concurrent registrations). Re-registering the same `(listener, sensorType)` pair returns `true` and just refreshes the rate. A common trap is `getDefaultSensor(type)` returning `null` because the board has no matching `[[sensor]]` entry — see [registerListener returns false / sensor event never fires](/guides/troubleshooting/#registerlistener-returns-false--sensor-event-never-fires) in Troubleshooting. The sensor API is covered in [Sensors](/api/sensors/).

**A lifecycle/service op was silently dropped.** Activity and Service transitions go through an internal pending-op queue with a fixed depth. When that queue is full, the op is **dropped with no log at all** — despite some internal comments suggesting otherwise, there is no overflow log on this path. If a service callback or transition is intermittently missing under heavy churn, suspect queue overflow and reduce the rate of pending transitions. (The separate `MainExecutor`/`BackgroundExecutor` queues *do* log `queue full, dropped` — that is a different queue.)

**The listener was GC-swept.** A View, dialog, compound-button, or `EditText` listener whose only owner is a native map gets collected on the first GC and stops firing a few seconds in, with no error. This is the silent variant of the [NoSuchMethod playbook](#nosuchmethod--input-dies-after-a-while) — keep a Java field reference to the widget. See [Embedded gotchas](/guides/embedded-gotchas/).

> Note: on the host simulator the hardware key/touch dispatcher never fires — `drain_gpio_event` always returns `None` in sim builds. Button input in the sim comes through the control channel below (which uses a different injection path that *does* work for `has_buttons` boards), not the hardware key path. End-to-end key verification on the hardware path requires a device — use [`pdb input`](#injecting-input-pdb-input) to drive that path over USB CDC.

### pdb install fails

Every refusal prints `Refusing to install: <reason>` and exits non-zero. The reasons you'll actually hit:

**Legacy firmware (no compat protocol).** The firmware predates the framework-map-version handshake, so `pdb` can't verify compatibility over USB:

```text
Firmware advertises "picodroid/2.0", which predates the framework-map-version protocol field.
pdb cannot verify install compatibility against this firmware.
Reflash firmware via SWD (./scripts/flash.sh) to install over USB.
```

Recovery: reflash via SWD (`./scripts/flash.sh`) to advance to a firmware that advertises the field. See [`pdb install` says "Refusing to install"](/guides/troubleshooting/#pdb-install-says-refusing-to-install).

**Shrink-map version mismatch.** The PAPK and the running firmware were built with different `--shrink` settings, so their framework maps don't line up:

```text
PAPK is incompatible with running firmware.
  PAPK     framework-map-version = ...
  Firmware framework-map-version = ...
  Reason: ...
  Rebuild the PAPK with matching --shrink setting (see docs/shrinker.md).
```

The same condition can surface as a firmware-load panic, `PAPK framework-map-version incompatible with firmware`. **Recovery:** rebuild the APK and firmware with the *same* `--shrink` flag, or rebuild the PAPK without `--shrink` to match a non-shrunk firmware, or reflash matching firmware. The shrinker is documented in [Shrinker](/reference/shrinker/); the panic and recovery steps are in [`PAPK framework-map-version incompatible with firmware`](/guides/troubleshooting/#papk-framework-map-version-incompatible-with-firmware).

**Other refusals.** A malformed package is rejected unconditionally with `PAPK file is not a valid PAPK: <error>` (this check runs even under `--skip-host-check`, to prevent bricking). An over-size package fails with `error: PAPK is <n> KB but device supports max <n> KB`. If you bypass the host check, the device can still reject with `device rejected install: STATUS_INCOMPAT — framework-map-version mismatch` — and the existing PAPK on flash is left untouched.

### Driving the simulator headlessly

`scripts/sim-remote.sh` runs the simulator under a virtual X server so you can drive the UI over SSH, from a browser, or from a script — no local display required. It spins up Xvfb + x11vnc + noVNC and then runs `sim.sh`; **every flag passes straight through to `sim.sh`**.

```bash
./scripts/sim-remote.sh --board pico_enviro_mon --app picoenvmon
```

It picks a free X display and free VNC/web ports, then prints a banner with a noVNC URL (`http://localhost:<port>/vnc.html?...`) and the control-FIFO path. You need `Xvfb`, `x11vnc`, and `websockify` installed; `xdotool` is strongly recommended for keyboard focus.

**Control channel (no browser needed).** The script creates a FIFO at `/tmp/picodroid-sim-remote-<display>-ctrl` and exports it as `PICODROID_SIM_CTRL_FIFO`. Write verbs to it to inject button input:

```bash
echo 'tap B' > /tmp/picodroid-sim-remote-<display>-ctrl
```

The `<display>` number is chosen dynamically (`:99..:150`), so rather than look it
up, use the `scripts/sim-ctrl.sh` wrapper — it auto-discovers the running sim's
FIFO and forwards the command:

```bash
./scripts/sim-ctrl.sh tap B          # auto-discovers the single running sim
./scripts/sim-ctrl.sh -d 100 tap B   # or target display :100 explicitly
```

The verb grammar is `down|up|press|tap <button>`:

- `down` — press (active-low falling edge)
- `up` — release (rising edge)
- `press` / `tap` — press, wait 40 ms, release (one clean tap)

Button tokens are the silkscreen names `A`/`B`/`X`/`Y` (1st/2nd/3rd/4th declared button), the semantic names `PREV`/`UP`, `NEXT`/`DOWN`, `ENTER`/`OK`/`SELECT`, `ESC`/`BACK`, or a bare GPIO pin number. An unknown verb prints `[sim] control channel: unknown command '<x>'`; an unknown button prints `[sim] control channel: unknown button '<x>'`. At startup the sim prints a ready banner listing the accepted commands. (The control channel is present on `has_buttons` and `has_touch` boards.) For the navigation model these buttons drive, see [Button navigation](/guides/button-navigation/).

The control channel also accepts the same Android verbs as hardware [`pdb input`](#injecting-input-pdb-input) — `input keyevent <KEYCODE|n>`, `input dpad <dir>`, `input back`, `input tap <x> <y>`, and `input swipe <x1> <y1> <x2> <y2> [ms]`. Prefer these when you want one vocabulary that works identically in the sim and on a real device: rehearse a sequence headlessly in the sim, then run the exact same verbs via `pdb input` over USB CDC.

**Capturing frames.** The sim window is named `picodroid`. Grab it with `scrot`:

```bash
DISPLAY=:<n> scrot --window "$(xdotool search --name picodroid)" out.png
```

**Logs.** Infrastructure logs go to `/tmp/picodroid-sim-remote-<display>-{xvfb,x11vnc,novnc}.log`. App `println!` and lifecycle output land in the `sim-remote` log.

**Cleanup.** The script TERM-then-KILLs its subtree on exit and removes the FIFO and X locks. To kill a stray sim manually, match the **exact** process name:

```bash
pkill -x picodroid
```

Do **not** `pkill -f sim-remote.sh` — that pattern also matches the launching shell and self-kills it.

**xdotool pacing gotcha.** minifb misses fast clicks at 60 Hz. When scripting mouse input with `xdotool`, never use `click 1` — instead do `mousedown` / `sleep 0.3` / `mouseup`. The control-FIFO `tap` verb embeds its own 40 ms press-release gap for the same reason, so a `PRESSED` frame renders before the release and the two edges land in distinct ticks.

## See also

- [Embedded gotchas](/guides/embedded-gotchas/) — GC lifetime rules, memory constraints, and the traps behind most of the playbooks above.
- [Troubleshooting](/guides/troubleshooting/) — symptom-keyed fixes for build, flash, sensor, and `pdb install` errors.
- [Limits](/reference/limits/) — heap sizes, object/string/array caps, and other hard ceilings.
