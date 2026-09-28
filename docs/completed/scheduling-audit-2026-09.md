# Completed: Scheduling Audit — busy-waits, polling and blocking — 2026-09-12

Items closed out of [scheduling-audit-2026-09.md](../scheduling-audit-2026-09.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## Status

| Work package | State |
|---|---|
| WP1 quick wins (F1, F4, F6 opt-in, F9, F13, F15, F19 bounds + joiners) | **landed** 2026-09-12 |
| G5 `spin_until!` | **landed** 2026-09-12 (`crates/picodroid-core/src/hal/spin.rs`, a porting seam item) |
| G1 spin ledger | **landed** 2026-09-12 (`platforms/rp/src/spin_guard.rs`; 14 `spin-ok`, 5 `spin-todo`: F14 ×2, F18 ×3). **Paid down to 0 `spin-todo`** 2026-09-13 session 3 (F18 by WP11, F14 by WP9's blocking half) |
| G2 config assertions | **landed** 2026-09-12 (`task_affinity::idle_cores_sleep_and_driver_waits_yield`) |
| WP2 kernel-backed `RpDelay` (F2) | **landed** 2026-09-12 (`platforms/rp/src/hal/rp/delay.rs`; the type is changed in place, so no cycle-only delay remains) |
| WP3 USB bridge (F3, F11) | **landed** 2026-09-12 (`pdb_usb/mod.rs`: EP1-IN semaphore given from the ISR, 500 ms dead-host latch; install reads block a tick per attempt on a hardware-timer deadline) |
| WP4 touch by interrupt (F5) | **landed** 2026-09-12 in the interrupt-accelerated form (`HalTouch::wait_irq`; both edges on the INT pin → touch semaphore; 10 ms ceiling touched, 50 ms net idle); touch kit 2026-09-13: the first run exposed `Gt911::read_point` treating a stale buffer as a release (phantom release per INT edge — drags stalled, rollers stepped backwards); fixed in the driver, see S8 in `docs/designs/scroll-performance-2026-09.md` |
| WP6 per-sector PAPK erase (F8) | **landed** 2026-09-12 (`install/region.rs::erase_run`) |
| WP8 stop-path correctness (F10, F12) | **landed** 2026-09-12 (`monitor_store.rs` returns `Interrupted` on an aborted `Forever` lock; `wait_for_park` blocks on a notification the JVM task sends) |
| WP10 sim parity (F17) | **landed** 2026-09-13 (child drain by notification, `delay_ms(0)` yields, `accept` waits in `poll(2)`) |
| F19 leftover (`cyw43_yield`) | **landed** 2026-09-13 (hook is `((void)0)`; nothing else may run on core 1 at priority 22) |
| WP5 gSPI DMA completion by IRQ (F7) | **landed** 2026-09-13 (`pio_spi.rs`: ch4/5 on `INTE1`/`DMA_IRQ_1`, one loud channel per frame > 8 bytes, semaphore take then busy-bit confirm; short frames spin). W slot: netdemo, http_get, blinky loop + install-stress PASS |
| WP11 hot-path polish (F18) | **landed** 2026-09-13 (`select_target`: the I²C target register is its own cache; XPT2046 sample = one 30-byte interrupt-driven transfer; SPI polled paths under `spi_lock`, pending DMA writes collected only by their starter). The handover's bus-hold API was not needed — no XPT2046 board has a touch task; see the handover doc |

## Remediation plan (work packages, in order)

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

**WP8 — Stop-path correctness (F10, F12).** `monitor_store.rs:106-111` stop-aware failure arm;
`pdb/coordinator.rs` notification instead of the 10 ms poll.

**WP10 — Sim parity (F17).** Three small changes in `hal/sim/`.

**WP11 — Hot-path polish (F18).** I²C `IC_TAR` cache; XPT2046 batched read; SPI small path takes
`spi_lock`.

**Report landing.** This document; `docs/parity-audit.md` TIM-04 and `docs/quality-roadmap.md`
point at it.
