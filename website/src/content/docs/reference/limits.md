---
title: "System limits & memory budgets"
description: "How much an app can do before it falls over: per-board RAM/flash, Java heap behavior, runtime caps, and idle sleep."
---

Picodroid runs your Java app inside a Rust JVM on an MCU with kilobytes — not gigabytes — of RAM. This page collects the hard ceilings and practical budgets so you can size an app before it falls over at runtime.

## Per-board memory budget

The MCU sets the ceiling. RAM and flash are the two scarce resources; everything below competes for them.

| Board | MCU | SRAM | Flash | Clock | Cores | FreeRTOS heap | LVGL buffer |
|---|---|---|---|---|---|---|---|
| `testbench_rp2040` | RP2040 (Cortex-M0+) | 256 KB | 2 MB | 125 MHz | 2 | 160 KB | 48 KiB |
| `testbench_rp2350` | RP2350 (Cortex-M33) | 520 KB | 4 MB | 150 MHz | 2 | 324 KB | 48 KiB |
| `testbench_rp2350w` | RP2350 (Cortex-M33) | 520 KB | 4 MB | 150 MHz | 2 | 324 KB | 48 KiB |
| `pico_enviro_mon` / `_w` | RP2350 (Cortex-M33) | 520 KB | 4 MB | 150 MHz | 2 | 324 KB | 48 KiB |
| `pico_display2_w` | RP2350 (Cortex-M33) | 520 KB | 4 MB | 150 MHz | 2 | 324 KB | 48 KiB |
| `pico_touch_kit` | RP2350B (Cortex-M33) | 520 KB | 16 MB | 150 MHz | 2 | 252 KB | 64 KiB |

Notes on the numbers:

- The SRAM figure is what the linker assumes, not the chip's physical total. RP2040 declares 256 KB (its four 64 KB main banks); the two 4 KB scratch banks are excluded, so the chip's 264 KB physical SRAM is reported as 256 KB. RP2350's 520 KB matches physical.
- The **FreeRTOS heap** (`configTOTAL_HEAP_SIZE`) is the single pool the JVM allocates from — see [How the Java heap works](#how-the-java-heap-works). It is single-sourced from the MCU TOML's `heap_kb` key (`mcus/rp/rp2350.toml`: 408 KB, `mcus/rp/rp2350b.toml`: 336 KB — the touch kit's draw bands came out of it, `mcus/rp/rp2040.toml`: 160 KB) and injected at build time; `FreeRTOSConfig.h` refuses to compile without the injection.
- On the RP2350 the code that runs from SRAM comes out of that figure, so the arena a device links is smaller than `heap_kb`: the interpreter loop's RAM copy takes `jvm_loop_ram_kb` (36 KB, MCU TOML) and the other hot sets take the board's `hot_ram_kb` (48 KB on every RP2350 board). 408 − 36 − 48 = 324 KB, and 336 − 36 − 48 = 252 KB on the touch kit; the table shows the linked arena. The RP2040 runs nothing from RAM and links the whole 160 KB. The simulator's heap cap keeps modelling `heap_kb`.
- On WiFi boards (`testbench_rp2350w`, `pico_enviro_mon_w`, `pico_display2_w`, `pico_touch_kit`), the networking stack shares that same arena: the FreeRTOS+TCP network buffers and the `cyw43` task stack are all allocated from it, so a WiFi build has correspondingly less Java-heap headroom. Heap-constrained boards shrink the stack's share with the `net_*` keys in `board.toml` — `pico_enviro_mon_w` and `pico_display2_w` halve the descriptor count and per-socket TCP buffers relative to the testbench defaults.
- **LVGL buffer** is the UI render pool (`lv_mem_kb`, default 64 KiB). Every board but `pico_touch_kit` overrides it, down to 48 KiB. The pool is a static array in `.bss`: on `testbench_rp2040` and the Enviro boards the trim leaves the JVM more of a tight budget, and on the RP2350 testbenches and `pico_display2_w` the 64 KiB default left the core-0 main stack at or under the 8,192-byte floor the linker script asserts. That is why those boards have a practical list-row cap (see [Runtime limits](#runtime-limits)). `pico_touch_kit` keeps the default size in SRAM; its `board.toml` can move the pool into the module's 8 MiB PSRAM (`lv_mem_in_psram`) and hand the 64 KB back to the JVM arena, at a measured frame-time cost, so the board ships with it off.

## How the Java heap works

There is no fixed "JVM heap size" constant on RP boards. The JVM allocates on demand from the global allocator, and on RP the global allocator **is** the FreeRTOS heap:

```rust
#[global_allocator]
static GLOBAL: FreeRtosAllocator = FreeRtosAllocator;
```

Every Java object, array, and string routes through `pvPortMalloc`, drawing from the single `configTOTAL_HEAP_SIZE` pool. So your effective Java heap is whatever of that pool is left after task stacks, queues, framework BSS, and LVGL take their share — practically a **160 KB pool on RP2040** and a **324 KB pool on RP2350** (252 KB on the touch kit), shared with everything else.

A few mechanics worth knowing:

- **One process-wide heap.** All JVM threads share a single `SharedJvmHeap` (objects, arrays, strings), matching the standard Java memory model. Background threads build their own interpreter state but allocate into the same shared pool.
- **No-op OOM hook.** When `pvPortMalloc` returns NULL, the malloc-failed hook is intentionally a no-op so Rust's `try_reserve_exact` can return `Err` and trigger a GC on the next interpreter step. Non-fallible allocations still abort — on a device, a board reset — which is why the JVM's own growth paths are fallible: object and array slot chunks report a failure the caller collects and retries on, and the method and field resolution caches simply stop memoising when the heap refuses (before 2026-09-15, a 10,240-byte cache doubling reset the RP2040 on a full heap).
- **Chunked slot allocator.** Object and array slot tables grow one fixed-size chunk at a time (`ChunkedSlots`) instead of doubling a single `Vec`. The default chunk is 64 slots (`slot_chunk_shift = 6`). This caps the worst-case contiguous request — single-digit KiB for most types, tens of KiB for arrays — so the FreeRTOS heap can satisfy growth even when fragmented. The doubling allocator it replaced once demanded a 90 KB contiguous block that the heap could not serve on `pico_enviro_mon`.

## Runtime limits

| Limit | Default | Overflow behavior |
|---|---|---|
| GC cadence | every 256 allocations | not an error — a collection runs |
| Activity stack depth | 8 | new Activity silently dropped; logged on host, no Java exception |
| Pending-op queue | 8 | the call throws `IllegalStateException`; the drop is logged |
| Background `Thread` stack | 16 KiB, core 0 | FreeRTOS task creation fails if heap is exhausted |
| Boxed collection entries | a few hundred (app guideline) | `OutOfMemoryError` from the collection's growth; the RP2040 reaches it first |
| PAPK install size | app region minus 4 KB: 1532 KB (`testbench_rp2350`, `pico_enviro_mon`), 1276 KB (the 4 MB WiFi boards), 10,236 KB (`pico_touch_kit`), 764 KB (RP2040) | rejected at install with `InstallError::TooLarge`; a smaller free run than the PAPK needs, even after compaction, is `NoRoom` |
| Installed apps | 8 (RP2350 boards, `max_installed_apps`), 16 (`pico_touch_kit`), 1 (RP2040) | a ninth package is refused with `NoRoom`; a reinstall of an installed package replaces it; system apps (the launcher) do not count |
| App storage path | 185 bytes (`sandbox::APP_PATH_MAX`); LittleFS allows 255 per segment | the operation fails: a predicate answers `false`, a write throws `IOException` |
| Storage volume | `fs_kb`: 512 KB (RP2350 boards), 4096 KB (`pico_touch_kit`), 128 KB (RP2040) | writes fail once LittleFS is full; every directory costs an 8 KB metadata pair; at boot every board removes the `/data/<package>` of any package not installed |
| App storage cap (multi-app boards) | `app_data_cap_kb`: a quarter of the volume (128 KB on the RP2350 boards, 1024 KB on `pico_touch_kit`); `fs_system_reserve_kb`: 64 KB kept for the system | a write past the cap or into the reserve throws `IOException`; `mkdir` answers `false`; `StatFs.getAvailableBytes()` says what is left |
| Assets per PAPK | 256 KiB (recommended) | not enforced — see below |
| Class file size | 65,535 bytes | the packer refuses the class (`class file over 65535 bytes`); every offset in a link table is a `u16` |
| Focusable list rows (small boards) | ~12 (app guideline) | render-pool stall, not a framework cap |
| Network buffer descriptors (WiFi boards) | 16 (`testbench_rp2350w`, `pico_touch_kit`) / 8 (`pico_enviro_mon_w`, `pico_display2_w`) | in-flight packets beyond the pool wait for a descriptor to free |
| Network MTU (WiFi boards) | 1500 bytes | larger frames are never carried |

Details:

- **GC cadence.** A collection runs after `gc_alloc_threshold` allocations (default 256) or on an OOM signal. Lower it to shrink the heap high-water mark, raise it to cut pause frequency — see [JVM tunables](/reference/jvm-tunables/).
- **Activity stack depth** (`activity_stack_depth`, default 8). Pushing past the cap returns soft (no `Result` threaded through JVM dispatch). The new Activity is dropped, the parked view is restored, and the app keeps running on the previous top. The framework **does** log this (host `eprintln!` / device `defmt::error!`), but it is never surfaced to Java as an exception. Raise the depth for deep modal/wizard flows.
- **Pending-op queue** (`pending_op_queue`, default 8). This FIFO holds lifecycle ops queued by `startActivity`, `finish()` and the Service calls (`startService`, `stopService`, `bindService`, `unbindService`). On a full queue the op is dropped, the log says `pending-op queue full, op dropped`, and the call that queued it throws `IllegalStateException` (`too many pending Activity/Service transitions in one frame (raise [jvm] pending_op_queue)`), so a dropped `finish()` is never silent. (This is distinct from the executor runnable queues backing `MainExecutor`/`BackgroundExecutor` — different queues.)
- **Boxed collection entries.** Every `Integer`, `Long` or other box in a `List`/`Map`/`Set` is a Java object on the shared heap, with a header and a slot table entry of its own and no escape analysis to elide it. On the RP2350's arena, framework included, a 5000-element boxed list or a 1200-entry boxed map does not fit; on the RP2040's 160 KB arena far less does, so an app that grows a collection past a few hundred boxes hits `OutOfMemoryError` there first. Size collections for the smallest board the app targets, prefer primitive arrays for bulk numeric data, and run any deliberate OOM probe last: the fragmentation it leaves disturbs later allocations (QA round 2026-09-13).
- **Background threads.** Each `picodroid.concurrent.Thread.start()` spins up one FreeRTOS task, pinned to **core 0** (required by the single-core safety assumption of the shared JVM state), with a **16 KiB stack** (the stack size is counted in words, not bytes — 4096 words × 4 = 16 KiB; do not read it as 4 KB). Priority maps from the Java thread's priority field, defaulting to `Thread.NORM_PRIORITY`. The simulator runs the same FreeRTOS kernel, so threads are real there too — single-core rather than dual-core, and without the core pinning. See [background services](/tutorials/background-service/).
- **PAPK install ceiling.** Installed apps share one *app region* of flash (`app_region_kb` in `board.toml`: 1536 KB on `testbench_rp2350` and `pico_enviro_mon`, 1280 KB on `testbench_rp2350w`, `pico_enviro_mon_w` and `pico_display2_w`, 10,240 KB on `pico_touch_kit`, 768 KB on `testbench_rp2040`), as contiguous runs of 4 KB sectors — a 4 KB boot-meta sector followed by the image. A PAPK larger than the whole region minus that sector is rejected outright with `InstallError::TooLarge`; one that would fit an empty region but not the free space left by the other installed apps is refused with `NoRoom` after the device has tried compacting the region (sliding the installed runs together). The device advertises the region's ceiling and its free space in the `pdb ping` greeting. See the [manifest reference](/reference/manifest/) and the [shrinker](/reference/shrinker/) for keeping small: `--shrink` renames every framework reference in the PAPK, and `--shrink-app` the app's own classes and members as well (about −19 % on `picoenvmon`'s stripped PAPK). Since PAPK v2 (2026-09-28) every class carries the link table the packer built for it, which made packed apps 19–30 % larger than the same app in v1 (`claudeusage`: about 99 KB → 118,004 B).
- **Installed apps.** The package directory holds `max_installed_apps` packages (8 on the RP2350 boards, 16 on `pico_touch_kit`); a package is identified by its manifest `package=`, and installing one that is already there upgrades it in place of the old copy rather than taking a second entry. Up to two system apps built into the firmware (today the launcher) sit beside them without taking a slot or a sector. RP2040 boards keep one app, which every install replaces, and no launcher.
- **App storage.** Every path an app names resolves under its own `/data/<package>` on the one LittleFS volume ([storage](/api/storage/)), so the app-visible path is capped at 185 bytes whatever the package name's length: `/data/` plus a 64-byte name plus the path must fit the 256-byte buffer the natives map into. A `..` segment is refused outright. LittleFS keeps a directory as a pair of 4 KB metadata blocks, so a directory costs 8 KB before it holds a byte — `/prefs` for `SharedPreferences`, `/files` for `openFileOutput` — and an app that uses both starts at 24 KB with its own directory. At boot, every board deletes the data directory of any package that is not installed — on a single-app board, flashing a different app removes the previous app's files and preferences. Under the simulator the host image has the board's size (`PICODROID_SIM_FS_KB` overrides it).
- **Assets size.** The "under 256 KiB of assets per PAPK" figure is a **recommended** guideline, not an enforced limit — neither the packer nor the on-device parser rejects oversized assets. The only hard ceiling is the app-region size above, shared with every other installed app. See [assets](/guides/assets/).
- **Networking caps** (WiFi boards). The FreeRTOS+TCP pool is sized by `ipconfigNUM_NETWORK_BUFFER_DESCRIPTORS` and `ipconfigNETWORK_MTU` in `crates/picodroid-core/net-freertos-tcp/FreeRTOSIPConfig.h`; the buffer tunables are `#ifndef`-wrapped defaults that a board's `net_*` keys override per-board (`net_buffer_descriptors`, `net_tcp_rx_bytes`, `net_tcp_tx_bytes`, `net_tcp_win_segs` — see `pico_enviro_mon_w/board.toml`). All of it comes out of the shared FreeRTOS heap (see the per-board notes above).
- **Text faces are flash, per board.** `TextView.setTextSize` snaps to the faces a board compiles (`text_sizes` in its MCU or board toml, see the [porting guide](/reference/porting-guide/)). Measured on `testbench_rp2350` (release, ASCII-only Montserrat from `scripts/gen-fonts.sh`, 4 bpp, uncompressed): the 20 px face costs 10.8 KB, 28 px 16.9 KB and 64 px 68.0 KB — the glyph bitmaps are 6.7, 12.8 and 63.9 KB of that, the rest is the 2.9 KB kerning table and descriptors each face carries. The stock 14 px theme face (with LVGL's symbols) is 13.6 KB and always present. The RP2350 boards ship all four (+99.6 KB over the one-face image); `testbench_rp2040` ships 14 alone, so every size snaps to it there.
- **Framework classes are embedded whole.** Every compiled SDK class ships in firmware on every board, with the link table built for it at firmware-build time (the same class section a PAPK carries; 103 KB of tables over 246 classes on `testbench_rp2350`), and is registered at boot, so a new SDK class costs its full `.class` size and its table in flash whether or not any app touches it — there is no tree-shaking. The tables are read in place: a registered class costs 16 bytes of RAM and nothing is parsed. On a board whose program region is nearly full (RP2040), a board can drop classes it does not need with the optional top-level `framework_class_excludes` key in `board.toml` (a `;`- or `,`-separated list of JVM internal names, e.g. `picodroid/json/JSONObject`; excluding a class also excludes its inner classes). An exclude that matches no compiled class fails the build, so a typo cannot silently keep shipping the class. An app that calls into an excluded class gets a native miss naming the exclusion. `testbench_rp2040` uses this to drop the `picodroid.net.*` classes it can never run (all but `NetworkInfo`, which stays answerable so portable apps can probe and degrade), the `java.util.concurrent` core set (`picodroid.concurrent` pools, `Future`, atomics and latch) — some 26 KB on the fleet's tightest program region — the fragment classes (`picodroid.app.Fragment`, `FragmentManager`, `FragmentTransaction`, `FragmentFactory`, `picodroid.widget.ViewPager2`, `FragmentStateAdapter`; an Activity only creates its manager on the first `getSupportFragmentManager()`, so an app that composes with Activities and Views never resolves them; `picodroid.lifecycle` stays, since `Activity` implements its owner interfaces and `LiveData` / `ViewModel` serve an Activity-only app too), `java.time` with `java.util.TimeZone` (about 67 KB of class files), the scheduled executor (`ScheduledExecutorService`, `ScheduledFuture`, `ScheduledFutureTask`, `MainScheduledExecutor`), `SntpClient`, and `picodroid.media` (`ToneGenerator`, `AudioManager`; the board has no buzzer). Feature switches extend the list on their own: a board that leaves `has_json` off drops the `picodroid.json` classes (and compiles the native parser out) without naming them here, `has_protobuf` does the same for `picodroid.protobuf` and its native codec, and `has_canvas = false` (set on `testbench_rp2040` only) drops `Canvas`, `Paint` and the display list behind `View.onDraw(Canvas)`. No other board excludes anything by hand.
- **JSON node pool.** `picodroid.json` documents live in a native pool capped at 2048 nodes and 16 KiB of string and key bytes across all live documents, with at most 32 levels of nesting. A parse past the cap throws `JSONException` (`JSON pool exhausted`); a `put` past it throws `OutOfMemoryError`. Nodes are reclaimed with the garbage collector once no `JSONObject`/`JSONArray` wrapper reaches them.
- **Focusable list rows.** On boards with a small LVGL pool (e.g. 48 KiB on `pico_enviro_mon`), keep focusable `lv_list` rows to roughly a dozen — the picoenvmon History screen caps at `MAX_ROWS = 12`. Each focusable row consumes render-pool memory; too many starve the LVGL draw tasks and stall the renderer. This is an **app-level guideline driven by the board's `lv_mem_kb`, not a framework constant** — boards with the default 64 KiB pool have more headroom. See [embedded gotchas](/guides/embedded-gotchas/) and [button navigation](/guides/button-navigation/).

## Kotlin apps

Kotlin costs class metadata, not object heap. Measured like-for-like on
`examples/picoenvmon_kt` against its Java twin `examples/picoenvmon` (same
screens, Service, dashboard server and DI graph; the [Kotlin guide](/guides/kotlin/)
explains the frugality rules the port follows):

| Metric | Java | Kotlin | Δ |
|---|---|---|---|
| PAPK (no-shrink / shrink) | 75,170 / 68,896 B | 79,095 / 72,908 B | +5 % |
| Classes in the PAPK | 35 | 45 (3 shim survivors) | +10 |
| Parsed class metadata after a nav cycle (device-derived) | 64.3 KB | 66.8 KB | +2.5 KB (+3.8 %) |
| JVM live floor, idle serving (sim, 416 KB arena) | 13.0 KB | 13.6 KB | +0.5 KB |
| JVM live floor after 7.5 h on device | — | 13.6 KB | stable |
| Device free heap at boot / after 7.5 h (`pdb sysmon`, `pico_enviro_mon_w`) | — | 164.5 KB / 135.9 KB | — |
| Device **min-ever-free** after 7.5 h soak | — | **124.3 KB** | budget ≥ 120 KB ✓ |
| Idle allocation signature | `alloc=+2 stri=+1`/s | identical | — |

These figures were measured on 2026-08-30 and are kept for the comparison between the two
languages. Two of them no longer describe a current build in absolute terms: PAPK v2 made every
packed app larger by its link tables, and there is no parsed class metadata on the heap any more
(a registered class is 16 bytes, its tables stay in flash).

Soak conditions (2026-08-30, mem-diag debug firmware): dashboard fetch every
2 s with 3-way bursts (11,677 requests), hourly four-screen navigation bursts,
NTP + weather refreshes; no crash, reboot, OOM or GC-pressure event in the measured window. An unattended overnight extension ended at 00:17 (9 h 29 m uptime): after a ~30 s serving gap the dashboard's uptime footer reset (569 m → 0 m) and the device came straight back up serving — a real reboot, unattributed because telemetry was down at that moment (the branch's own pre-commit had terminated the RTT attach at 22:37); a follow-up soak with telemetry attached end-to-end is open work. The 4 AM nightly HIL then reclaimed the device and ran green. At warm-up,
first-visit class parsing and socket set-up take the native footprint from
237 KB to a 272 KB plateau; after that the native-floor sentinel trips repeatedly under combined load — ~5 KB transient oscillations (sockets, weather/NTP buffers) that always return to the ~287 KB baseline within a minute or two, with min-ever-free unmoved after warm-up; no monotonic growth (286.1 KB at 20 minutes, 287.5 KB at 7.5 hours). A minimal Kotlin app (`hellokt`) is a
2.8 KB PAPK of three classes (PAPK v1; see the note under the table).
`examples/gcstress_kt` is the collector stress lane for Kotlin-specific churn
(lambda proxies, `Ref` boxes, autoboxing, `Pair`, map entry views).

## Screens

Every board is at least **240×240 logical pixels** (`board.toml` is refused below that), a `dp`
is one of them on every board, and the panels are 240×240, 320×240 and 320×480. A layout that
stretches (`match_parent`, weights) fills whichever it lands on; one that cannot declares a
`<supports-screens>` design size and runs in a window of that size — centred on a larger panel,
panned on a smaller one — and a root larger than its window pans rather than clips. The
simulator and a debug build log `[layout] fit ok WxH in WxH` or `[layout] overflow …` after each
`setContentView`. Resource directories may vary by `sw<N>dp`, `w<N>dp`, `h<N>dp`, `land`/`port`
and `notouch`/`finger` only. See [Apps on every board](/guides/every-board/).

## Display idle sleep

On every board, the simulator included, the panel **dozes** after the screen timeout with no key edge or touch: **60 seconds** by default (`idle_timeout_ms`), or what Settings → Display stored (`Settings.System.SCREEN_OFF_TIMEOUT`, `0` for never). The backlight and panel go off and LVGL stops ticking; **the app keeps running** — Runnables, alarms, `ScheduledExecutorService` tasks, network callbacks and the sensors continue, and `PowerManager.isInteractive()` says false. Any button wakes the panel, and so does a finger where the controller is read while dark (the touch kit's GT911 sampler task; the testbenches' XPT2046 polled inline). `KEYCODE_SLEEP` dozes at once, `KEYCODE_WAKEUP` wakes, `KEYCODE_POWER` toggles. The log numbers each transition: `display: doze #3 after 60000 ms idle`, `display: wake #3 (touch)`.

The wake behavior affects input handling: the keypress that wakes the panel **and its release edge are both swallowed**, and the finger that wakes it lands as no click. They wake the display but do not reach LVGL focus navigation, your `OnKeyListener` or a click listener — so a user pressing a sleeping screen wakes it without also navigating or clicking. The first *new* press after wake behaves normally.

A screen that is watched rather than touched holds the panel on with `View.setKeepScreenOn(true)` on its root (`android:keepScreenOn="true"` in a layout): `claudeusage`, `picoenvmon`, `weather` and `picoclock`'s face do. The hold ends with the view; a `KEYCODE_SLEEP` still dozes. An Activity that must be seen when it starts — `picoclock`'s ringing alarm — calls `Activity.setTurnScreenOn(true)`. See "Input and idle power" in [your first app](/get-started/first-app/).

## Tuning these limits

Most of these caps are board-level knobs:

- The five JVM/platform knobs (`gc_alloc_threshold`, `slot_chunk_shift`, `inline_array_data`, `activity_stack_depth`, `pending_op_queue`) live in your board's `[jvm]` block — see [JVM tunables](/reference/jvm-tunables/).
- Heap size, LVGL pool, idle timeout, and the background pool are set in `board.toml` and the platform config files — see [advanced configuration](/reference/advanced-config/).

## Sources

Every concrete number on this page comes from the build configuration, not from prose. If you change any of these files, re-grep this page so it stays accurate:

- Per-MCU RAM/flash/clock/cores: [`platforms/rp/mcus/rp`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/mcus/rp) (`rp2040.toml`, `rp2350.toml`, `rp2350b.toml`).
- FreeRTOS heap and clock branches: [`platforms/rp/mcus/rp/FreeRTOSConfig.h`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/mcus/rp/FreeRTOSConfig.h) (heap size injected from the `heap_kb` key in the MCU TOMLs).
- Networking buffer/MTU caps: [`crates/picodroid-core/net-freertos-tcp/FreeRTOSIPConfig.h`](https://github.com/shivrajora/picodroid-rs/blob/main/crates/picodroid-core/net-freertos-tcp/FreeRTOSIPConfig.h).
- JVM tunable defaults and ranges: [`crates/build_support/jvm_defaults.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/crates/build_support/jvm_defaults.rs).
- Per-board overrides (`lv_mem_kb`, `hot_ram_kb`, `app_region_kb`, `fs_kb`, `max_installed_apps`, `idle_timeout_ms`): each board's `board.toml` under [`platforms/rp/boards`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/boards).
