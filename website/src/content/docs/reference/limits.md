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
| `tdeck_plus` | ESP32-S3 (Xtensa LX7) | 512 KB | 8 MB | 240 MHz | 2 | — (M1) | 64 KiB |

Notes on the numbers:

- The SRAM figure is what the linker assumes, not the chip's physical total. RP2040 declares 256 KB (its four 64 KB main banks); the two 4 KB scratch banks are excluded, so the chip's 264 KB physical SRAM is reported as 256 KB. RP2350's 520 KB matches physical.
- The **FreeRTOS heap** (`configTOTAL_HEAP_SIZE`) is the single pool the JVM allocates from — see [How the Java heap works](#how-the-java-heap-works). It is selected by chip architecture in `FreeRTOSConfig.h`: 416 KB on the M33 (RP2350), 128 KB on the M0+ (RP2040).
- **LVGL buffer** is the UI render pool (`lv_mem_kb`, default 64 KiB). Only `pico_enviro_mon` overrides it, down to 48 KiB to fit its tighter budget — which is why that board has a practical list-row cap (see [Runtime limits](#runtime-limits)).
- **ESP32-S3 / `tdeck_plus` is Milestone 1**: there is no FreeRTOS yet. `start_tasks` calls the JVM directly, single-threaded, and the whole JVM runs from a **fixed static 256 KiB arena** rather than a FreeRTOS pool. `Thread.start` and the background pool are RP-only paths there. Treat the ESP board as experimental.

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

On **ESP32-S3** there is no FreeRTOS pool: the JVM runs from a single static 256 KiB arena, single-threaded. Tune that figure when FreeRTOS/PSRAM land.

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

Details:

- **GC cadence.** A collection runs after `gc_alloc_threshold` allocations (default 256) or on an OOM signal. Lower it to shrink the heap high-water mark, raise it to cut pause frequency — see [JVM tunables](/reference/jvm-tunables/).
- **Activity stack depth** (`activity_stack_depth`, default 8). Pushing past the cap returns soft (no `Result` threaded through JVM dispatch). The new Activity is dropped, the parked view is restored, and the app keeps running on the previous top. The framework **does** log this (host `eprintln!` / device `defmt::error!`), but it is never surfaced to Java as an exception. Raise the depth for deep modal/wizard flows.
- **Pending-op queue** (`pending_op_queue`, default 8). This FIFO holds lifecycle ops queued by `startActivity` and `finish()`. On a full queue the op is **dropped silently** — there is no log at the real call sites and no Java-visible error. Do not rely on a warning here. (This is distinct from the executor runnable queues backing `MainExecutor`/`BackgroundExecutor`, which *do* log `queue full, dropped` — different queues.)
- **Background threads.** Each `picodroid.concurrent.Thread.start()` spins up one FreeRTOS task, pinned to **core 0** (required by the single-core safety assumption of the shared JVM state), with a **16 KiB stack** (the stack size is counted in words, not bytes — 4096 words × 4 = 16 KiB; do not read it as 4 KB). Priority maps from the Java thread's priority field, defaulting to `Thread.NORM_PRIORITY`. Threading is a no-op in the simulator. See [background services](/tutorials/background-service/).
- **PAPK install ceiling.** The whole package (manifest + classes + assets) must fit in `PAPK_MAX_DATA_SIZE = 1020 KB` (a 1 MB flash slot minus a 4 KB metadata sector). Larger payloads are rejected at install time with `InstallError::TooLarge`; the device advertises this ceiling to the host in its ready frame. See the [manifest reference](/reference/manifest/) and [shrinker](/reference/shrinker/) for keeping under it.
- **Assets size.** The "under 256 KiB of assets per PAPK" figure is a **recommended** guideline, not an enforced limit — neither the packer nor the on-device parser rejects oversized assets. The only hard ceiling is the overall 1020 KB PAPK size above. See [assets](/guides/assets/).
- **Focusable list rows.** On boards with a small LVGL pool (e.g. 48 KiB on `pico_enviro_mon`), keep focusable `lv_list` rows to roughly a dozen — the picoenvmon History screen caps at `MAX_ROWS = 12`. Each focusable row consumes render-pool memory; too many starve the LVGL draw tasks and stall the renderer. This is an **app-level guideline driven by the board's `lv_mem_kb`, not a framework constant** — boards with the default 64 KiB pool have more headroom. See [embedded gotchas](/guides/embedded-gotchas/) and [button navigation](/guides/button-navigation/).

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

- Per-MCU RAM/flash/clock/cores: [`platforms/rp/mcus/rp`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/mcus/rp) (`rp2040.toml`, `rp2350.toml`) and `platforms/esp/mcus/esp/esp32s3.toml`.
- FreeRTOS heap and clock branches: [`platforms/rp/mcus/rp/FreeRTOSConfig.h`](https://github.com/shivrajora/picodroid-rs/blob/main/platforms/rp/mcus/rp/FreeRTOSConfig.h).
- JVM tunable defaults and ranges: [`build_support/jvm_defaults.rs`](https://github.com/shivrajora/picodroid-rs/blob/main/build_support/jvm_defaults.rs).
- Per-board overrides (`lv_mem_kb`, `idle_timeout_ms`): each board's `board.toml` under [`platforms/rp/boards`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/boards).
