---
title: "Release notes"
description: "User-facing changes for Picodroid v0.4.0 onward."
---

This page covers everything that landed in releases v0.4.0 through v0.13.0. Earlier history is in `git log v0.1.0...v0.3.0`.

## v0.13.0 — 2026-08-19

The networking-maturity release. Sockets stop failing with an uncatchable JVM-internal error and start throwing the `java.net` exceptions Android apps already catch; `HttpURLConnection` grows the header and timeout surface real REST work needs; and `picoenvmon` becomes a WiFi showcase on a new Enviro+/Pico 2 W board — live web dashboard, NTP-anchored clock, internet weather. Underneath, a long device soak turned up an SMP heap-corruption family in the JVM (now fixed), and the memory work that followed cut picoenvmon's live heap by a third.

**Source-incompatible: net methods declare their checked exceptions**

- `Socket.send`/`recv`, `ServerSocket(int)`/`accept`, `DatagramSocket(int)`/`send`/`receive`, and the `HttpURLConnection` surface now declare `throws IOException` and friends, matching `java.net`'s contracts. **Code that called these without handling `IOException` stops compiling** — catch it or declare it. `Socket()` stays undeclared, like `java.net.Socket()`. The in-repo examples were updated in the same change (`http_get` previously had no catch at all: a DNS failure was an uncatchable app-kill).

**Networking — typed errors, headers, timeouts**

- Every net native now surfaces failure as the typed exception Android expects, with Android's wording: `ConnectException`, `SocketTimeoutException`, `NoRouteToHostException`, `BindException`, `UnknownHostException`, `SocketException`, `ProtocolException`. Previously a failed connect aborted the app with `InvalidReference`, a JVM-internal error no `catch` can reach; that error is now reserved for malformed native arguments. `Socket.recv` returns `-1` only at orderly EOF. The full mapping is the error-handling table in the [networking API reference](/api/networking/).
- The JVM knows the `java.net` hierarchy natively, so a native-thrown exception with no classfile behind it still matches `catch (IOException)` / `catch (Exception)` exactly as on Android — including real Java's quirk that `SocketTimeoutException` extends `InterruptedIOException`, not `SocketException`.
- `HttpURLConnection` gains request and response headers — `setRequestProperty` / `addRequestProperty` / `getRequestProperty`, `getHeaderField` by name and by index, `getHeaderFieldKey`, `getResponseMessage`, `getErrorStream`, and the `HTTP_*` status constants — which is what an auth token, a content type, or reading `Location` back needs. Response headers land in a bounded table; header count and sizes are capped like the `SharedPreferences` limits, since the memory comes out of the same shared heap as everything else.
- `HttpURLConnection.setConnectTimeout` / `setReadTimeout`, with Android's semantics (`0` = infinite, negative throws `IllegalArgumentException`, expiry throws `SocketTimeoutException`). Before this there was no timeout anywhere on the HTTP path, so a server that accepted the connection and then stalled hung the calling thread for good.
- New: `InetAddress.getByName(String)` — the first DNS entry point, throwing `UnknownHostException`; dotted-quad literals resolve without touching the network. And `ServerSocket.setSoTimeout(int)`.
- **`vendor/freertos-plus-tcp` now points at the picodroid fork.** Connecting to a reachable host with a closed port froze the app forever: upstream v4.4.1's RST handler moves a `SYN-SENT` socket to `eCLOSED`, but `vTCPStateChange()` sets no event bit for that transition, and `FreeRTOS_connect` sleeps with an infinite block time. **Existing checkouts must run** `git submodule sync && git submodule update --init vendor/freertos-plus-tcp`; the device build fails early with instructions if the unpatched upstream is detected. (Upstream PR #1355 fixes the same defect from the RFC-793 direction — we drop the patch and take theirs once it reaches a release.)
- Hardware WiFi: link-up is gated on full association instead of a join-*in-progress* state, so DHCP no longer starts on a link that cannot carry frames and `net: down` stops flapping through every retry. The cyw43 host-wake IRQ on GP24 replaces the 100 ms poll as the RX path (26 IRQ wakes per boot, DHCP 4 s faster); the poll is now a 1 s safety net. The RP2350 TRNG feeds `xApplicationGetRandomNumber`. `PICODROID_WIFI_AUTH` picks `open` / `wpa2` / `wpa3` / `wpa2wpa3` at build time — WPA3 is plumbed through to the driver's SAE path but untested against a real AP.
- Socket and HTTP handle tables unified on one slot-reusing implementation, fixing two defects: the 64-bit tables never reused slots (a create/close loop exhausted them after ~31 sockets), and the 32-bit arms handed Java the raw pointer with a no-op remove, making close-then-use a dangling dereference into the network stack. A stale handle now throws `SocketException("Socket is closed")`.

**New board: `pico_enviro_mon_w` — Enviro+ Pack on Pico 2 W**

- The first board combining sensors and networking (same wiring as `pico_enviro_mon`; the CYW43 pins are on-module). FreeRTOS+TCP's buffer tunables became `#ifndef`-wrapped defaults that optional `net_*` keys in `board.toml` override per board — this board halves descriptors, TCP buffers, and window segments, because the stack shares the 416 KB heap with the JVM and serves one connection at a time. Measured: +5.1 KB static RAM for the network stack, ~20 KB static headroom on a release build.
- `picoenvmon` on it is the showcase: a live web dashboard on port 8080 (five smoothed readings, IP, uptime, 2 s meta-refresh), an NTP-anchored wall clock (a single RFC 4330 exchange against pool.ntp.org, re-syncing every 6 h) that puts real `HH:MM` stamps on history rows, sample dialogs, and alert lines, and an internet weather row from wttr.in refreshed every 15 min. All three are strictly fail-soft, and CI never asserts on internet-dependent content. The Network screen gained a Refresh action; on WiFi-less boards the screen stays reachable and explains itself.
- New `SystemClock.setCurrentTimeMillis` (Android's real API) anchors `System.currentTimeMillis` to epoch time — reads stay monotonic-driven, and leaving it unset preserves the historical count-from-boot behaviour.
- **Behaviour change on both Enviro+ boards:** `SensorLoggerService` now starts at boot. The device is an environmental monitor, so logging (and the IAQ LED) default on — a freshly flashed board serves live data unattended instead of reading `--` until someone opens Live and flips the Logger switch. The switch remains the off-toggle.

**JVM correctness**

- **Category-2 arguments occupy two local slots, per JVMS.** Classfile local indices count a `long`/`double` as two slots, but frames packed arguments one per slot — so any Java method with a parameter *after* a `long` or `double` read garbage. Never hit before because the tree had no such method until `TimeFormat.floorDiv(long, long)`, which killed the network thread with `InvalidBytecode`.
- **Every `StringBuilder` gets its own buffer.** All instances shared one global buffer stack, so two builders alive at once interleaved their bytes — and across threads it was silent corruption rather than a visible error.
- `String.getBytes()` and `new String(byte[])` — the `byte[]`↔`String` bridge that network code was hand-rolling in both directions.
- **SMP heap corruption fixed.** FreeRTOS SMP yields whenever an unblocked task's priority is ≥ the running task's, and `configUSE_TIME_SLICING = 0` only disables tick round-robin, not that. Two equal-priority JVM tasks could therefore interleave inside an arena resize or inside GC scratch growth — the single root cause behind a whole family of soak failures (slice-OOB compaction panics, child-thread `InvalidReference` death, rooted objects swept, permanent GC thrash). `gc::collect` and every compound heap mutation now run inside a scheduler-atomic section; measured cost on the device benchmark is 0.81%, and it is a no-op on the host.
- GC could not see a parked task's frames, so a thread blocked in `sleep` or `accept` had its frame locals swept by any collection another task triggered. Collection now walks a registry of every executing stack.
- **Native allocations count toward GC pacing.** The threshold counted only bytecode allocation opcodes, so a workload allocating mainly through natives and builtins (`getBytes`, `toString`/format interning, sensor events) accumulated KB of garbage with the counter near zero — then OOM'd when a table-growth step needed tens of contiguous KB. Each heap now counts its own allocation events at the source.
- `Thread.start` children and background-pool workers **share one loaded class set** instead of each building a private JVM and re-parsing every class. picoenvmon's single network thread was priced at ~13.9 KB of duplicate metadata; child spawn latency drops too.
- Offensive memory-diagnostics mode now actually arms on device — it was silently sim-only.

**Memory**

- **Packed `byte[]` / `boolean[]` storage** — 1 byte per element instead of a full 4-byte arena slot, with 32 bytes of inline reach instead of 8. Semantics are unchanged (`bastore` already truncated, loads sign-extend). Measured on picoenvmon: byte payloads 12,428 → 3,921 B (−68%), 27 of 33 byte arrays fully inline, total live heap 21.4 → 13.8 KB. `char[]`/`short[]` are a noted follow-up.
- New `heapcensus` diagnostic answers *who holds the bytes right now*, where the existing histogram only counted churn: live bytes and counts per class, arrays by element type (inline vs arena, dead/slack), dyn-string length vs capacity, the `ArrayList`/`HashMap`/`StringBuilder`/exception side tables that `live=` never counted, and per-executor class-metadata cost. Background-thread GCs are finally visible in `gc=`, and a new `gcb=` column reports bytes reclaimed per window. See `docs/memory-diagnostics.md`.
- picoenvmon's dashboard serve path is allocation-free (constant response heads and page framing cached as `byte[]`, byte-level assembly into a persistent buffer), which let `gc_alloc_threshold` go back up to 128 on the W board: perfbench composite 1018 → 792 (lower is better), GCs 240 → 119, GC time 1424 → 768 µs. New `prereserve_arena8_bytes` tunable, and a prereserve retune for the packed-arena era.

**Flash**

- **`framework_class_excludes`** — every compiled SDK class ships on every board and is loaded at boot, so a new class costs its full size in flash whether an app uses it or not, and the RP2040 program region had 1,585 bytes left. A board can now drop classes it can never use via an optional top-level key in `board.toml`. An exclude matching no compiled class fails the build, so a typo cannot silently keep shipping it. `testbench_rp2040` uses it to leave out `picodroid.net.*` except `NetworkInfo` (~9 KB). Note the split this introduces: on a board that merely lacks networking a socket call **throws** `UnsupportedOperationException`; on one that also excludes the classes it fails to **resolve**. Probing with `NetworkInfo.isConnected()` and degrading is the portable pattern.
- All seven `Arrays.sort` primitive overloads, the GC's arena compaction, and the touchscreen median filter now share a single `u64` sort instead of monomorphising Rust's generic sort per element type — each element type maps onto an order-preserving key (sign-bit flip for integers, the IEEE-754 total-order transform for floats). Sort machinery: 35,092 → 7,386 bytes across 34 → 5 instantiations.
- Together those take the RP2040 image from 915,663 to 881,875 bytes — from 1,585 bytes of headroom to ~35 K.

**Simulator & tooling**

- `SystemClock.sleep` blocks through the kernel. As a bare `std::thread::sleep` it was invisible to the FreeRTOS POSIX port, so with time slicing off the sleeping task stayed Running and an equal-priority sibling never got a yield point — `threaddemo`'s second thread starved without executing a single instruction. `Thread.start` also no longer swallows a declined spawn.
- Sim socket read timeouts actually expire now. The scheduler's 1 ms `SIGALRM` `EINTR`s every blocking read and the naive retry restarted `SO_RCVTIMEO` from scratch, so a timeout could never elapse; `tcp_recv`/`udp_recvfrom` track an explicit deadline and `tcp_accept` emulates one with a nonblocking poll. The device side normalises FreeRTOS+TCP's inverted encoding to the same contract (`Ok(0)` = orderly EOF, timeout is an error).
- `build-apk.sh` passes the shrink flag as a per-invocation Gradle property. It relied on an env fallback that a long-lived Gradle daemon freezes at start, so a daemon left behind by a `--shrink` run silently shrink-stamped every later PAPK, which then failed `pdb install` against no-shrink firmware. Latent until v0.12.0 shipped the first real shrink map.
- Permanent info-level logging for key dispatch and activity transitions — pdb injection receipt (keycode → pin), per-edge dispatch outcome, BACK dismissal, activity push/pop by class — plus a warning on the previously silent 64-slot key-queue overflow.
- Nightly coverage: a `sim`-only test category for rows the HIL board cannot run, with a per-row board override; a `netexception` row asserting the typed-exception taxonomy in both shrink modes; and a `pico_enviro_mon_w` dashboard smoke lane. Network test targets are baked in at build time (`-PpicodroidNetTestHost`), so pointing `netdemo`/`http_get` at a real machine is an env var rather than a source edit.
- google-java-format 1.35.0 → 1.36.1.

Shrink map: **stable — byte-identical to v0.12.0** (135 classes). Everything new this release landed as methods on classes the v0.11.0 cut already named.

## v0.12.0 — 2026-08-14

The networking release. WiFi on the Pico 2 W goes from "compiles" to validated end-to-end on hardware, the host simulator starts running the real FreeRTOS kernel, and runtime flash writes (the `pdb install` path) are fixed on both chips. The Java SDK surface is unchanged — everything here is framework, platform, and tooling work behind the existing API.

**Networking — WiFi works on hardware (Pico 2 W)**

- The full `picodroid.net` stack (TCP/UDP sockets, `HttpURLConnection`) now runs end-to-end on `testbench_rp2350w`: WPA2 join in ~6 s, DHCP lease shortly after, TCP echo and HTTP GET/POST validated against LAN hosts. The API itself is unchanged — it now works on the device instead of only under the simulator's host stack.
- The cyw43 gSPI transport was rewritten in Rust on PIO + DMA (PIO0, 37.5 MHz), replacing the vendored bit-bang C transport, and the WiFi task runs on core 1 — leaving core 0 to the JVM.
- `vendor/cyw43-driver` now points at the picodroid fork. **Existing checkouts must run** `git submodule sync && git submodule update --init vendor/cyw43-driver`; the device build fails early with instructions if the unpatched upstream is detected.
- WiFi credentials are baked in at build time via `PICODROID_WIFI_SSID` / `PICODROID_WIFI_PASS` — see the new [WiFi & networking setup guide](/get-started/networking/). On hardware, poll `NetworkInfo.isConnected()` before the first socket call; `netdemo` and `http_get` show the pattern (an app's `onCreate` races the WiFi join + DHCP window).
- Current limits are collected on the new [known issues](/reference/known-issues/) page: open/WPA2-AES networks only, no TLS, 256-byte socket I/O chunking, coarse HTTP error reporting.

**Breaking: ESP32-S3 support removed**

- The compile-only ESP32-S3 / Lilygo T-Deck Plus target (Milestone-1 scaffolding from v0.9.0) is gone: `platforms/esp/`, the `tdeck_plus` board, its cargo aliases, and its docs pages. Retrieve `platforms/esp/` from git history if it returns. Supported boards are now the four RP-family ones (Pico, Pico 2, Pico 2 W, Pico + Enviro+ pack).

**Simulator — the real FreeRTOS kernel**

- The host simulator now compiles and runs the actual FreeRTOS kernel (POSIX port) in-process instead of approximating it with host threads. `Thread.start()` spawns a real task with the device's 16 KiB stack charged against the simulated heap, `Executors.backgroundExecutor()` runs on the device's four `jvm-bg` worker tasks, `synchronized` uses kernel recursive mutexes, and `threaddemo` now runs — and is asserted — under the sim.
- Remaining gap, documented in the [simulator guide](/get-started/simulator/): the POSIX port is single-core, so cross-core races remain hardware-only.

**Runtime flash writes fixed on both chips**

- RP2040: install-time flash writes no longer hang. Three stacked causes fixed — core-1 execution in the XIP-off window (a core-1 parker task now covers it), a FreeRTOS scheduler-configuration deadlock, and a per-core VTOR defect. `pdb install` now works on the original Pico.
- RP2350: a recurrence (core 1 taking an interrupt inside the XIP-off window) fixed by extending the core-1 parker; regression-verified across the full HIL suite.

**Tooling**

- New `pdb input` — Android-faithful synthetic input over USB: `keyevent`, `dpad`, `back`, `tap`, `swipe`, resolved against the board's button table on-device. The sim control channel accepts the same `input …` verbs, so an input sequence rehearsed headlessly replays verbatim on hardware. See the new [pdb command reference](/reference/pdb-commands/).
- `pdb --help` now prints the real command set (including `input` and `logcat`); `pdb sysmon` prints the JVM block after the task table; `papk-info` labels ≥ 1 MiB sizes correctly.
- Flash-size reporting is honest: builds report usage against the linker's *program region* (an RP2040 `--release` image is 99% full, not the previously reported 43% of chip total), and RP2040 release builds automatically drop LTO to fit the 896 K ceiling — build via `scripts/build.sh`, not raw `cargo build --release`, on RP2040.
- `hil-run.sh` derives the probe chip from the board and can now drive the RP2040 testbench.
- PAPKs are structurally validated at embed time — a corrupt `PICODROID_APK_PATH` fails the build with a clear message instead of failing mysteriously at install.

**Framework & robustness**

- `NotificationManager.notify` / `cancel` are implemented (previously stubs).
- Button GPIO edges get a 5 ms per-pin dead-time debounce; all sensor I2C moved to a dedicated FreeRTOS sampler task shared by sim and device.
- Keypad bursts are no longer misdelivered across Activity transitions; widget listener maps unregister on `LV_EVENT_DELETE`; picoenvmon threshold alerts are edge-detected (one log line per alert edge instead of ~13/s at idle).
- New `[jvm] prereserve_*` board tunables pre-reserve steady-state heap storage at app start to curb navigation-churn fragmentation — see [JVM tunables](/reference/jvm-tunables/).

**Internal architecture**

- The platform-agnostic framework moved into the `picodroid-core` crate (JVM natives, lifecycle, graphics, networking, sim HAL); the PAPK container format and the PDB wire protocol became the `papk-format` and `pdb-protocol` crates — each a single source of truth shared by firmware, simulator, and host tools. A pre-commit shadow-twin guard keeps the trees disjoint.
- The 2026-07 code-health audit closed its P0/P1 backlog: clippy now gates all host tools and every board (ARM targets included), CI enforces the shrink-map append-only invariant and the widened LVGL constant drift guard, and a generation-tagged widget handle table is staged behind the default-off `handle-table-32` feature for 32-bit targets.

Shrink map: **stable — byte-identical to v0.11.0** (135 classes). The networking, simulator, and extraction work all landed outside the `sdk/java` framework surface.

## v0.11.0 — 2026-07-20

The memory-diagnostics and Android-parity-completion release. Folds in a large SDK surface expansion (widget completion, Android-cased renames, package moves), a JVM correctness pass, and a full opt-in memory diagnostics suite built to make heap growth and steady-state churn visible in both the simulator and on real hardware.

**Android parity — widget completion & renames**

- `AlertDialog` moved to `picodroid.app` (matching `android.app.AlertDialog`); `IBinder` moved to `picodroid.os`; `Url`/`HttpUrlConnection` renamed to Java's `URL`/`HttpURLConnection` casing; `Preferences` became `SharedPreferences` with Android's full get/edit/commit idiom.
- New widgets: `RadioButton` + `RadioGroup` with mutual exclusion, `NumberPicker` with keypad edit mode (replacing the picoenvmon Settings keyboard entry), `TextWatcher` with `afterTextChanged` on `EditText`, `GestureDetector.SimpleOnGestureListener`, the standard interpolator family (`Linear`/`Accelerate`/`Decelerate`/`AccelerateDecelerate`) plus animation end actions, `View.OnLongClickListener` + `performLongClick`, `AdapterView.OnItemSelectedListener`, view-relative `MotionEvent.getX/getY` and screen-absolute `getRawX/getRawY`.
- Rounded out: `AlertDialog` neutral button (Android's 3-slot layout) and single-/multi-choice list variants, `SeekBar` press-edge tracking callbacks, `Service.onRebind`/`stopSelfResult`, `startActivityForResult` with Android's result-delivery order, `Activity.getIntent()`, the `onRestart` lifecycle callback, `View.setId/getId/setTag/getTag`, full `View` property getters, `picodroid.view.Gravity`, full `IME_ACTION_*`/`InputType` constant sets, Android sensor `TYPE_*`/`SENSOR_STATUS_*` constants, `DialogInterface.BUTTON_NEUTRAL`, `Log` severity ladder + `Throwable` overloads.
- An `android.*` import-compatibility layer (stub jar + class-shrink alias rewriting) was landed, then reverted a few commits later — apps still import `picodroid.*` only; see the compat matrix notes in `docs/`.

**JVM correctness**

- `getClass()` no longer mints a fresh `Class` object per call after the first string concat (was breaking identity comparisons); `Class.getName()` returns Java's dot-form; the builtin `Throwable` hierarchy now matches for `catch`/`instanceof`; clinit throws are wrapped in `ExceptionInInitializerError`; `Throwable.addSuppressed`/`getSuppressed` now store/return.
- `Object.clone()` shallow copy + `Cloneable` marker, `Object.getClass()` with `ldc`-literal identity, `java.util.Comparator` + `Collections.sort(List, Comparator)`, `Integer.parseInt` family, boxed `Byte`/`Short`, full-contract `System.arraycopy`.
- `StringBuilder.append(char)` no longer scrubs `\n` to a space (was breaking `\n`-joined strings passed to native code).
- 32-bit-clean object layout: fields arena + 12-byte slots everywhere, closing the last 64-bit assumption in object layout.
- Fixed `MethodNotFound`/sensor-dispatch spam caused by `class_table` and `Intent` target-class names aliasing a GC-freed `dyn String`; both now canonicalize at the native boundary.

**Robustness**

- The `Display` singleton is now a GC root (was being swept and slot-reused, breaking all navigation with a post-first-GC `NoSuchMethod`).
- A view's animations are canceled when the view is deleted; a soft keyboard unbinds from its textarea on delete; consumed `onTouch`/long-press now correctly suppress the synthetic click.
- New handle use-after-delete sanitizer for the simulator (`--sanitize-handles`) and a method-class cross-check test against the native dispatch registry.

**Memory diagnostics (new)**

- Opt-in `--mem-diag` monitor: `[memmon]` heap-growth sentinel, per-class allocation histogram (`PICODROID_MEMDIAG_HISTO`), offensive heap checks (`PICODROID_MEMDIAG_OFFENSIVE`), and a `pdb` `CMD_SYSMON` extension that pulls the live JVM heap block over USB.
- Plugged the input-driven heap leaks the new diagnostics surfaced: recycled `KeyEvent`/`MotionEvent` give zero-alloc steady-state key and touch dispatch; sensor delivery is now allocation-free with an emergency GC at the native boundary; runtime flash writes now restore fast XIP mode afterward.
- Killed JVM string-churn copies via an `intern_dyn_owned` handoff and format-scratch reuse.
- New steady-state flatness test, a soak-test harness (`scripts/test-memdiag.sh`), dedicated CI lanes, and a full guide at `docs/memory-diagnostics.md`.

**Simulator ↔ MCU parity**

- The simulator now models the device heap for real: a `heap_4` arena, a default heap cap, a flash-modeled APK, and boot pre-charge — closing most of the sim/hardware memory-behavior gap. Parity-strict `Thread.start`, parity-metrics execution counters, and a parity-bench ratio tracker round out the harness; see `docs/parity-audit.md`.
- Fixed the host-only minifb window buffer being wrongly charged against the simulated heap cap (was causing spurious OOM at low `-l` limits).

**picoenvmon polish**

- History now shows recorded data with a clearer empty state; Settings moved from soft-keyboard entry to `NumberPicker` steppers; several layout/clipping fixes (Live/Settings tile spacing, Logger/Units switch knob, ListView focus highlight, Settings hint truncation); Back is disabled on the home hub so Y is the only exit.

**Tooling**

- Fixed a `class-shrink` short-name allocator bug found while cutting this release's map: two unrelated classes (`picodroid.os.IBinder`, `picodroid.text.InputType`) could be assigned the identical shrunk name when the raw-index allocator crossed a skipped Java-reserved-keyword boundary (`"do"`/`"DO"`) — the per-call skip-ahead wasn't reflected in the caller's counter. Fixed by threading a single shared raw-index counter through the allocator and deriving each release's starting index by inverting existing entries' shrunk names rather than trusting the map's entry count.
- Error Prone enabled as a default bug net (plus `@Override` enforcement); CI now caches Rust/Gradle builds, compiles all example apps, and runs sim smoke on every push; nightly failure emails now diff against the previous run.

Shrink map: **+25 classes (110 → 135)** — see the [shrinker reference](/reference/shrinker/) for the full per-class breakdown; v0.10.0 entries copied verbatim.

## v0.10.0 — 2026-06-02

The Android-parity release. Folds in the typed-listener, adapter, and focus-navigation surface that had been accumulating on `main` since v0.9.0, plus a wave of JVM heap and garbage-collector fixes that keep long-running, callback-driven apps alive.

**Android parity**

- **Typed listener interfaces (Tier 1)** and the **`Adapter` pattern (Tier 2)** land as first-class developer surface: `ViewGroup` + `ViewGroup.LayoutParams`, `Adapter` / `AdapterView` / `ArrayAdapter` / `BaseAdapter`, `CompoundButton`, and `DialogInterface`. Listener interfaces now match `android.*` shapes — `View.OnClickListener` / `OnFocusChangeListener`, `AdapterView.OnItemClickListener`, `CompoundButton.OnCheckedChangeListener`, `Spinner.OnItemSelectedListener`, `SeekBar.OnSeekBarChangeListener`, `DatePicker.OnDateChangedListener`, `TimePicker.OnTimeChangedListener`, `SwipeRefreshLayout.OnRefreshListener`, `Keyboard.OnReadyListener`.
- `ArrayAdapter` now renders correctly — `Object.toString()` resolves through the JVM, so adapter-backed `ListView`s show real item text.
- **Context constructors + `Display` cleanup (Tier 4)** round out the parity work.

**Keypad & focus navigation**

- New **View focus API** (`setFocusable` / `requestFocus`) backed by per-Activity LVGL focus groups, plus real **D-pad item selection in `ListView`**. This is what makes button-only devices (no touchscreen) fully navigable.
- `AlertDialog` is now keypad-dismissable (BACK cancels, ENTER confirms) and is torn down whenever its Activity leaves the foreground — no more leaked dialogs.

**JVM & runtime**

- `invokestatic` now walks the superclass chain per JVMS §5.4.3.3.
- **Garbage-collector fixes for callback-driven apps:** Views and dialogs referenced only by native listener maps (key / touch / click / dialog) are now GC roots, fixing input that died ~15 s into a session. Also plugs a native-state root leak and a GC-starvation path.
- **Heap shrink:** `helloworld` peak heap drops 51 KB → 25 KB via a `JvmObject` layout rework (single `Box<[Value]>` field store, `class_idx` side table, tightened layout guard). New **chunked-slot heap storage** plus an RP2350 heap bump 384 KB → 416 KB.
- Past JVM optimisations are now tunable from a board's `[jvm]` `board.toml` section.

**Robustness**

- Bad-APK and poisoned-mutex paths log and early-return instead of panicking.
- A covered Activity no longer receives `onServiceConnected` (fixes a stale bound-service use-after-free) and has its dialogs dismissed when pushed under another Activity; further stale-view UAF and duplicate-launch hardening.

**picoenvmon showcase**

- Pimoroni **Pico Enviro+ Pack** bring-up — display plus I2C BME688 / LTR559 sensors.
- Redesigned to a hub-menu **4-button navigation** model (A=up / B=down / X=open / Y=back), smoothed `HomeActivity` to 1 Hz via a bound service, and fixed the sensordemo "1 event then silent" phantom-IRQ bug.

**Tooling, simulator & docs**

- The simulator now **emulates the physical buttons** via the keyboard plus a headless control channel, runs the real XPT2046 touch driver, and synthesizes BME688 / LTR559 readings instead of zeros.
- New `perfbench` (unified speed + memory) and `graphicsbench` (LVGL render pipeline) benchmarks, each with a composite SCORE.
- Documentation migrated to an **Astro Starlight** site, with a central reference page for the `[jvm]` tunables. Example apps coalesced 59 → 51.

Shrink map: **+23 classes (87 → 110)** covering the Tier 1/2 listener and adapter surface; v0.9.0 entries copied verbatim.

## v0.9.0 — 2026-05-06

The largest release yet. Bundles the licensing, multi-family, and lifecycle work that had been accumulating on `main` since v0.8.0.

**Licensing**

- Project relicensed Apache-2.0 → **GPL-3.0-only** (no Classpath Exception). Shipped a [Contributor License Agreement](/project/cla/) (Harmony FLA-style) and a dual-licensing framework — see [Licensing](/project/licensing/) for details.

**Multi-family architecture**

- `platforms/<family>/` directory replaces the flat `src/hal/<family>/` layout. RP code now lives under `platforms/rp/`; ESP scaffolding lives under `platforms/esp/`.
- New `picodroid-core/` workspace member holds cross-family shared code (no HAL imports).
- HAL CONTRACT v1 — the required public-symbol set every family must expose — is documented in `platforms/rp/src/hal/mod.rs` and compile-time enforced via `platforms/rp/src/hal/contract.rs`.
- Build pipeline generalized via `build_support/{config,freertos,network,boards}.rs` for shared path resolution.

**ESP32-S3 / Lilygo T-Deck Plus (M1)**

- First Xtensa target lands as **Milestone 1** — compile-only. The firmware produces a valid `xtensa-esp32s3-none-elf` ELF and flashes via `espflash`, but FreeRTOS, networking, display, and the LVGL stack are no-ops at this milestone. *(ESP32-S3 support, including its quickstart/toolchain pages and cargo aliases, was removed in 2026-07 — retrieve `platforms/esp/` from git history if needed.)*

**Lifecycle and dispatch**

- `Activity` now bootstraps the `Display` singleton **before** `onCreate()`, eliminating a class of null-pointer dereferences in app code that touched the display in `onCreate`.
- `pdb install` no longer panics when the running app never starts an Activity (e.g. a `blinky`-style LED loop).
- `main_queue` splits tick coalescing from cross-task wakes, reducing wakeup latency on busy frames.

**LVGL**

- Bumped 9.2.2 → 9.5.0 (already in v0.6.0; v0.9.0 enables `LV_DRAW_SW_SUPPORT_RGB565A8` on top, fixing aliased rendering for `ImageView.setScaleType` / `setScale`).

**Build & CI**

- `.actrc` lets `act` run the GitHub Actions workflows locally — see [Advanced configuration → .actrc](/reference/advanced-config/#actrc).
- macOS toolchain hardening: switched off the broken `gcc-arm-embedded` cask onto the formula; fixed `libudev-dev` and absolute APK path issues for HIL testing.

Shrink map: byte-identical to v0.8.0 (no new framework classes).

## v0.8.0 — 2026-05-02

**PAPK 1.1 — bundled image assets.** PAPKs gained an `ASST` section that carries pre-decoded PNG images as LVGL-native RGB565 structures mapped to XIP flash. `ImageView.setImageSource("foo.png")` becomes a name-keyed lookup with no on-device PNG decoder. See [Bundled image assets](/guides/assets/) and the new `imagedemo` example.

`papk-pack` and `papk-info` learned the asset table; the runtime resolver registers assets at boot via LVGL's image cache.

Shrink map: byte-identical to v0.7.0 — bundled assets land outside the framework class set.

## v0.7.0 — 2026-05-01

**Tier C widget framework.** Five new widgets (and one new listener) ship in this release:

- [`Snackbar`](/api/ui/#picodroidwidgetsnackbar) — toast with a clickable action lozenge.
- [`DatePicker`](/api/ui/#picodroidwidgetdatepicker) — `lv_calendar` binding.
- [`TimePicker`](/api/ui/#picodroidwidgettimepicker) — `lv_roller` binding, with 12-hour / AM-PM mode.
- [`SwipeRefreshLayout`](/api/ui/#picodroidwidgetswiperefreshlayout) — pull-to-refresh container.
- [`OnSwipeListener`](/api/ui/#picodroidviewonswipelistener) — per-View swipe-direction primitive.
- [`ImageView`](/api/ui/#picodroidwidgetimageview) gained `setScaleType` / `setTint` / `setScale`.
- [`ProgressBar`](/api/ui/#picodroidwidgetprogressbar) gained an indeterminate variant via `ProgressBar.indeterminate()` (`lv_spinner`).

Shrink map: 5 new entries (`a/CE`..`a/CI`); v0.6.0 entries copied verbatim.

## v0.6.0 — 2026-04-30

**Showcase release.** No new framework classes — the [`picoenvmon`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/picoenvmon) feature-showcase app and the LTR559 driver shipped this release. `picoenvmon` demonstrates the manual DI pattern (`ApplicationComponent` / `ActivitySingletonComponent`) in production-shape code.

Shrink map: stable, byte-identical to v0.5.0.

## v0.5.0 — 2026-04-29

**Soft-keyboard polish.** The system soft keyboard:

- Slides up from the bottom edge over ~150 ms when an `EditText` gains focus, and slides back down on dismiss.
- Forwards the OK key through a new `OnEditorActionListener` interface before its default close behavior runs.
- Dismisses on tap-outside.

Plus a new `EditorInfo` constants surface (`TYPE_NUMBER` / `TYPE_EMAIL` / `TYPE_PHONE` / `TYPE_PASSWORD` / `TYPE_TEXT`) for `EditText.setInputType`.

See [`EditText`](/api/ui/#picodroidwidgetedittext) and the polish notes under [`Keyboard`](/api/ui/#picodroidwidgetkeyboard).

Shrink map: 2 new entries (`OnEditorActionListener`, `EditorInfo`); v0.4.0 entries copied verbatim.

## v0.4.0 — 2026-04-27

**DI + Service framework (Preview).** Introduced the `picodroid.app.Service` lifecycle plus the manual DI components used by `picoenvmon`. New surface:

- [`Service`](/api/services/#picodroidappservice) — `onCreate` / `onStartCommand` / `onBind` / `onUnbind` / `onRebind` / `onDestroy`.
- [`IBinder`](/api/services/#picodroidosibinder), [`Notification`](/api/services/#picodroidappnotification-and-startforeground) (with `Notification.Builder`), and `startForeground(int, Notification)` for foreground services.
- [`ServiceConnection`](/api/services/#picodroidcontentcontext--start--bind--stop) for binding lifecycle.
- [Manual DI components](/api/services/#manual-di-applicationcomponent--activitysingletoncomponent): `ApplicationComponent`, `ActivitySingletonComponent`.

Also includes the `servicedemo` example which drives the full Service v1 lifecycle in one non-UI run.

Shrink map: ~10 new entries covering the DI + Service surface; v0.3.0 entries copied verbatim.

## Older releases

For v0.1.0–v0.3.0, see `git log` and the original `docs/` history. Highlights:

- v0.3.0 — `Theme`, gestures (`GestureDetector`, `OnTouchListener`), animations (`ViewPropertyAnimator`), dialogs (`AlertDialog`), `Toast`, `Keyboard`.
- v0.2.0 — `SensorManager` family (BME688), HTTP client, `KeyEvent` / `OnKeyListener`, `Executors` (main + background).
- v0.1.0 — first release cut: 42 framework classes covering peripherals, storage, basic widgets, the JVM core.
