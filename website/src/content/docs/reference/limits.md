---
title: "System limits & memory budgets"
description: "How much an app can do before it falls over: per-board RAM/flash, Java heap behavior, runtime caps, and idle sleep."
---

Picodroid runs your Java app inside a Rust JVM on an MCU with kilobytes — not gigabytes — of RAM. This page collects the hard ceilings and practical budgets so you can size an app before it falls over at runtime.

## Per-board memory budget

The MCU sets the ceiling. RAM and flash are the two scarce resources; everything below competes for them.

| Board | MCU | SRAM | Flash | Clock | Cores | FreeRTOS heap | LVGL buffer |
|---|---|---|---|---|---|---|---|
| `testbench_rp2040` | RP2040 (Cortex-M0+) | 256 KB | 2 MB | 125 MHz | 2 | 128 KB | 64 KiB |
| `testbench_rp2350` | RP2350 (Cortex-M33) | 520 KB | 4 MB | 150 MHz | 2 | 416 KB | 64 KiB |
| `testbench_rp2350w` | RP2350 (Cortex-M33) | 520 KB | 4 MB | 150 MHz | 2 | 416 KB | 64 KiB |
| `pico_enviro_mon` | RP2350 (Cortex-M33) | 520 KB | 4 MB | 150 MHz | 2 | 416 KB | 48 KiB |

Notes on the numbers:

- The SRAM figure is what the linker assumes, not the chip's physical total. RP2040 declares 256 KB (its four 64 KB main banks); the two 4 KB scratch banks are excluded, so the chip's 264 KB physical SRAM is reported as 256 KB. RP2350's 520 KB matches physical.
- The **FreeRTOS heap** (`configTOTAL_HEAP_SIZE`) is the single pool the JVM allocates from — see [How the Java heap works](#how-the-java-heap-works). It is single-sourced from the MCU TOML's `heap_kb` key (`mcus/rp/rp2350.toml`: 416 KB, `mcus/rp/rp2040.toml`: 128 KB) and injected at build time; `FreeRTOSConfig.h` refuses to compile without the injection.
- On WiFi boards (`testbench_rp2350w`, `pico_enviro_mon_w`), the networking stack shares that same 416 KB arena: the FreeRTOS+TCP network buffers and the `cyw43` task stack are all allocated from it, so a WiFi build has correspondingly less Java-heap headroom. Heap-constrained boards shrink the stack's share with the `net_*` keys in `board.toml` — `pico_enviro_mon_w` halves the descriptor count and per-socket TCP buffers relative to the testbench defaults.
- **LVGL buffer** is the UI render pool (`lv_mem_kb`, default 64 KiB). Only `pico_enviro_mon` overrides it, down to 48 KiB to fit its tighter budget — which is why that board has a practical list-row cap (see [Runtime limits](#runtime-limits)).

## How the Java heap works

There is no fixed "JVM heap size" constant on RP boards. The JVM allocates on demand from the global allocator, and on RP the global allocator **is** the FreeRTOS heap:

```rust
#[global_allocator]
static GLOBAL: FreeRtosAllocator = FreeRtosAllocator;
```

Every Java object, array, and string routes through `pvPortMalloc`, drawing from the single `configTOTAL_HEAP_SIZE` pool. So your effective Java heap is whatever of that pool is left after task stacks, queues, framework BSS, and LVGL take their share — practically a **128 KB pool on RP2040** and a **416 KB pool on RP2350**, shared with everything else.

A few mechanics worth knowing:

- **One process-wide heap.** All JVM threads share a single `SharedJvmHeap` (objects, arrays, strings), matching the standard Java memory model. Background threads build their own interpreter state but allocate into the same shared pool.
- **No-op OOM hook.** When `pvPortMalloc` returns NULL, the malloc-failed hook is intentionally a no-op so Rust's `try_reserve_exact` can return `Err` and trigger a GC on the next interpreter step. Non-fallible allocations still abort.
- **Chunked slot allocator.** Object and array slot tables grow one fixed-size chunk at a time (`ChunkedSlots`) instead of doubling a single `Vec`. The default chunk is 64 slots (`slot_chunk_shift = 6`). This caps the worst-case contiguous request — single-digit KiB for most types, tens of KiB for arrays — so the FreeRTOS heap can satisfy growth even when fragmented. The doubling allocator it replaced once demanded a 90 KB contiguous block that the heap could not serve on `pico_enviro_mon`.

## Runtime limits

| Limit | Default | Overflow behavior |
|---|---|---|
| GC cadence | every 256 allocations | not an error — a collection runs |
| Activity stack depth | 8 | new Activity silently dropped; logged on host, no Java exception |
| Pending-op queue | 8 | op dropped silently — no log, no Java exception |
| Background `Thread` stack | 16 KiB, core 0 | FreeRTOS task creation fails if heap is exhausted |
| PAPK install size | 1020 KB | rejected at install with `InstallError::TooLarge` |
| Assets per PAPK | 256 KiB (recommended) | not enforced — see below |
| Focusable list rows (small boards) | ~12 (app guideline) | render-pool stall, not a framework cap |
| Network buffer descriptors (WiFi boards) | 16 (`testbench_rp2350w`) / 8 (`pico_enviro_mon_w`) | in-flight packets beyond the pool wait for a descriptor to free |
| Network MTU (WiFi boards) | 1500 bytes | larger frames are never carried |

Details:

- **GC cadence.** A collection runs after `gc_alloc_threshold` allocations (default 256) or on an OOM signal. Lower it to shrink the heap high-water mark, raise it to cut pause frequency — see [JVM tunables](/reference/jvm-tunables/).
- **Activity stack depth** (`activity_stack_depth`, default 8). Pushing past the cap returns soft (no `Result` threaded through JVM dispatch). The new Activity is dropped, the parked view is restored, and the app keeps running on the previous top. The framework **does** log this (host `eprintln!` / device `defmt::error!`), but it is never surfaced to Java as an exception. Raise the depth for deep modal/wizard flows.
- **Pending-op queue** (`pending_op_queue`, default 8). This FIFO holds lifecycle ops queued by `startActivity` and `finish()`. On a full queue the op is **dropped silently** — there is no log at the real call sites and no Java-visible error. Do not rely on a warning here. (This is distinct from the executor runnable queues backing `MainExecutor`/`BackgroundExecutor`, which *do* log `queue full, dropped` — different queues.)
- **Background threads.** Each `picodroid.concurrent.Thread.start()` spins up one FreeRTOS task, pinned to **core 0** (required by the single-core safety assumption of the shared JVM state), with a **16 KiB stack** (the stack size is counted in words, not bytes — 4096 words × 4 = 16 KiB; do not read it as 4 KB). Priority maps from the Java thread's priority field, defaulting to `Thread.NORM_PRIORITY`. The simulator runs the same FreeRTOS kernel, so threads are real there too — single-core rather than dual-core, and without the core pinning. See [background services](/tutorials/background-service/).
- **PAPK install ceiling.** The whole package (manifest + classes + assets) must fit in `PAPK_MAX_DATA_SIZE = 1020 KB` (a 1 MB flash slot minus a 4 KB metadata sector). Larger payloads are rejected at install time with `InstallError::TooLarge`; the device advertises this ceiling to the host in its ready frame. See the [manifest reference](/reference/manifest/) and [shrinker](/reference/shrinker/) for keeping under it.
- **Assets size.** The "under 256 KiB of assets per PAPK" figure is a **recommended** guideline, not an enforced limit — neither the packer nor the on-device parser rejects oversized assets. The only hard ceiling is the overall 1020 KB PAPK size above. See [assets](/guides/assets/).
- **Networking caps** (WiFi boards). The FreeRTOS+TCP pool is sized by `ipconfigNUM_NETWORK_BUFFER_DESCRIPTORS` and `ipconfigNETWORK_MTU` in `platforms/rp/src/hal/rp/port/FreeRTOSIPConfig.h`; the buffer tunables are `#ifndef`-wrapped defaults that a board's `net_*` keys override per-board (`net_buffer_descriptors`, `net_tcp_rx_bytes`, `net_tcp_tx_bytes`, `net_tcp_win_segs` — see `pico_enviro_mon_w/board.toml`). All of it comes out of the shared FreeRTOS heap (see the per-board notes above).
- **Framework classes are embedded whole.** Every compiled SDK class ships in firmware on every board and is loaded at boot, so a new SDK class costs its full `.class` size in flash whether or not any app touches it — there is no tree-shaking. On a board whose program region is nearly full (RP2040), a board can drop classes it does not need with the optional top-level `framework_class_excludes` key in `board.toml` (a `;`- or `,`-separated list of JVM internal names, e.g. `picodroid/json/JSONObject`; excluding a class also excludes its inner classes). An exclude that matches no compiled class fails the build, so a typo cannot silently keep shipping the class. An app that calls into an excluded class gets a native miss naming the exclusion. `testbench_rp2040` uses this to drop the `picodroid.net.*` classes it can never run (all but `NetworkInfo`, which stays answerable so portable apps can probe and degrade) — worth ~9 KB on the fleet's tightest program region. No other board excludes anything.
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

Soak conditions (2026-08-30, mem-diag debug firmware): dashboard fetch every
2 s with 3-way bursts (11,677 requests), hourly four-screen navigation bursts,
NTP + weather refreshes; no crash, reboot, OOM or GC-pressure event. The
growth sentinel trips at warm-up (first-visit class parsing and socket set-up
take the native footprint from 237 KB to a 272 KB plateau) and transiently on
each 3-way HTTP burst and each hourly nav burst (~4.7 KB of socket and screen
buffers that return within a minute, min-free unaffected); across the run the native footprint
was flat — 286.1 KB at 20 minutes, 287.5 KB at 7.5 hours. A minimal Kotlin app (`hellokt`) is a
2.8 KB PAPK of three classes; the per-class costs in the table above (~20 B
registered, ~0.8 KB parsed, 32 B per method) are what to budget for.
`examples/gcstress_kt` is the collector stress lane for Kotlin-specific churn
(lambda proxies, `Ref` boxes, autoboxing, `Pair`, map entry views).

## Display idle sleep

On `has_buttons` boards (not the simulator, not touch-only boards), the panel sleeps after **60 seconds** with no button input (the default `idle_timeout_ms`). Setting `idle_timeout_ms = 0` disables sleep — `pico_enviro_mon` does this.

The wake behavior has one quirk that affects input handling: the keypress that wakes the panel **and its release edge are both swallowed**. They wake the display but do not reach LVGL focus navigation or your `OnKeyListener` — so a user pressing a button on a sleeping screen wakes it without also navigating or clicking. The first *new* press after wake behaves normally.

Sleep only exists on button-driven boards because the wake path blocks on a button IRQ; a touch-only board would never wake. See "Input and idle power" in [your first app](/get-started/first-app/).

## Tuning these limits

Most of these caps are board-level knobs:

- The five JVM/platform knobs (`gc_alloc_threshold`, `slot_chunk_shift`, `inline_array_data`, `activity_stack_depth`, `pending_op_queue`) live in your board's `[jvm]` block — see [JVM tunables](/reference/jvm-tunables/).
- Heap size, LVGL pool, idle timeout, and the background pool are set in `board.toml` and the platform config files — see [advanced configuration](/reference/advanced-config/).

## Sources

Every concrete number on this page comes from the build configuration, not from prose. If you change any of these files, re-grep this page so it stays accurate:

- Per-MCU RAM/flash/clock/cores: [`platforms/rp/mcus/rp`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/mcus/rp) (`rp2040.toml`, `rp2350.toml`).
- FreeRTOS heap and clock branches: [`platforms/rp/mcus/rp/FreeRTOSConfig.h`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/mcus/rp/FreeRTOSConfig.h) (heap size injected from the `heap_kb` key in the MCU TOMLs).
- Networking buffer/MTU caps: [`platforms/rp/src/hal/rp/port/FreeRTOSIPConfig.h`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/src/hal/rp/port/FreeRTOSIPConfig.h).
- JVM tunable defaults and ranges: [`build_support/jvm_defaults.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/build_support/jvm_defaults.rs).
- Per-board overrides (`lv_mem_kb`, `idle_timeout_ms`): each board's `board.toml` under [`platforms/rp/boards`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/boards).
