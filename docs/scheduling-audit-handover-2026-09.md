# Scheduling Audit Handover — 2026-09-13 (remaining work)

Scope: what is left from `docs/scheduling-audit-2026-09.md` — the busy-wait /
polling / blocking audit of 2026-09-12 — after its first landing session
(fourteen commits, `4cad7261..6e68d8c6` on local `main`, **not pushed**). This
doc is the starting point for the follow-up session: read it, then the audit's
"Ranked findings" and "Remediation plan" sections for the evidence behind each
item. Line numbers below were correct at `6e68d8c6`.

Companion context: `docs/scheduling-audit-2026-09.md` (the report, status table
at the top), `docs/designs/scroll-performance-2026-09.md` §S8 (touch interrupt),
`docs/designs/cyw43-pio-transport.md` (gSPI transport), project memory
`project_scheduling_audit_2026_09`, `reference_spin_guard_ledger`,
`feedback_offensive_guards_for_arch_rules`,
`feedback_release_lease_before_detached_hil`.

---

## 0. Where things stand

Landed and gated (fast tier per commit, `--full` green at the end, both boards'
flash below their ratchet baselines): F1, F2, F3, F4, F5, F8, F9, F10, F12, F13,
F15, F19; the `spin_until!` helper (`crates/picodroid-core/src/hal/spin.rs`, a
porting seam item); the spin ledger (`platforms/rp/src/spin_guard.rs`, 14
`spin-ok`, 5 `spin-todo`); the config assertions in
`platforms/rp/src/task_affinity.rs::idle_cores_sleep_and_driver_waits_yield`;
the `rtt-lossy` feature. Hardware: `netdemo` and `http_get` `net` rows and the
`blinky` `loop` + `pdb install-stress` rows pass on the W-board slot with the
final tree.

Two things the next session inherits that are **not** code debt:

- **The `blinky pdb launch` row fails on the `pico_enviro_mon_w` slot with
  `device refused: park timeout`, and it did so before this work too** (bench
  bisect + a control run of the pre-audit tree, all identical). The row needs
  the testbench's XPT2046 touch panel; on that slot the board lacks it and
  the row had only ever SKIPped. `pdb sysmon` during the stall: `jvm` Blocked,
  never parks; the launcher never logs `ready` after `onCreate`. Worth ten
  minutes to understand *why* the launcher stalls on a board with no touch
  chip (the inline XPT2046 read on a floating bus is the suspect — a
  permanent phantom press?), but it is a bench item, not an audit item.
- **WP4 (touch by interrupt) is unrun on hardware.** The touch kit was leased
  all day. First thing on the kit: tap and scroll parity (`parity-bench.sh
  --hil`), then `pdb sysmon` switch counts idle vs. touching. See §3.

## 1. The spin ledger's debts (5 `spin-todo`, all `platforms/rp/src/hal/rp/`)

Each `spin-todo:` names its finding; the work package that replaces one lowers
`EXPECTED_SPIN_TODO` in `spin_guard.rs` (and may raise `EXPECTED_SPIN_OK`).

| Where | Finding | What replaces it |
|---|---|---|
| `uart.rs` `write_byte`, ×2 | F14 | WP9: TX ring + `UARTx_IRQ` TXIM, block on a semaphore only when the ring is full; RXIM → queue (the `i2c/mod.rs:435-487` pattern). Defer until a board ships a serial app; at minimum `spin_until!` the FIFO wait. |
| `i2c/mod.rs` `apply_speed!`, `write_internal`, `read_internal` | F18 | WP11: cache the last `IC_TAR` per bus and skip the disable/settle/enable when it is unchanged (every sensor and GT911 transfer pays ~one SCL bit-time today). |

## 2. Open work packages, in the order I would take them

### WP7 — tick timebase (F16) — shared code, sim-testable, medium

`crates/picodroid-core/src/graphics/lvgl/lifecycle.rs::tick` is fed a literal
`16` (`lifecycle.rs` `g.tick(16)`), so a 200 ms frame on the touch board
advances LVGL's clock by 16 ms and animations run slow; `lv_timer_handler()`'s
"ms until next" is discarded, so the 16 ms software timer fires 62.5×/s with
nothing to draw. Plan: measured `now_ms()` delta into `lv_tick_inc`; use the
return value to reprogram the timer (add `tick_timer_set_period` to the `Rtos`
seam — device arm in `platforms/rp/src/glue.rs` next to `tick_timer_start`,
sim arms in `hal/sim/rtos*.rs`) or let the main loop wait with
`queue_recv(Timeout::Ms(next))`; keep an earliest-deadline for
`alarms::dispatch_alarms` so the per-tick table scan (`alarms.rs:366-405`)
only runs when due; `IDLE_GC_TICKS = 125` (`lifecycle.rs:353`) becomes a
`now_ms()` comparison. Guard: extend the `LV_DEF_REFR_PERIOD == TICK_PERIOD_MS`
scan in `executors/tick_source.rs:65-103` to reject a literal in `g.tick(`.
Verify with `animdemo` in the sim (animation wall-clock) and `pdb sysmon` on
the touch kit (timer-task switch count with an idle screen).

### WP5 — gSPI DMA completion by interrupt (F7) — W board, medium

`platforms/rp/src/hal/rp/pio_spi.rs` `wait_dma_done` (now `spin_until!`, 20 M
cap) spins for every WiFi frame in both directions, on the cyw43 task and on
the IP task (≤ 450 µs each). `DMA_IRQ_1` is unclaimed in the tree (`dma.rs`
only claims IRQ_0). Plan: route channels 4/5 to `INTE1`, a `DMA_IRQ_1` handler
at priority 0x10 that posts a completion (prefer a depth-1 **queue** token or
the existing `cyw43_set_poll_task` notification — see §4 on why not a fresh
semaphore), `take(Duration::ms(5))` in `cyw43_spi_transfer`, keep the spin for
≤ 8-byte frames. Keep the design-doc property that frames complete
autonomously under preemption. Validate on `testbench_rp2350w`: `netdemo`,
`http_get`, 10× `pdb install` soak, and the `instr_rx_*` counters in
`NetworkInterface_CYW43.c` (gdb-read).

### WP0 — ISR-safe seam primitives — small, unblocks shared IRQ-driven waits

`crates/picodroid-core/src/rtos/mod.rs` has no `sem_give_from_isr` /
`task_notify_from_isr` and no `delay_until`; family code reaches around it
(`gpio.rs` raw `give_from_isr`) and shared code cannot express IRQ → task at
all (WP4 sidestepped this with a defaulted `HalTouch::wait_irq`). Add the pair
(returning "higher-priority woken" so the caller can yield) and `delay_until`;
device arms in `glue.rs`, sim arms in `hal/sim/rtos.rs` and `rtos_freertos.rs`;
update the `seam_guard` must-list in `rtos/mod.rs`. Only worth doing when a
shared consumer exists (WP5 or a second family).

### G4 — `sched-diag` runtime monitor + G6 soak lanes — large

The design is in the audit ("Offensive guards" §G4/§G6): a cargo feature
mirroring `mem-diag` (zero-cost off), `traceTASK_SWITCHED_IN/OUT` +
`configUSE_TICK_HOOK 1` in the diag config, rules HOG (RT-band task > 2 ms
continuous), STARVE, POLL (> 200 switch-ins and < 5 % run share per window),
BUSYDELAY (from `RpDelay` — this is G3), SPIN (from `spin_until!` past a soft
threshold), a one-line `schedmon:` window report, STRICT abort for soaks,
`PICODROID_SCHEDDIAG_SELFTEST`, `scripts/test-scheddiag.sh`, `sim-run.sh` and
`hil-tests.conf` rows. `configGENERATE_RUN_TIME_STATS` is already on and
`pdb sysmon` already marshals `run_time_counter`, so per-task CPU % in sysmon
is the cheap first step and useful on its own. Budget a full session.

### WP10 — sim parity (F17) — small

`sim_boot.rs:179-183` drains child threads with a 10 ms sleep-poll (device
uses notifications, `boot_tasks.rs:185-190`); `hal/sim/rtos.rs:528` makes
`delay_ms(0)` a `sleep(0)` while the device's `vTaskDelay(0)` reschedules
(`Thread.yield` divergence); `hal/sim/net.rs:226-228` polls `accept` at 5 ms.

### WP11 — hot-path polish (F18) — small

The I²C `IC_TAR` cache (§1); XPT2046 `sample()` batches its ten 3-byte polled
transfers into one ≥ 9-byte ISR transfer (`drivers/xpt2046.rs:140-152`); the
SPI small path takes `spi_lock` before its FIFO polls (`spi/mod.rs:400-406`
returns before `:408` today — a 1–6 byte command can interleave with a locked
`reconfigure()`).

### F19 leftovers — trivial

`cyw43_port.c` `cyw43_yield()` is a bare `taskYIELD()` on a core with no
equal-priority peer (a no-op that reads as a yield point): delete the hook or
say so in a comment.

## 3. WP4 on the touch kit — what to check first

`platforms/rp/src/hal/rp/gpio.rs` arms GP11 for **both** edges and routes that
pin to `TOUCH_WAKE_SEM` inside `IO_IRQ_BANK0`; `hal/touch_sampler.rs::run`
waits with `PERIOD_MS = 10` while a finger is down and `IDLE_POLL_MS = 50`
idle; scripted touches call `gpio::kick_touch_irq`. Measure what GP11 does
across a gesture (pulse per report vs. level; edge on release?) — S8's open
question. With an edge confirmed at touch-down, raise `IDLE_POLL_MS` toward
1000 (the idle-power win). Add `pause()/resume()` next to
`sensors::sampler::pause` if the kit ever gains a display-sleep path.

## 4. Things that will bite (learned the hard way on 2026-09-12)

- **RP2040 flash is the ratchet.** Three lessons: (1) on `testbench_rp2040`
  nothing else links a semaphore's or a task notification's ISR entry points,
  so a new one costs ~1 KB — a depth-1 **queue** over the already-linked
  queue calls was free (`pdb_usb/mod.rs`); (2) a call inside a loop invites
  LLVM to inline the 660 B XIP-off erase body at every caller — the family's
  `erase_range`/`program_range` are `inline(never)` now, keep them so;
  (3) a fifteen-iteration loop with kernel calls gets fully unrolled unless
  the bound is `black_box`ed (`coordinator.rs::wait_for_park`). Attribute
  before accepting: `nm --size-sort -S target/lane-thumbv6m/thumbv6m-none-eabi/release/picodroid`
  in two stashed states, diff by symbol.
- **`inline(never)` on `RpDelay`'s three methods** — a dozen inlined copies of
  the scheduler check cost 600 B.
- **Bench leases.** A one-off `pdb.sh --board X …` leaves this session holding
  X; a later detached `hil-run.sh` queues silently. `device-lock.sh release`
  before detached bench work. Never run a HIL build while an edit script is
  rewriting Rust files (an http_get row failed "firmware build failed" on a
  half-edited tree).
- **The porting guard counts every `#[macro_export]` macro as a seam item**
  (`porting.rs` `EXPECTED_SEAM_ITEMS`, the re-exports, the numbered list, and
  the website porting guide all have to name it).
- **Gates:** `cargo fmt --all` first; sim smoke (`helloworld`, `benchmark`,
  `timeout -k 2 40 sim.sh --app blinky`) + `./scripts/pre-commit` after every
  change; `--full` before pushing. Commit one finding per commit; hooks run
  the fast tier on the working tree, so do not edit while a commit is in
  flight.

## 5. Not done, deliberately

- `defmt-rtt` stays in blocking mode by default (HIL rows need every line);
  `rtt-lossy` exists for probe-attached timing work. F6 is Medium, not fixed.
- `configUSE_TICKLESS_IDLE` stays 0 (the port's `vPortSuppressTicksAndSleep`
  is single-core SysTick code; revisit after WP7).
- The `blinky pdb launch` row on the W slot (§0).
