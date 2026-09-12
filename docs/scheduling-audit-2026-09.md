# Scheduling Audit — busy-waits, polling and blocking — 2026-09-12

Full-tree scan for places where a FreeRTOS task **busy-waits** (spins on a flag, counts CPU
cycles, sleep-polls a condition) instead of blocking on an interrupt, DMA completion, queue,
semaphore, mutex or task notification, and for delays that burn cycles instead of calling the
RTOS delay. A busy-waiting task at a higher priority than the interpreter (the `RT_*` band,
21–30) or on the same core as it starves Java code outright; a sleep-polling loop adds latency
and wakes the core needlessly. The second half of the document is the **offensive-guard**
design that makes the property enforced rather than remembered.

- **Baseline:** commit `4cad7261` (main, 2026-09-12).
- **Method:** three parallel deep-dives (RP HAL + drivers; RTOS seam, thread model and main
  loops; networking, storage, sensors, logging), each grounded in file:line evidence, with the
  High findings re-verified by hand against the source. Every claim carries a path and line.
- **Scope:** `platforms/rp/**`, `crates/picodroid-core/**`, the concurrency parts of
  `crates/jvm/**`, the port glue under `platforms/rp/src/hal/rp/port/**`, and the vendored
  `cyw43-driver` / `FreeRTOS-Plus-TCP` forks where the project's port layer calls into them.

## Status

| Work package | State |
|---|---|
| WP1 quick wins (F1, F4, F6 opt-in, F9, F13, F15, F19 bounds + joiners) | **landed** 2026-09-12 |
| G5 `spin_until!` | **landed** 2026-09-12 (`crates/picodroid-core/src/hal/spin.rs`, a porting seam item) |
| G1 spin ledger | **landed** 2026-09-12 (`platforms/rp/src/spin_guard.rs`; 14 `spin-ok`, 6 `spin-todo`: F3 ×1, F14 ×2, F18 ×3) |
| G2 config assertions | **landed** 2026-09-12 (`task_affinity::idle_cores_sleep_and_driver_waits_yield`) |
| WP2 kernel-backed `RpDelay` (F2) | **landed** 2026-09-12 (`platforms/rp/src/hal/rp/delay.rs`; the type is changed in place, so no cycle-only delay remains) |
| WP0, WP3–WP11, G3, G4, G6 | open — see the plan below |

## Executive summary

- **The spine is right.** The UI loop blocks on a queue; `Thread.sleep`/`join`/`Object.wait` block on
  task notifications; `synchronized` is a kernel recursive mutex (priority inheritance for free);
  Java sockets are blocking FreeRTOS+TCP calls with `SO_RCVTIMEO` and there is no polling anywhere in
  `net/`; CYW43 RX, buttons, I²C and display DMA are IRQ-driven; GC stops the world by suspending the
  scheduler, not by spinning. Nothing in the JVM's own concurrency model spins.
- **The defects are at the edges**, in port glue and a few HAL waits, and four of them are High:
  1. `cyw43_delay_ms(1)` is a hard `nop` spin because the RTOS branch is gated on `ms >= 2`, and every
     CYW43 poll loop passes `1` — up to 500 ms per ioctl and 3 s at boot on core 1, up to 1 s on the IP
     task. **One comparison fixes all of it.**
  2. The RP `DelayNs` is `cortex_m::asm::delay`, and the panel and touch drivers run it from the JVM
     task after the scheduler starts: 350–540 ms at boot, 120 ms on every screen wake, on the UI task.
  3. The USB debug bridge spins unbounded on an ISR flag at priority 21 on the JVM's core; a host that
     stops reading freezes the app forever.
  4. The wall-clock seqlock reader spins with no yield at the JVM tier; with slicing off, a preempted
     writer plus a woken UI task is a livelock.
- **Sleep-polling where a signal exists:** the touch panel is read 100×/s at priority 23 forever while
  its INT line sits configured and unread (already tracked as S8); the install coordinator polls an
  atomic at 10 ms; the sim drains child threads by polling; the alarm table is scanned every 16 ms.
- **Structural:** no core ever executes `wfi` when idle; the 16 ms tick fires regardless of pending
  work and LVGL's own "time till next" is discarded while a literal 16 is fed to `lv_tick_inc`; a
  PAPK install erases the whole run in one interrupts-off window (both cores dead for seconds, WiFi RX
  lost); `defmt-rtt` is in blocking mode so a stalled probe host stalls any logging task; the `Rtos`
  seam has no ISR-safe give/notify, so shared code cannot build IRQ-driven waits at all.
- **Offensive guards proposed** (section below): a source-level "spin ledger" test that rejects any
  unlisted `asm::delay`/`spin_loop`/`nop`/empty-body register poll, config assertions on the idle
  hooks and the CYW43 delay threshold, a zero-cost `sched-diag` runtime monitor (hog / poll-storm /
  busy-delay / idle-share, strict-abort in soaks) built on the trace facility that is already compiled
  in, and a bounded `spin_until!` helper that makes every remaining spin named and finite.

## Ranked findings

Severity: **High** = busy spin in task context ≥ ~1 ms, or unbounded, or starves lower tasks /
livelocks; **Med** = bounded spin on a hot path, or sleep-polling with a usable signal, or a structural
power/latency cost; **Low** = pre-scheduler, µs-scale, or by design. Size = XS (< 10 lines), S (one
file), M (one subsystem + HIL). IDs are stable; detail and line-level evidence are in Appendices C–E.

| ID | Sev | Where | What | Runs on | Fix | Size |
|---|---|---|---|---|---|---|
| F1 | High | `platforms/rp/src/hal/rp/port/net/cyw43_port.c:45-52` + `cyw43_configport.h:153-154` | `cyw43_delay_ms(1)` never yields (`ms >= 2` gate) → `cyw43_do_ioctl` spins ≤ 500 ms/ioctl (`cyw43_ll.c:1170-1195`), SDPCM TX-credit wait ≤ 1 s (`:651-695`), F2-ready boot wait ≤ 3 s (`:1708-1730`), all holding the driver mutex | cyw43 22 / core 1; IP task 7 | `ms >= 1`; later: host-wake notification satisfies the ioctl and credit waits | XS |
| F2 | High | `platforms/rp/src/hal/rp/delay.rs:22-25`; used at `hal/rp/display.rs:59`, `hal/rp/touch.rs:144` | `RpDelay` = `cortex_m::asm::delay`; `st7789.rs:103-134` (350 ms), `st7796.rs:168-194` (540 ms), `gt911.rs:90-111` (76 ms) at init from the JVM task (`graphics/lvgl/lifecycle.rs:58-59`); `sleep_out` 120 ms on every screen wake (`lifecycle.rs:375` → `display.rs:129-134`) | UI/JVM 15 / core 0 | `RpRtosDelay`: ≥ 1 ms and scheduler running → `CurrentTask::delay`, else `asm::delay`; swap the two construction sites | S |
| F3 | High | `platforms/rp/src/hal/rp/pdb_usb/mod.rs:528-532` | `wait_tx_ready`: `while !EP1_IN_DONE { nop }`, per 64-byte chunk, no timeout | pdb 21 / core 0 | ISR `give_from_isr` (flag set at `:245`, `:388`) → `sem.take(Duration::ms(500))`; drop chunk / mark dead on timeout | S |
| F4 | High | `crates/picodroid-core/src/os/system_clock.rs:81-92, 97-110` | seqlock reader `continue`s while seq is odd; writer (`SystemClock.setCurrentTimeMillis`, a tier-15 task) preempted mid-write + UI task woken by the timer task = UI spins forever, writer never scheduled (no slicing) | UI 15 / core 0 | writer wraps its three stores in `AtomicSection`; optionally bound reader retries | XS |
| F5 | High | `crates/picodroid-core/src/hal/touch_sampler.rs:176-181` (`PERIOD_MS = 10`); `hal/rp/touch.rs:89, 148` | 100 Hz unconditional panel read at priority 23 forever; GT911 INT and XPT2046 PENIRQ bound as inputs and dropped; no display-sleep pause | touch 23 / core 0 | edge IRQ → semaphore; `sem_take(Ms(10))` while touched, long timeout when idle; `pause/resume` like the sensor sampler (S8) | M |
| F6 | High | `platforms/rp/Cargo.toml:85` (`defmt-rtt = "1"`, 1.1.0) | probe-rs sets `MODE_BLOCK_IF_FULL`; `channel.rs:30-38` then spins until the host drains → any logging task (cyw43 22, IP task) stalls unbounded; contaminates HIL timing | any | `features = ["disable-blocking-mode"]` for device builds, or a `log-lossless` opt-in feature | XS |
| F7 | Med-High | `platforms/rp/src/hal/rp/pio_spi.rs:168-182, 573-581` | gSPI DMA-done + TXSTALL spin on every WiFi frame, both directions (≤ 450 µs each); `DMA_IRQ_1` unclaimed anywhere | cyw43 22 / core 1; IP task 7 | ch4/5 → `INTE1`, `DMA_IRQ_1` handler gives a semaphore, `take(Duration::ms(5))`; keep spin ≤ 8-byte frames | M |
| F8 | Med-High | `crates/picodroid-core/src/install/region.rs:126-131` → `hal/rp/flash.rs:250-261` | whole PAPK run erased in one `park_core1_for_flash` + `cpsid i` window: 45 ms × sectors ≈ seconds with both cores dead; host-wake IRQ and IP task frozen, CYW43 RX overflows | UI/fs / both cores | erase one sector (or 8) per park cycle, as LittleFS already does (`fs/storage.rs:74-77`) | S |
| F9 | Med | `cyw43_port.c:37-43`; `cyw43_ll.c:1907-1912` | `cyw43_delay_us(150000)` at bring-up has no RTOS branch → 150 ms spin | cyw43 22 / core 1 | route `us >= 1000` through `vTaskDelay`, spin the remainder | XS |
| F10 | Med | `crates/picodroid-core/src/monitor_store.rs:106-111` | `xTaskAbortDelay` from `abort_all_child_delays` (`pdb/pending.rs:132-141`) also aborts a mutex block → `Forever` failure mapped to `IllegalMonitorState` on app stop | any Java task | on `Forever` failure check `host::stop_requested()`; return the stop result `threads::park_loop` uses (`threads.rs:375-377`); re-lock on spurious abort | S |
| F11 | Med | `pdb_usb/mod.rs:503-516`; caller `pdb/platform.rs:35-38` | `queue_read_byte_busywait(2 s)` for **every** install byte on RP2350, not only inside the tick-frozen XIP-off window | pdb 21 / core 0 | hybrid: blocking `receive(Ms(2))`, fall back to the µs-timer spin only while the tick is frozen (`CORE0_PARKED` / tick not advancing) | S |
| F12 | Med | `platforms/rp/src/pdb/coordinator.rs:36-46` | `wait_for_park`: 10 ms sleep-poll of `CORE0_PARKED`, ≤ 15 s; the reverse channel (`pending::notify_jvm`) already exists | pdb 21 / core 0 | JVM task `notify()`s pdb right after setting `CORE0_PARKED` (`boot_tasks.rs:229-230`); `take_notification(true, Ms(15000))` re-check loop | S |
| F13 | Med | `hal/rp/dma.rs:214-215`; `pio_spi.rs:186-201` | DMA abort `while busy {}` ×2, unbounded, on the error-recovery path (RP2040-E13: abort can fail to retire) | UI 15; cyw43 22 | the bounded `wait_dma_done` shape (`pio_spi.rs:172-181`) via `spin_until!` (G5) | XS |
| F14 | Med | `platforms/rp/src/hal/rp/uart.rs:126-142` | TX FIFO-full spin per byte; default 9600 baud (`:103`) → ~1 ms/byte once the 32-deep FIFO fills; reached from Java (`pio/uart.rs`) at tier 15; RX side is non-blocking `-1` (pushes polling into Java) | JVM 15 | TX ring + `UARTx_IRQ` TXIM, block on a semaphore only when full; RXIM → queue (the I²C pattern `i2c/mod.rs:435-487`); at minimum a bound | M |
| F15 | Med | `platforms/rp/mcus/rp/FreeRTOSConfig.h:84, 118` | `configUSE_IDLE_HOOK 0`, `configUSE_PASSIVE_IDLE_HOOK 0`, `configUSE_TICKLESS_IDLE 0`: `prvIdleTask` tight-loops on both cores; no `wfi` anywhere outside the SMP `wfe` spin stubs | idle / both cores | both hooks on, body `wfi` (tick, IPI, host-wake all wake it); tickless idle deferred until F16 | XS |
| F16 | Med | `graphics/lvgl/lifecycle.rs:111-115` + `lifecycle.rs:387` (`g.tick(16)`); `lifecycle.rs:414-415` → `alarms.rs:366-405`; `lifecycle.rs:353` | literal 16 into `lv_tick_inc` (a 200 ms frame advances LVGL 16 ms), `lv_timer_handler()` return discarded, 62.5 forced wakes/s when idle; full alarm-table scan + seqlock read every tick; idle-GC counted in ticks (`IDLE_GC_TICKS = 125`) | UI 15 / core 0 | measured delta into `lv_tick_inc`; reprogram the timer from `lv_timer_handler`'s return (or main loop `queue_recv(Ms(next))`); earliest-deadline cache for alarms; idle-GC by `now_ms()` | M |
| F17 | Med (sim) | `sim_boot.rs:179-183`; `hal/sim/rtos.rs:528-529`; `hal/sim/net.rs:226-228` | child-drain 10 ms sleep-poll (device uses notifications); `delay_ms(0)` is `sleep(0)` not a yield while the device's `vTaskDelay(0)` reschedules; accept polls at 5 ms | sim | last child notifies the JVM task; `if ms == 0 { yield_now() }`; blocking accept with `SO_RCVTIMEO` | S |
| F18 | Low-Med | `i2c/mod.rs:502, 563, 112`; `drivers/xpt2046.rs:140-152`; `spi/mod.rs:400-406, 458-464` | controller-disable spin at the head of every I²C transfer (skip when `IC_TAR` unchanged); XPT2046 does 10 polled 3-byte transfers + 2 `set_frequency` per sample on the UI task (batch into one ≥ 9-byte ISR transfer); the SPI small path returns before taking `spi_lock` (a 1–6 byte command can interleave with a locked `reconfigure()`) | UI 15 | address cache; batch; take the lock | S |
| F19 | Low | `threads.rs:92, 386-392` (`JOIN_POLL_MS`); `cyw43_port.c:313-317`; `pico_shim_rp2040.c:93-101`; `core1_park.rs:130-132, 142-145` | 5th+ joiner sleep-polls at 20 ms; `cyw43_yield()` = `taskYIELD` on a core with no equal-priority peer (no-op); RP2040 core-1 launch handshake has no timeout (RP2350's has 5 M tries); parker handshake spins are correct by design but unbounded | — | size `joiners` to `MAX_JAVA_THREADS`; delete or `vTaskDelay(1)`; port the RP2350 timeout back; `spin_until!` cap + panic | XS each |
| F20 | Seam | `crates/picodroid-core/src/rtos/mod.rs:116-214` | no `sem_give_from_isr` / `task_notify_from_isr`, no `delay_until`, no counting semaphore / event group; family code reaches around the seam (`gpio.rs:354-358` raw `give_from_isr`), shared code (drivers, `touch_sampler`) cannot express IRQ → task at all | — | add the ISR-safe pair (+ `portYIELD_FROM_ISR`) and `delay_until`; extend `seam_guard` | S |

Not findings (verified, leave alone): peripheral de-reset polls (ns), `adc/mod.rs:60` (2 µs),
`pio_spi.rs:601-603` (100 ns IRQ sample delay), `psram.rs` QMI bring-up (pre-scheduler),
`spi/mod.rs:184-208` polls gated to ≤ 8 bytes, `gt911.rs:101` `delay_us(120)`, `flash.rs:299-301`
post-watchdog loop, fault-handler `loop {}`, SMP spinlocks in `pico_shim.h` (kernel substrate),
`monitor_store.rs:333` (inside `#[cfg(test)]`), BME688 `poll_ready(max_polls = 1)`.

## Remediation plan (work packages, in order)

Each WP is one PR-sized change, one commit per finding where practical, sim smoke +
`./scripts/pre-commit` after each, `--full` before push. HIL rows named per WP.

**WP0 — Seam prerequisites (F20).** `crates/picodroid-core/src/rtos/mod.rs`: add
`sem_give_from_isr(RawSem) -> bool` (returns "higher-priority woken" so the caller can yield),
`task_notify_from_isr(RawTask)`, `delay_until(&mut last_wake_ms, period_ms)`. Device arms in
`platforms/rp/src/glue.rs` (beside `delay_ms` at `:609`) using `freertos_rust`'s `give_from_isr` /
`vTaskNotifyGiveFromISR` + `portYIELD_FROM_ISR`; sim arms in `hal/sim/rtos.rs` and `rtos_freertos.rs`.
Migrate `gpio.rs:354-358` onto it. Update the `seam_guard` must-list.

**WP1 — Quick wins, one commit each (F1, F9, F4, F6, F13, F15, F19).**
- `cyw43_port.c:47` `ms >= 2` → `ms >= 1`; `cyw43_delay_us`: `us >= 1000 && scheduler running` →
  `vTaskDelay(us / 1000)` then spin the remainder. Re-verify join on `testbench_rp2350w` (DHCP bind
  time, `instr_*` counters in `NetworkInterface_CYW43.c:44-46`, `link.rs:144`).
- `system_clock.rs:97-110`: wrap the three stores in `AtomicSection` (import path used by
  `gc/mod.rs:294-299`); comment why.
- `platforms/rp/Cargo.toml:85`: `defmt-rtt = { version = "1", features = ["disable-blocking-mode"] }`;
  note in `docs/` that RTT is lossy under a slow host. (If lossless capture is ever needed, a
  `log-lossless` feature can re-enable it explicitly.)
- `dma.rs:214-215`, `pio_spi.rs:186-201`, `core1_park.rs:130-132, 142-145`, `pico_shim_rp2040.c:93-101`:
  bound with `spin_until!` (G5) / the RP2350 `tries` cap; log + `Err` or panic on expiry.
- `FreeRTOSConfig.h`: `configUSE_IDLE_HOOK 1`, `configUSE_PASSIVE_IDLE_HOOK 1`; `vApplicationIdleHook`
  and `vApplicationPassiveIdleHook` in `pico_shim_rp2040.c` / `pico_shim_rp2350.c` = `dsb; wfi; isb`.
  Keep `configUSE_TICKLESS_IDLE 0`. HIL: `bootcount` flash test (parker latency unchanged) + a
  `loop` row; measure idle share via `pdb sysmon` before/after.
- `threads.rs`: `MAX_JOINERS = MAX_JAVA_THREADS`, delete `JOIN_POLL_MS`; `cyw43_yield` → delete the
  hook or `vTaskDelay(1)`.

**WP2 — RTOS-backed delay (F2).** `platforms/rp/src/hal/rp/delay.rs`: add `RpRtosDelay` implementing
`DelayNs` (`ns >= 1_000_000 && scheduler_running` → `CurrentTask::delay(Duration::ms(ns/1e6))` then
`asm::delay` for the sub-ms remainder; otherwise `asm::delay`). Change `display.rs:40, 59` and
`touch.rs:144` to it; delete `RpDelay` if no pre-scheduler caller remains (none found). Drivers are
generic over `DelayNs` and need no change. Sim `SimDelay` stays a no-op (parity row TIM-04 gets a note).
Measure boot-to-first-frame and button-wake latency on `pico_enviro_mon` / `pico_touch_kit`.

**WP3 — USB debug bridge (F3, F11).** `pdb_usb/mod.rs`: binary semaphore beside `rx_queue()` (`:398`),
given from the ISR where `EP1_IN_DONE` is stored (`:245`, `:388`) with the existing
`InterruptContext` (`:333`); `wait_tx_ready` = `take(Duration::ms(500))`, on timeout set a
`tx_dead` flag and return so `write_bytes` stops. `queue_read_byte_busywait`: first
`receive(Duration::ms(2))`; only if `xTaskGetTickCount()` did not advance (tick frozen) fall into the
µs-timer loop. HIL: `pdb` rows + unplug-mid-`pdb list` (app must keep running) + install soak.

**WP4 — Touch by interrupt (F5, needs WP0).** Family side `hal/rp/touch.rs`: keep the pin bound, arm
`gpio::enable_edge_irq(TOUCH_PIN_INT / TOUCH_PIN_IRQ, Falling)` and give a semaphore from
`IO_IRQ_BANK0` (core-0 branch, `gpio.rs:265`). Shared side `touch_sampler.rs:176-181`: block on
`sem_take(Timeout::Ms(PERIOD_MS))` while the last sample was touched, `Timeout::Ms(1000)` (safety net)
when idle; add `pause()/resume()` wired next to `sensors::sampler::pause` in the display-sleep branch.
Ring/dedup unchanged. Verify with the scroll parity harness and `pdb sysmon` switch counts.

**WP5 — gSPI DMA completion by IRQ (F7).** `pio_spi.rs`: `INTE1` for ch4/5, `DMA_IRQ_1` handler at
priority 0x10 giving a binary semaphore (or `task_notify_from_isr` to the poll task the port already
holds via `cyw43_set_poll_task`); `wait_dma_done` = `take(Duration::ms(5))`, spin only for frames ≤ 8
bytes; TXSTALL stays a short spin. Keep the design-doc property (frames complete autonomously under
preemption). HIL on `testbench_rp2350w`: http_get, 10× install soak, `instr_rx_*`.

**WP6 — Flash window granularity (F8).** `install/region.rs:126-131` `erase_run`: loop
`F::erase_range(sector_offset(first + i), META_SIZE)` per sector (the `packagemanager/mod.rs:41-43`
path re-parks per call). Measure install time delta and TCP drops during an install soak with
`netdemo` running.

**WP7 — Tick and timebase (F16).** `graphics/lvgl/lifecycle.rs::tick`: take the measured elapsed ms
(`now_ms()` delta) instead of 16; return `lv_timer_handler()`'s "next" and let `tick_source`
reprogram the FreeRTOS timer period (add `tick_timer_set_period` to the seam) or switch the main loop
to `queue_recv(Timeout::Ms(next))`; alarms keep an earliest-deadline and skip the scan until it;
`IDLE_GC_TICKS` → ms. Guard: extend the `LV_DEF_REFR_PERIOD == TICK_PERIOD_MS` scan
(`tick_source.rs:65-103`) to reject a literal in `g.tick(`.

**WP8 — Stop-path correctness (F10, F12).** `monitor_store.rs:106-111` stop-aware failure arm;
`pdb/coordinator.rs` notification instead of the 10 ms poll.

**WP9 — UART TX IRQ (F14).** Defer until a board ships a serial app; land a `spin_until!` bound now.

**WP10 — Sim parity (F17).** Three small changes in `hal/sim/`.

**WP11 — Hot-path polish (F18).** I²C `IC_TAR` cache; XPT2046 batched read; SPI small path takes
`spi_lock`.

**Report landing.** This document; `docs/parity-audit.md` TIM-04 and `docs/quality-roadmap.md`
point at it.

## Offensive guards (catch the smell, not just the instance)

Modeled on what the tree already does: text-scan `#[test]`s over `crates/test_support/source_scan.rs`
with pinned expected counts (`task_affinity.rs`, `rtos/mod.rs::seam_guard`, `cfg_gates`), and the
zero-cost opt-in `mem-diag` monitor with strict/abort mode in soaks.

**G1 — Spin ledger (build-time, `cargo test`, runs in `scripts/test.sh` → CI and `pre-commit --full`).**
New `platforms/rp/src/spin_guard.rs` (`#[path]`-includes `source_scan.rs` like `task_affinity.rs`;
no shadow twin in core). It walks `platforms/rp/src/**/*.{rs,c,h}` and
`crates/picodroid-core/src/{drivers,hal,os,install}/**` (comment-stripped) and rejects:
- the primitives `cortex_m::asm::delay(`, `asm::delay(`, `asm::nop(`, `core::hint::spin_loop(`,
  `__asm volatile("nop")`, `cyw43_delay_us(`;
- empty-body waits: `while <cond> {}` / `while (<cond>) { <nothing or asm barrier> }`;
- sleep-poll shape: a `loop {`/`for` body that contains both a `delay_ms(`/`CurrentTask::delay(` and
  an `Atomic*.load(` or a `.receive(Duration::zero())` within ~8 lines;
unless the line (or the line above) carries `// spin-ok: <reason>` / `/* spin-ok: <reason> */`, or
the wait is expressed with `spin_until!` (G5). The number of `spin-ok` markers is pinned
(`EXPECTED_SPIN_OK`) like `EXPECTED_PROVIDERS`, so adding one is a visible review event, and the
test prints the ledger so it can be pasted into `docs/scheduling-diagnostics.md`. Self-test:
`the_matcher_knows_a_spin` with positive/negative literals (the `is_kernel_symbol` pattern).
- Seed of the ledger after WP1–WP3: de-reset polls, `adc/mod.rs:60`, `pio_spi.rs:601-603`,
  `psram.rs` bring-up, `gt911.rs:101`, `flash.rs:299-301`, `core1_park.rs` RAM loop, the SMP
  spinlocks, `queue_read_byte_busywait`'s tick-frozen branch.
- Companion in shared code: extend `rtos/mod.rs::seam_guard::BANNED_TOKENS` with `cortex_m::asm`,
  `spin_loop`, `asm::delay` (shared code may only wait through the seam or `DelayNs`).

**G2 — Config assertions (build-time).** Extend `task_affinity::kernel_config_matches_the_model`
(`platforms/rp/src/task_affinity.rs:378-407`, `assert_defined`) to require `configUSE_IDLE_HOOK 1`,
`configUSE_PASSIVE_IDLE_HOOK 1`, `configUSE_TIME_SLICING 0`, and add text asserts that
`cyw43_port.c`'s `cyw43_delay_ms` gate is `ms >= 1`, that `CYW43_DO_IOCTL_WAIT` /
`CYW43_SDPCM_SEND_COMMON_WAIT` expand to `cyw43_delay_ms`, and that `platforms/rp/Cargo.toml`'s
`defmt-rtt` carries `disable-blocking-mode` (or the `log-lossless` gate). A `cfg_gates`-style row is
not needed: the cargo test already runs in CI.

**G3 — Delay-type guard.** With WP2 there is exactly one `DelayNs` on the device; G1 bans
`cortex_m::asm::delay` everywhere but `delay.rs`. Under `sched-diag` (G4) `RpRtosDelay` also asserts
at runtime: a spin ≥ 1 ms while `scheduler_running()` logs `schedmon: BUSYDELAY` and aborts in strict
mode — this catches a driver that wires the wrong delay type on a new board, which no text scan can.

**G4 — `sched-diag` runtime monitor (cargo feature, zero-cost when off, mirrors `mem-diag`).**
- Feature `sched-diag` in `platforms/rp/Cargo.toml` and `crates/picodroid-core/Cargo.toml`;
  `./scripts/sim.sh --sched-diag`, `PICODROID_EXTRA_FEATURES=sched-diag` for firmware (RP2040:
  manual opt-in only, like mem-diag's flash caveat).
- Data source: the trace facility already on. In the diag `FreeRTOSConfig.h` define
  `traceTASK_SWITCHED_IN()` / `traceTASK_SWITCHED_OUT()` to record `switched_in_us[core]` and bump a
  per-task switch counter (TCB `uxTaskNumber` as index), and `configUSE_TICK_HOOK 1` with
  `vApplicationTickHook` doing one comparison per core: current task via
  `xTaskGetCurrentTaskHandleForCore(c)`, its priority, `now - switched_in_us[c]`.
- Rules (device defmt `schedmon: …`, sim `[schedmon] …`, one greppable line per 1 s window like
  `memmon`):
  - **HOG** — a task at priority ≥ 21 (RT band) or the timer task running continuously > 2 ms;
    a tier-15 task running continuously > 1000 ms while another tier-15 task on the same core is
    Ready (`eTaskGetState` at the window edge) → **STARVE**.
  - **POLL** — per window, a task with > 200 switch-ins and < 5 % run share (a sleep-poll storm).
  - **BUSYDELAY** — from G3 and from `cyw43_delay_us` (C-side counter).
  - **SPIN** — from G5: any `spin_until!` that exceeded its soft threshold, by name.
  - Window line: `schedmon: w=N idle0=NN% idle1=NN% sw=NNN hog=0 poll=0 spin=0`, plus per-task CPU %
    in `pdb sysmon` (delta of `run_time_counter`, `platforms/rp/src/pdb/platform.rs:114`) — useful
    with the feature off too.
  - `PICODROID_SCHEDDIAG_STRICT` (sim env; device build-baked like `mem_diag::apply_device_flags`):
    any HOG/POLL/BUSYDELAY/SPIN → `abort()`, turning soaks into hard failures.
  - `PICODROID_SCHEDDIAG_SELFTEST`: inject a 5 ms spin on the sensor task once → must print `HOG`.
- Sim: the host FreeRTOS kernel gets the same hooks (`freertos-host/FreeRTOSConfig.h`:
  `configGENERATE_RUN_TIME_STATS 1` with a monotonic µs counter, tick hook). `SimDelay` is a no-op
  (TIM-04) so BUSYDELAY is device-only; HOG/POLL/STARVE are visible in sim.
- Doc: `docs/scheduling-diagnostics.md` in the shape of `docs/memory-diagnostics.md`.

**G5 — `spin_until!` (HAL helper).** `crates/picodroid-core/src/hal/spin.rs`:
`spin_until!(cond, max_iters, "name") -> Result<(), SpinTimeout>`; under `sched-diag` counts
iterations and reports `SPIN name` past a soft threshold. Every bounded hardware wait that must stay a
spin (DMA abort, FIFO room, reset_done, parker handshake, QMI CSR) is rewritten with it, so each is
named, finite, and visible to the monitor; G1 accepts it without a marker.

**G6 — Soak lanes.** `scripts/test-scheddiag.sh` (sim, strict: helloworld, animdemo, threaddemo,
picoclock, the sim `net` rows), a `sim-run.sh` lane, and `hil-tests.conf` `loop`/`net` rows built
with `PICODROID_EXTRA_FEATURES=sched-diag` expecting `schedmon:` windows and no
`schedmon: (HOG|POLL|BUSYDELAY|SPIN|STARVE)`; the nightly 3 AM / 4 AM runs pick them up.

## Verification

- After every WP: `./scripts/sim.sh --app helloworld`, `--app benchmark`, `timeout -k 2 5
  ./scripts/sim.sh --app blinky`; `./scripts/pre-commit`; `./scripts/pre-commit --full` before push.
- **F1/F9/F7 (WiFi):** `hil-run.sh` `net` rows on `testbench_rp2350w` and `pico_enviro_mon_w`; compare
  DHCP bind time, `instr_tx_*`/`instr_rx_*` counters, and `pdb sysmon` cyw43 CPU % before/after.
- **F2:** RTT timestamps boot → first frame on `pico_touch_kit` (ST7796 + GT911) and
  `pico_enviro_mon` (ST7789); button wake latency on `pico_enviro_mon`.
- **F3/F11/F12:** `pdb` rows; unplug USB mid-`pdb list` → app keeps running; 10× install soak.
- **F5:** `pdb sysmon` touch-task switch count idle vs touching; `parity-bench.sh --hil` scroll
  numbers unchanged (band-height / 10.2 fps baseline).
- **F8:** install soak with `netdemo` resident; TCP drop counters and install duration.
- **F15:** `bootcount` + `power-cycle.sh` (parker unaffected); `pdb sysmon` idle share; bench current
  if a meter is on the bench.
- **Guards:** G1 self-test literals + a deliberate `asm::delay` in a scratch branch must fail
  `scripts/test.sh`; G4 `PICODROID_SCHEDDIAG_SELFTEST=1` must print `HOG` in sim and abort under
  STRICT; `test-scheddiag.sh` green on the four sim apps; one HIL sched-diag `loop` row green on a
  RP2350 slot.

## Appendix A — Scheduler configuration (verified, `platforms/rp/mcus/rp/FreeRTOSConfig.h`)

| Setting | Value | Note |
|---|---|---|
| `configUSE_PREEMPTION` | 1 | |
| `configUSE_TIME_SLICING` | 0 | load-bearing for the lock-free JVM heap (THR-06) |
| `configTICK_RATE_HZ` | 1000 | `delay_ms` quantises to 1 ms |
| `configNUMBER_OF_CORES` / `configRUN_MULTIPLE_PRIORITIES` / `configUSE_CORE_AFFINITY` | 2 / 1 / 1 | real SMP |
| `configUSE_TICKLESS_IDLE` | 0 | idle task never executes `wfi` (port only emits it under tickless idle, `port.c:820`) |
| `configUSE_IDLE_HOOK` / `configUSE_PASSIVE_IDLE_HOOK` / `configUSE_TICK_HOOK` | 0 / 0 / 0 | no hook to add `wfi` or a hog detector today |
| `configUSE_MUTEXES` / `RECURSIVE_MUTEXES` / `COUNTING_SEMAPHORES` / `TASK_NOTIFICATIONS` / `TIMERS` | all 1 | every blocking primitive is available |
| `configUSE_TRACE_FACILITY` / `configGENERATE_RUN_TIME_STATS` | 1 / 1 | per-task run-time counters already exist (`picodroid_get_runtime_counter` = TIMERAWL µs) and are read by `pdb sysmon` |
| `configTIMER_TASK_PRIORITY` | 31 (max) | LVGL 16 ms tick is a software timer on core 0 |

Priority ladder (`crates/picodroid-core/src/task_priority.rs`): idle 0 · sensor sampler 6 ·
all Java 15 · pdb 21 · cyw43 22 · fs worker 22 · touch sampler 23 · flash parker 30 ·
timer task 31.

## Appendix B — Existing enforcement infrastructure (verified)

- `crates/test_support/source_scan.rs` — shared comment-stripping source walker, `#[path]`-included by
  `platforms/rp/src/task_affinity.rs` (spawn/pin scan), `crates/picodroid-core/src/rtos/mod.rs`
  (`seam_guard`: bans raw FreeRTOS API names outside the seam) and `porting.rs`.
- `scripts/pre-commit` `STAGES` table (`guards` lane: `twins`, `cfg_gates`, `hil_conf`, `jvm_env`) — a
  new text guard is one `stage_*` function plus one row.
- `mem-diag` opt-in feature (`docs/memory-diagnostics.md`, `crates/picodroid-core/src/mem_diag.rs`,
  `scripts/test-memdiag.sh`, `PICODROID_MEMDIAG_STRICT/OFFENSIVE`) — the template for a zero-cost
  runtime monitor with strict/abort mode in soaks.
- `pdb sysmon` (`platforms/rp/src/pdb/platform.rs`) — already marshals `uxTaskGetSystemState`
  including `run_time_counter`; per-task CPU share is one subtraction away.

## Appendix C — RTOS layer, thread model, main loops (audit detail)

Seam inventory (`rtos/mod.rs:116-214`): spawn, task_current, scheduler_running, task_notify /
task_wait_notification, u32 + ptr queues, **recursive** mutex only, **binary** semaphore only, tick
timer singleton, delay_ms. Gaps that explain hand-rolled polls below: no `delay_until`, no event
groups, no counting semaphore, **no ISR-safe give/notify** (so `gpio.rs:354-358` reaches around the
seam with raw `freertos_rust::give_from_isr`, and the USB ISR can only set an `AtomicBool`).

### High
- **R-H1 Unbounded spin on a USB ISR flag, priority 21, core 0.** `platforms/rp/src/hal/rp/pdb_usb/mod.rs:528-532`
  `wait_tx_ready`: `while !EP1_IN_DONE.load() { nop }`. Runs on the `pdb` task (RT_1 = 21) per 64-byte
  chunk; if the host stops draining (unplug mid-write, stalled client) it spins forever above the whole
  JVM tier on the JVM's core. Fix: ISR gives a binary semaphore (`give_from_isr`, the `gpio.rs:352-358`
  pattern), task does `sem_take(Timeout::Ms(n))`.
- **R-H2 Seqlock reader spins with no yield.** `crates/picodroid-core/src/os/system_clock.rs:81-92`
  `wall_offset_ms` loops while `WALL_SEQ` is odd. Reader = UI task (15, core 0) once per frame
  (`lifecycle.rs:1943`) and per alarm scan. If the writer (`set_current_time_millis`) is preempted
  between its three stores by a higher-priority task and an equal-priority task gets the core back
  first (no time slicing → the reader never yields to the writer), core 0 livelocks. Fix: writer holds
  an `AtomicSection` across the stores (short, non-blocking), or reader bounds retries and returns last
  good value. _(verify writer's task in review — NTP runs on the background executor, i.e. tier 15 core 0)_
- **R-H3 Unbounded DMA-abort spins.** `platforms/rp/src/hal/rp/dma.rs:214-215` (`while …busy()…{}` ×2,
  UI task tier 15) and `pio_spi.rs:186-201` `abort_dma` (cyw43 task 22, core 1). Error-recovery paths
  with no bound; RP2040-E13 means an abort can fail to retire. Fix: the bounded `wait_dma_done` shape
  already at `pio_spi.rs:172-181` (count → `Err`).
- **R-H4 Aborted `monitorenter` surfaces as `IllegalMonitorStateException` on app stop.**
  `crates/picodroid-core/src/monitor_store.rs:106-111`: `abort_all_child_delays`
  (`platforms/rp/src/pdb/pending.rs:132-141`, `xTaskAbortDelay`) also aborts a block on a mutex; the
  `Timeout::Forever` failure arm maps to `IllegalMonitorState`. Fix: on `Forever`+failure check
  `host::stop_requested()` and return the stop-shaped result `threads::park_loop` already uses
  (`threads.rs:375-377`); re-lock in a loop on a spurious abort. Correctness, not perf.
- **R-H5 Install-path byte reads busy-wait up to 2 s per byte on RP2350.**
  `pdb_usb/mod.rs:503-516` `queue_read_byte_busywait` (non-blocking `receive(0)` in a hot loop on a µs
  timer), called for **every** `read_byte_timeout` from `platforms/rp/src/pdb/platform.rs:35-38`, not
  only inside the tick-frozen `with_xip_disabled!` window that justifies it. Fix: use the blocking
  `queue_read_byte_timeout` (`:492-498`, real `xQueueReceive` with tick timeout) except while a flash
  window is open / `CORE0_PARKED`.

### Medium (sleep-polling / needless wakeups)
- **R-M1 Touch panel polled at 100 Hz at priority 23 with the INT line wired and unread.**
  `crates/picodroid-core/src/hal/touch_sampler.rs:176-181` (`PERIOD_MS = 10`), above the JVM, fs and
  pdb tasks. GT911 INT is `pin_int = 11` (`platforms/rp/boards/pico_touch_kit/board.toml:154`), used
  only for the address-latch reset dance (`drivers/gt911.rs:72-108`); no `enable_edge_irq` for it
  anywhere. Already tracked as S8 in `docs/designs/scroll-performance-2026-09.md`. Fix: arm GP11
  falling-edge IRQ → ISR gives a semaphore → task `sem_take(Timeout::Ms(PERIOD_MS))` (wake on touch,
  then pace while a finger rests — GT911 INT is periodic while touched), the shape of
  `hardware/sensors/sampler.rs:138-170`.
- **R-M2 Alarm table scanned every 16 ms frame.** `lifecycle.rs:414-415 → 1945-1951 → alarms.rs:366-405`:
  full scan + seqlock read + `installed()` per entry each tick even when empty. Fix: cache the earliest
  deadline and skip until reached, or arm a software timer at the earliest deadline that posts
  `MainTask::Wake` (`executors/main_queue.rs:256-258` exists).
- **R-M3 Fixed 16 ms tick regardless of pending work; LVGL's "time till next" discarded and a literal
  16 fed to `lv_tick_inc`.** `graphics/lvgl/lifecycle.rs:111-115` + `lifecycle.rs:387`
  `g.tick(16)`. The blocking wait itself is right (`main_queue::recv_blocking`, `xQueueReceive(Forever)`),
  but 62.5 forced wakes/s with nothing to draw, and on the touch board a 120–200 ms frame advances
  LVGL's clock by 16 ms so animations run slow. Fix: pass measured elapsed ms to `lv_tick_inc`; use
  `lv_timer_handler()`'s return to reprogram the timer period (or let the UI loop compute its own
  deadline and `queue_recv(Timeout::Ms(next))`, which also subsumes R-M2).
- **R-M4 Sim drains child threads by sleep-poll.** `crates/picodroid-core/src/sim_boot.rs:179-183`
  `while live_jvm_children() > 0 { delay_ms(10) }`; the device does it right with a notification
  re-check loop (`platforms/rp/src/boot_tasks.rs:185-190`). Fix: last exiting child `task_notify`s the
  JVM task; loop on `task_wait_notification(Forever)`.
- **R-M5 Overflow joiners sleep-poll at 20 ms.** `crates/picodroid-core/src/threads.rs:92, 386-392, 438`
  (`JOIN_POLL_MS`, `MAX_JOINERS = 4`). Fix: size `joiners` to `MAX_JAVA_THREADS` (16) so overflow is
  unreachable and `poll_ms` disappears, or have `terminate` broadcast to every parked `Park::Join`.
- **R-M6 Sim TCP accept polls every 5 ms.** `crates/picodroid-core/src/hal/sim/net.rs:226-228`. Fix:
  blocking accept with `SO_RCVTIMEO` or a `Condvar`.
- **R-M7 Sim `delay_ms(0)` is not a yield.** `crates/picodroid-core/src/hal/sim/rtos.rs:528-529`
  (`std::thread::sleep(0)`), while `Thread.yield()` → `delay_ms(0)` (`native_handler/threads.rs:270-274`)
  and device `vTaskDelay(0)` does reschedule (`tasks.c:2512`). Fix: `if ms == 0 { yield_now() }`.
- **R-M8 Idle-GC counted in ticks not time.** `lifecycle.rs:353, 422-435` `IDLE_GC_TICKS = 125` ("~2 s")
  becomes ~25 s on the touch board's 200 ms frames. Fix: compare `now_ms()` deltas like `last_input_ms`.
- **R-M9 No core ever executes `wfi`/`wfe` when idle (power).** `configUSE_IDLE_HOOK 0`,
  `configUSE_PASSIVE_IDLE_HOOK 0`, `configUSE_TICKLESS_IDLE 0`; `prvIdleTask` tight-loops on both
  cores. Fix: `configUSE_IDLE_HOOK 1` + `configUSE_PASSIVE_IDLE_HOOK 1` (the kernel's designated
  "manage core activity" hook for core 1, `tasks.c:5879-5891`), each hook body `wfi` (tick or IPI wakes
  it; cyw43 host-wake IRQ is on core 1). Tickless idle is a later, larger step (port's
  `vPortSuppressTicksAndSleep` is single-core SysTick code) and only worth it once R-M3 makes the tick
  demand-driven.

### Low / by design (not to change)
- `core1_park.rs:130-132, 142-145` spin on `PARKED` — correct by design (core 0 waits for core 1's
  priority-30 parker; nothing on core 0 can help). Cheap insurance: a spin bound + panic.
- `pio_spi.rs:172-181, 573-581` bounded 20 M-iteration spins with `Err` — the right shape.
- `spi/mod.rs:187-206` FIFO polls for small transfers, `uart.rs:134,138` TX-FIFO-full spin,
  peripheral de-reset polls (`gpio.rs`, `input_pin.rs`, `dma.rs:50`, `i2c/mod.rs`, `pio_spi.rs:253`) —
  sub-µs or pre-scheduler.
- `input_inject.rs` `delay_ms` gesture pacing on the pdb task — blocks, does not spin.
- Panel/controller settling delays in `drivers/st7789|st7796|gt911.rs` — datasheet-mandated, boot only
  (but see audit 1 for the `RpDelay` = `asm::delay` implementation they run through).
- `monitor_store.rs:333` `yield_now` is inside `#[cfg(test)]` — seed was a false positive.

### GC / safepoints / app stop — verified sound
Collections run inside `AtomicSection` (`vTaskSuspendAll`/`xTaskResumeAll`, `gc/mod.rs:294-299`);
parked frames stay scannable via `GcState::parked_frames`; the stop flag is polled every 256
bytecodes (`interpreter/mod.rs:649-658`) on the running path only, and blocked threads are woken by
`xTaskAbortDelay` + `threads::wake_all_parked`. No spin anywhere in this path.

### Good patterns to hold up as the reference
`executors/serial_worker.rs:116-130` (ptr queue + notify + done re-check), `executors/main_queue.rs:150-203`
+ `lifecycle.rs:385` (blocking `queue_recv(Forever)` with tick coalescing and a `Wake` sentinel),
`executors/background_pool.rs:128-135`, `threads.rs:360-396` `park_loop` (one wait primitive for
sleep/join/wait with deadline + interrupt + stop checks), `monitor_store.rs` (one kernel recursive mutex
per monitor → priority inheritance for free), `boot_tasks.rs:185-234` notification re-check loops,
`hardware/sensors/sampler.rs:138-170` (`sem_take(Ms)` as pacing + control point),
`hal/freertos_tcp/mod.rs:49-74` + `hal/rp/cyw43/link.rs:74-107` (IRQ is the wake, 1000 ms timeout is
the safety net), `gpio.rs:352-402` (ISR `give_from_isr` → task take), `spi/mod.rs:232` (DMA done
semaphore, 5 s bound, behind a bus mutex), `hal/event_ring.rs` (SPSC Acquire/Release, no RMW).

## Appendix D — HAL and drivers (audit detail)

### High
- **H-H1 `RpDelay` is a cycle burn used after the scheduler starts.** `platforms/rp/src/hal/rp/delay.rs:22-25`
  `delay_ns` = `cortex_m::asm::delay(ns / NS_PER_CYCLE)`. Two construction sites, `display.rs:59` and
  `touch.rs:144`, both reached from `graphics/lvgl/lifecycle.rs:58-59` (`hal::display::init()`,
  `hal::touch::init()`), i.e. on the JVM task (15, core 0) via `boot_tasks.rs:141`. Interrupts stay
  enabled, so the tick and the 21/22/23 tasks run; what is starved is the sensor sampler (6), the
  background pool (15) and idle.
- **H-H2 ST7789 init ≈ 350 ms.** `drivers/st7789.rs:106,108,112,116,129,133` (10+120+150+50+10+10).
- **H-H3 ST7796 init ≈ 540 ms.** `drivers/st7796.rs:169,171,173,192,194` (3×100 + 120 + 120). The touch
  sampler task is spawned *after* this (`lifecycle.rs:60`), so nothing samples the panel for half a
  second at startup.
- **H-H4 Display wake = 120 ms spin, sleep = 5 ms, at runtime.** `st7789.rs:227-237`,
  `st7796.rs:295-305` `sleep_out`/`sleep_in`; reached from `lifecycle.rs:375` `with_gfx(|g| g.wake())`
  → `graphics/lvgl/mod.rs:106` → `hal/rp/display.rs:129-134`. The first 120 ms of every wake-from-idle
  is a spin on the UI task immediately before a full repaint.
- **H-H5 GT911 reset ≈ 76 ms.** `drivers/gt911.rs:92,105,110` (10 + 6 + 60 ms); `:101` `delay_us(120)`
  is genuine µs timing and must stay a spin. Called from `touch.rs:147` inside `build_touch()`.
- **H-H6/H7 = N-0 / N-M2** (cyw43 delay threshold, 150 ms `cyw43_delay_us`). Also
  `cyw43_bthci_uart.c:324` `cyw43_delay_us(5000)` (BT, not built today).
- **H-H8 = R-H1** (`wait_tx_ready`). Adds: the file already has `InterruptContext` machinery (`:333`)
  and a RX `Queue` (`:398`) to put a semaphore beside.
- **H-H9 Flash window: interrupts masked on both cores for the whole ROM op — forced, but granularity is
  a choice (= N-M5).** `flash.rs:183-228` `cpsid i` + `connect(); exit_xip(); $body; flush()`;
  `core1_park.rs:82-101` RAM-resident `cpsid i` + `ldr/cmp/bne` loop. Correct design; the only lever is
  one sector per park cycle.

### Medium
- **H-M1 = N-H5** (gSPI DMA-done spin per frame; 20 M-iteration guard ≈ 2 s, two orders past useful).
- **H-M2 = R-H3** (unbounded DMA abort spins; the documented RP2040 abort hazard).
- **H-M3 SPI small path: polled byte-by-byte, no bus lock, and the XPT2046 hot path.**
  `spi/mod.rs:184-208` (`poll_write_raw!`/`poll_transfer_raw!`) for `len <= SMALL_XFER_THRESHOLD = 8`
  (`spi/xfer.rs:7`), from `write_raw_start:400-406` and `transfer_raw:458-464`. ~1 µs per panel command —
  fine — but (a) the small path returns before `spi_lock` is taken (`:400` vs `:408`), so a 1–6 byte
  command from the UI task can interleave with a locked `reconfigure()` (`:369-374`); (b)
  `xpt2046.rs:140-152` issues `NUM_SAMPLES * 2 = 10` three-byte polled transfers plus two
  `set_frequency` (each taking the bus mutex) per `sample()` at frame rate ≈ 120 µs spin per read.
  Batch into one ≥ 9-byte ISR transfer.
- **H-M4 UART TX per-byte spin, no IRQ path.** `uart.rs:126-142`; default 9600 baud (`:103`) → ~1.04 ms
  per byte once the 32-deep FIFO fills; a 100-byte Java `write` ≈ 70 ms spin at tier 15. `read_byte`
  (`:145-167`) returns `-1`, pushing polling into Java. No TX/RX IRQ handler exists.
- **H-M5 = R-H5** (`queue_read_byte_busywait`; hybrid recommended).
- **H-M6 I²C controller-disable spin on every transfer.** `i2c/mod.rs:502, 563` (and `apply_speed!`
  `:112`) `while ic_enable_status_busy() {}` at the head of every `write_internal`/`read_internal` —
  bounded by ~one SCL bit time (10 µs at 100 kHz) but on the hot path of every sensor and GT911
  transfer (100×/s with F5). Skip the disable/enable when `IC_TAR` is unchanged.
- **H-M7 = F5** (touch poll; GT911 INT bound at `touch.rs:148` and dropped).
- **H-M8 `xip_cache_clean_all!` (2048 volatile stores) runs inside the interrupts-off flash window.**
  `psram.rs:431-442`, expanded at `flash.rs:194`; tens of µs — does not move H-H9's needle.
- **H-M9 RP2040 core-1 launch handshake has no timeout.** `pico_shim_rp2040.c:93-101` (bare `wfe` after
  an outer `sev`) vs `pico_shim_rp2350.c:111-120` (5 M tries + `sev; wfe` inside the loop, the correct
  shape — an event set before `wfe` is consumed, not lost). Port the timeout back.
- **H-M10 SMP spinlocks spin with interrupts off** (`pico_shim.h:60-65, 96-102`) — the kernel's
  `portENTER_CRITICAL` substrate, held for a handful of instructions; by design.
  `best_effort_wfe_or_timeout` (`:144-157`) is a correct non-spinning stub.
- **H-M11 = N-M1** (`cyw43_yield` no-op on core 1).

### Low (table)
Peripheral de-reset polls (`gpio.rs:411-421`, `spi/mod.rs:290-301`, `i2c/mod.rs:210-221`,
`uart.rs:63-74`, `adc/mod.rs:19-24`, `pwm/mod.rs:72-77`, `dma.rs:50`, `pio_spi.rs:253-255`,
`trng.rs:46`, `input_pin.rs:30-31`, `pdb_usb/mod.rs:410`) — few cycles, idempotence-guarded;
`adc/mod.rs:60` ~2 µs; `system_clock.rs:27-50` hi-lo-hi TIMERAW retry (correct lock-free read);
`pio_spi.rs:601-603` 100 ns; `psram.rs:215-255` pre-scheduler (`main.rs:117` precedes
`start_tasks`); `flash.rs:299-301`; `port/pico.h:33` and `main.rs` fault loops; `gt911.rs:101`.

### Good patterns (HAL)
1. **I²C is the reference driver**: ISR only masks + `give_from_isr` (`i2c/mod.rs:73-81`); `wait_for`
   re-arms the level mask and blocks with a 200 ms timeout (`:448-487`, `:322`); per-bus lock
   semaphores (`:618-658`); module doc `:1-23` explains the level-vs-edge race avoided.
2. **Display DMA**: `dma.rs:229-255` `DMA_IRQ_0` at 0x10, RX-drain channel carries completion,
   `give_from_isr` into `SPI0_DONE`/`SPI1_DONE`.
3. **Asynchronous band flush**: `spi/mod.rs:387-443` (`write_raw_start`/`write_raw_finish`) +
   `st7796.rs:236-253` / `st7789.rs:169-186` + `display.rs:94-100` wired to LVGL `flush_wait_cb`
   (`graphics/lvgl/lifecycle.rs:254-256`): render band N+1 while band N is in flight, block on a
   semaphore, CS/bus-lock ownership carried across the gap.
4. **SPI full-duplex ISR-driven with FIFO seeding**: `spi/mod.rs:76-120, 212-227`.
5. **Buttons fully IRQ-driven**: `gpio.rs:247-302` (per-core CPUID branch `:265`), `:350-361`,
   `:396-402`; `lifecycle.rs:367` sleeps the whole UI task in screen-off on it.
6. **CYW43 host-wake IRQ** (`gpio.rs:199-235` → `cyw43_port.c:304-311`), and the doc comment at
   `gpio.rs:173-198` on banked-NVIC routing.
7. **Link service loop** notification-driven with a 1000 ms safety net (`freertos_tcp/mod.rs:71-74`,
   `link.rs:77`).
8. **Flash parker blocks until asked** (`core1_park.rs:64-73`), RAM-resident park loop `:80-101`.
9. **JVM supervisor re-check loops** (`boot_tasks.rs:185-234`, contract at `:173-181`).
10. **USB RX** queue + DPRAM flow control (`pdb_usb/mod.rs:314-347, 451-488`).
11. **TRNG never blocks** (`trng.rs:96-106`, LCG fallback `entropy.rs:33-45`).
12. **Touch ring** lock-free SPSC with dedup (`touch_sampler.rs:148-174, 218-231`).
13. **LVGL tick is a FreeRTOS timer posting to a queue** (`tick_source.rs:34`, pause/resume
    `glue.rs:583-609`).
14. **Seam sleeps are real sleeps** (`hal/rp/system_clock.rs:9-11`, `glue.rs:609`
    `CurrentTask::delay`); `cyw43_delay_ms` for `ms >= 2` already yields.
15. **`cyw43_thread_enter/exit` is a real recursive mutex** (`cyw43_port.c:251-267`).

## Appendix E — Networking, WiFi glue, storage, sensors, logging (audit detail)

Task map used: flashpark 30/core 1 · touch 23/core 0 · cyw43 22/**core 1** · fs 22/core 0 ·
pdb 21/core 0 · jvm 15/core 0 · FreeRTOS+TCP IP task **7** (`FreeRTOSIPConfig_family.h:19`) ·
sensor 6/core 0.

### Root cause N-0 — `cyw43_delay_ms(1)` is a hard `nop` spin (verified)
`platforms/rp/src/hal/rp/port/net/cyw43_port.c:45-52`:
```c
void cyw43_delay_ms(uint32_t ms) {
    if (ms >= 2 && xTaskGetSchedulerState() == taskSCHEDULER_RUNNING) vTaskDelay(pdMS_TO_TICKS(ms));
    else cyw43_delay_us(ms * 1000);          // nop loop on the µs timer
}
```
and `cyw43_configport.h:153-154` wires both driver wait hooks to exactly `cyw43_delay_ms(1)`:
`CYW43_DO_IOCTL_WAIT` and `CYW43_SDPCM_SEND_COMMON_WAIT`. Every *polling* `cyw43_delay_ms` in the
vendored driver passes `1` (`cyw43_ll.c:1239,1242,1295,1325,1471,1536,1584,1615,1679,1727`); only the
fixed settling delays pass 2/20/50/250 and yield. **One comparison (`>= 1`) converts every finding
N-H1..N-H3 below from spin to yielded ticks.** (`vTaskDelay(1)` at 1 kHz sleeps to the next tick edge,
0–1 ms; the driver's timeouts are µs-timer based, not iteration based, so a shorter sleep is safe.)

### High
- **N-H1 `cyw43_do_ioctl` spins up to 500 ms per ioctl at priority 22 on core 1, driver mutex held.**
  `third_party/cyw43-driver/src/cyw43_ll.c:1170-1195` (`CYW43_IOCTL_TIMEOUT_US = 500000`,
  `cyw43_config.h:122`), 1 ms nop-spin per iteration. A join issues dozens; `cyw43_clm_load`
  (`:1362-1394`) issues one per CLM chunk. The recursive driver mutex (`cyw43_port.c:251-267`) is held
  throughout, so the IP task's TX path queues behind it. Proper fix after N-0: host-wake ISR
  (`cyw43_port.c:304-311`, already `vTaskNotifyGiveFromISR`) also satisfies an ioctl-completion wait
  (`xTaskNotifyWait` with the 500 ms as timeout) — the chip already asserts GP24 when a control
  response is pending.
- **N-H2 SDPCM TX-credit stall spins up to 1 s on the IP task (priority 7).**
  `cyw43_ll.c:651-695`: `xCYW43_Output` (`NetworkInterface_CYW43.c:88-119`) calls
  `cyw43_send_ethernet(..., false)` synchronously under `cyw43_thread_enter()`; when flow control or
  credits stall it loops `CYW43_SDPCM_SEND_COMMON_WAIT` for up to 1 s holding the CYW43 mutex, so the
  cyw43 RX task (22) also stalls. Fix: N-0, then give a semaphore when credits arrive in the RX path so
  `xNetworkInterfaceOutput` blocks (`ipconfigZERO_COPY_TX_DRIVER 0`, so the buffer is already copied).
- **N-H3 F2-ready boot wait spins up to 3 s on core 1.** `cyw43_ll.c:1708-1730` (a `// PICODROID:`
  patch: 3000 × `cyw43_delay_ms(1)`). The comment records the *previous* bug was the µs timer making
  this a no-op — the intent was always a yielding delay. Fix: N-0; medium-term `vTaskDelay(5)` here.
- **N-H4 `defmt-rtt` blocks (spins) any logging task when a probe is attached and the ring fills.**
  `platforms/rp/Cargo.toml:85` `defmt-rtt = "1"` (1.1.0) without `disable-blocking-mode`;
  probe-rs sets `MODE_BLOCK_IF_FULL`, and `channel.rs:30-38` then loops `blocking_write` until the host
  drains. Affects the cyw43 task at 22 on core 1 (`link.rs:64` log path) and the IP task
  (`hal/freertos_tcp/mod.rs:97,99`). Unbounded. Every HIL "task X stalled" timing is contaminated by
  this. Fix: enable `disable-blocking-mode` for device builds (lossy but non-blocking), or reserve
  blocking mode for an explicit `log-lossless` feature; document either way.
- **N-H5 gSPI transport spins on DMA completion for every frame, both directions; `DMA_IRQ_1` unused.**
  `platforms/rp/src/hal/rp/pio_spi.rs:168-182` `wait_dma_done` (bounded, 20 M spins) + TXSTALL spin
  `:573-581` + the unbounded abort spins `:187-202`. Per the module's own comment the longest legal
  frame is ~450 µs, so 50–450 µs of pure spin per packet on the cyw43 task (RX) **and the IP task**
  (TX). `pio_spi.rs:231` sets `irq_quiet` "never raises DMA_IRQ_0 (display owns it)", but `DMA_IRQ_1`
  is unclaimed in the tree (`dma.rs:72-74,231` only claim IRQ_0). Fix: route ch4/5 to `INTE1`, a
  `DMA_IRQ_1` handler that gives a binary semaphore, `sem.take(Duration::ms(5))` in
  `cyw43_spi_transfer`; keep the spin for ≤ 8-byte frames (the `SMALL_XFER_THRESHOLD` shape from
  `spi/xfer.rs:7`).
- **N-H6 = R-M1 (touch polled at 100 Hz, IRQ wired and dropped).** Additional evidence: both panels
  bind the IRQ pin as an input and discard it — `platforms/rp/src/hal/rp/touch.rs:89`
  `let _irq = RpInputPin::new(TOUCH_PIN_IRQ, true)` (XPT2046 PENIRQ, which `drivers/xpt2046.rs:94`
  deliberately enables) and `touch.rs:148` `let _int_in = …TOUCH_PIN_INT…` (GT911). No
  pause-on-display-sleep gate either (the sensor sampler has one, `sampler.rs:36-48`). Raised to High
  here because it is the highest-priority application task waking 100×/s forever.

### Medium
- **N-M1 `cyw43_yield()` → bare `taskYIELD()` is a no-op on core 1.** `cyw43_port.c:313-317`, wired as
  `CYW43_EVENT_POLL_HOOK`; called once per 64-byte block of the ~230 KB firmware download
  (`cyw43_ll.c:434`) and in CLM load. With slicing off and `CORE1_TASKS = [flashpark, cyw43]`, a yield at
  22 can only hand to flashpark (30). Fix: `vTaskDelay(1)` if lower work is meant to breathe, else
  delete the hook so it does not read as a yield point.
- **N-M2 150 ms hard spin in CYW43 bring-up.** `cyw43_ll.c:1907-1912` `cyw43_delay_us(150000 - dt)`.
  Own fork; convert to `cyw43_delay_ms((150000 - dt + 999) / 1000)`.
- **N-M3 `wait_for_park` sleep-polls an atomic at 10 ms for up to 15 s.**
  `platforms/rp/src/pdb/coordinator.rs:36-46` on the pdb task (21). The reverse channel already exists
  (`jvm_task` blocks on `take_notification`, woken by `pending::notify_jvm()`); the JVM→pdb direction is
  missing. Fix: record the pdb task handle, `notify()` it right after `CORE0_PARKED` is set
  (`boot_tasks.rs:229-230`), `wait_for_park` = `take_notification(true, Duration::ms(15_000))` re-check
  loop. Saves up to 10 ms per install step.
- **N-M4 = R-H1 (USB `wait_tx_ready` spin).** Adds: RX side is done right (IRQ-driven endpoint, blocking
  queue, `pdb_usb/mod.rs:451-498`), and `queue_read_byte_busywait` is the legitimate exception only
  inside a tick-frozen window (`pdb/mod.rs:56-63`).
- **N-M5 Whole-run PAPK erase in one XIP-off window freezes both cores for seconds.**
  `crates/picodroid-core/src/install/region.rs:126-131` `erase_run` → one `erase_range` for all
  sectors → `platforms/rp/src/hal/rp/flash.rs:250-261` under a single `park_core1_for_flash()`.
  `core1_park.rs:4-6` states the cost: "45 ms per sector, seconds for a PAPK slot"; a 512 KB run ≈ 5.8 s
  with interrupts masked on both cores — host-wake IRQ lost, IP task frozen, CYW43 RX FIFO overflows,
  TCP times out. Fix: erase per sector (or per 8) re-taking the park guard each time; LittleFS already
  does one 4 KB block per call (`platforms/rp/src/fs/storage.rs:74-77`). Cross-check with
  `instr_rx_drop_*` (`NetworkInterface_CYW43.c:45`) during an install soak.
- **N-M6 = core1_park unbounded spins** (see R Low) — add a cap + panic so a missing parker fails loudly.
- **N-M7 UART TX spins on FIFO-full from a JVM task.** `platforms/rp/src/hal/rp/uart.rs:126-142`,
  reached from Java via `pio/uart.rs:10` → `native_handler/pio.rs:98` at tier 15. 32-deep FIFO, ~87 µs
  per byte at 115200: a 256-byte write ≈ 19 ms spin. Fix: TX IRQ + semaphore (the `i2c/mod.rs:435-487`
  pattern) or at least a bound. Low urgency until a board ships a serial app.

### Low (unavoidable hardware timing — no action)
`pio_spi.rs:253,255` reset_done; `pio_spi.rs:601-603` 16-iteration IRQ_SAMPLE_DELAY (~100 ns,
required); `adc/mod.rs:60` (~2 µs conversion); `psram.rs:215-255` one-time QMI bring-up spins;
`psram.rs:431-442` cache clean inside the XIP-off window; `spi/mod.rs:184-208` FIFO polls gated to
≤ 8 bytes (exemplary); `spi/mod.rs:242` few-µs `bsy` drain; `i2c/mod.rs:112,502,563` enable-status
settling; `bme688/mod.rs:88-97` `poll_ready(max_polls=1)` is a single status read (the retry cadence
lives in the sampler state machine — correct); `flash.rs:299-301` post-watchdog `loop { nop }`.

### Done right (protect in review)
- **Java sockets → blocking FreeRTOS+TCP with `SO_RCVTIMEO`; zero polling anywhere in `net/`.**
  `hal/freertos_tcp/mod.rs:311-319` (`tcp_recv` single blocking call, 0 → TimedOut, ENOTCONN → EOF),
  `:342-353` (`FREERTOS_SO_RCVTIMEO`), `:431-439` (blocking accept), `:391-410` (UDP), `:322-332`
  (send), `:462-476` (blocking DNS); `ipconfigSOCK_DEFAULT_RECEIVE_BLOCK_TIME` left at `portMAX_DELAY`;
  `http_connection.rs:200-214` connect timeout via RCVTIMEO swap; header/body loops are blocking drains.
- **CYW43 RX is IRQ-driven** (NET-5 done): `link.rs:74-77` `SERVICE_TIMEOUT_MS = 1000` is the safety
  net; `hal/freertos_tcp/mod.rs:71-74` blocks on `task_wait_notification(Ms(1000))`;
  `cyw43_port.c:304-311` `vTaskNotifyGiveFromISR` + `portYIELD_FROM_ISR`; `:283-296` ISR-vs-task
  dispatch branches on `__get_current_exception()`; `gpio.rs:247-280` level-high IRQ masked in handler
  and re-armed via `CYW43_POST_POLL_HOOK`; `NetworkInterface_CYW43.c:164-202` RX hand-off with zero
  block time, drop-to-counter.
- `i2c/mod.rs:435-487` — the model driver: level IRQ + `Semaphore::take(ms)`, drain stale wake before
  arming, re-check on wake, TX_ABRT in every mask; module doc: "The CPU never busy-spins on peripheral
  state."
- `dma.rs` + `spi/mod.rs:229-244` display DMA → `DMA_IRQ_0` → semaphore, 5 s bound, abort path.
- `hardware/sensors/sampler.rs:134-170` — deadline scheduling on `sem_take(Ms)`, `Forever` when idle or
  display asleep, `MAX_WAIT_MS = 1000`, `kick()` collapses control changes; BME688 trigger→wait→read is
  a state machine, not a 45 ms delay. Priority 6. Textbook.
- `media/driver.rs:191-200` + `media/sequencer.rs` — tones advance on the existing 16 ms tick with an
  `AtomicBool` fast path; no task, no delay loop.
- `trng.rs:96-106` non-blocking with LCG fallback; `pdb_usb/mod.rs:451-498` IRQ-driven RX with flow
  control; `core1_park.rs:64-74` parker blocks on a notification until asked.
- Alarms ride the 16 ms tick (`alarms.rs:361`), and `tick_source::pause()` during display sleep
  consistently stalls them (`alarms.rs:79` documents alarms do not wake a sleeping device).

### Vendored vs owned
Both network dependencies are the user's own forks and already carry picodroid patches
(`third_party/cyw43-driver` = shivrajora/cyw43-driver@`picodroid`, `cyw43_ll.c:1708,1915` inline
`// PICODROID:`; `third_party/freertos-plus-tcp` = shivrajora fork one commit ahead of V4.4.1). The
root cause N-0, N-H4, N-H5, N-H6, N-M1, N-M3..N-M7 live in project code; the loop *structures* of
N-H1/N-H2/N-H3/N-M2 are in the cyw43 fork (fixable there, and N-0 removes the spin without touching
them). FreeRTOS+TCP's own ARP/SYN retry ladders are stack timers on the IP task, not spins.

