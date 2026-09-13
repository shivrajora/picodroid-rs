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

**Session 2, 2026-09-13 (three commits on local `main`, not pushed):** WP7
(`719d43e7`), WP10 (`61394bae`) and the F19 leftover (`4b65a715`) landed —
each through the fast tier, sim smoke per commit, `--full` at the end. See
§2 for what each did and what WP7 deliberately left out. The size ratchet
was advanced once, in the WP7 commit: RP2040 +92 B flash / +8 B RAM over
the old baseline (+172 B / +8 B over the pre-change tree, attributed by
symbol in the commit message); RP2350 −1,388 B against its baseline.

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

### WP7 — tick timebase (F16) — LANDED 2026-09-13 (`719d43e7`), one half deferred

Done: `tick_source::step_ms()` feeds the UI clocks (LVGL, toasts,
snackbars, property animations) one period while the loop keeps up and
the tick's lateness against the fed clock when it is more — *not* a raw
wall-clock delta, because `lv_timer_exec` re-stamps `last_run` with no
credit and a 15 ms step against the 16 ms refresh period skips a frame on
every millisecond of jitter (S3). Unit-tested, including the u32 wrap.
`Display.update()` keeps a fixed period by contract (`graphicsbench` C
counts frames as `2000 / 16`). Alarms keep a horizon (earliest armed
trigger per clock + "a row is in flight"); `alarms::due` is two
comparisons and no seqlock read on an idle tick, and a package-directory
change forces a poll. Idle GC is 2 s on the clock, not 125 ticks. Guard:
`tick_source`'s scan rejects an integer literal at any `.tick(`,
`lifecycle::tick(` or `lv_tick_inc(` call.

**Deliberately not done — reprogramming the timer from `lv_timer_handler`'s
return.** It would buy nothing today. LVGL 9.5 does pause its refresh timer
when nothing is invalidated (`lv_display_refr_timer` → `lv_timer_pause`;
`lv_inv_area` resumes it), and the anim timer when no animation runs, but
the pointer indev's `read_timer` runs every `LV_DEF_REFR_PERIOD` in
`LV_INDEV_MODE_TIMER`, so `lv_timer_handler` never answers more than 16 ms
while the panel is polled through LVGL. The idle-wake reduction is gated on
event-mode input: the touch sampler (already interrupt-woken after WP4)
would post `main_queue::enqueue_wake` on a state change and the loop would
call `lv_indev_read` with the indev in `LV_INDEV_MODE_EVENT`. Only then is
a `tick_timer_set_period` seam item worth its porting-guard and flash
cost. Note the sim's minifb window pump (`hal::display::update_window`)
also wants a periodic tick, so the sim arm would keep a floor.

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

### WP10 — sim parity (F17) — LANDED 2026-09-13 (`61394bae`)

The FreeRTOS sim backing records the JVM task's handle and `child_gone`
notifies it when the child count reaches zero (both exit paths);
`sim_boot` waits on `task_wait_notification` and re-checks, as
`boot_tasks.rs` does. The test backing's `delay_ms(0)` is `yield_now`.
`accept` with a timeout blocks in `poll(2)` with the deadline tracked
across the 1 ms `SIGALRM` `EINTR`s (`SO_RCVTIMEO` on `accept` is
Linux-only; the dev host is sometimes macOS).

### WP11 — hot-path polish (F18) — small

The I²C `IC_TAR` cache (§1); XPT2046 `sample()` batches its ten 3-byte polled
transfers into one ≥ 9-byte ISR transfer (`drivers/xpt2046.rs:140-152`); the
SPI small path takes `spi_lock` before its FIFO polls (`spi/mod.rs:400-406`
returns before `:408` today — a 1–6 byte command can interleave with a locked
`reconfigure()`).

**Looked at on 2026-09-13, left alone — it needs the testbench.** The
"small" SPI item is not small on the boards that matter: on
`testbench_rp2040/2350` the XPT2046 is a second device on the display's bus
(`board.toml` `[touch]` has no `spi_id`; `RpSpiBus::handle` says so). The
async flush (S5) holds `spi_lock` from `write_pixels_start` to the
`write_pixels_wait` LVGL issues at the end of each refresh, and the XPT2046
sample already serialises on that lock *twice* — its `set_frequency(2 MHz)`
and the restore both go through `reconfigure` — but its ten polled 3-byte
transfers in between do not, so the JVM task can start the next band's DMA
at 2 MHz between them and the sampler's bytes can land in the band's TX
FIFO. Taking `spi_lock` in the small path alone would make the touch task
(priority 23) block behind a band on a binary semaphore with no priority
inheritance, which is correct but new, and does not close the gap. The fix
that does: a bus-hold API (`spi::hold(id)`/`release`, or a `with_bus`
closure) that the XPT2046 driver wraps its whole sample in, with
`reconfigure` and the polled paths asserting or taking it. Validate with
touch during a repaint on `testbench_rp2040` (`parity-bench.sh --hil`,
`pdb input swipe` while a scroll paints) — a corrupt band or a stalled
drag is the symptom either way.

### F19 leftovers — LANDED 2026-09-13 (`4b65a715`)

`cyw43_yield()` is gone; `CYW43_EVENT_POLL_HOOK` is `((void)0)` with the
reasoning in `cyw43_configport.h`: the driver's boot loops run on the
cyw43 task, pinned to core 1 at priority 22, and the only other task
allowed on core 1 is the flash parker at 30, which preempts rather than
waits for a yield. Compile-checked on `testbench_rp2350w` — the ratchet
boards do not link the port, so a W-board build is the only gate for that
file.

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
  is single-core SysTick code; revisit once the tick is event-paced — see
  WP7's deferred half).
- WP7's timer reprogramming (WP7 above): no gain until the indev is
  event-driven.
- WP11's SPI lock (WP11 above): needs the testbench, and a bus-hold API
  rather than the one-line change the plan described.
- The `blinky pdb launch` row on the W slot (§0).
