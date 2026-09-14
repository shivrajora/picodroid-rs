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

**Session 3, 2026-09-13 (six commits on local `main`, not pushed):** WP11
in three parts (`43274aa1` I²C target skip, `ab35c53f` XPT2046 batch,
`spi/mod.rs` polled paths under the lock), the blocking half of WP9
(`1254ce59`) and WP5 (`49590df9`, gSPI DMA completion by interrupt) landed, each through the fast tier and
sim smoke, `--full` at the end. The spin ledger is 14 `spin-ok` / 0
`spin-todo`. The ratchet moved four times, each attributed by symbol in the
commit: RP2040 +160 / −240 / +158 / −808 B, RP2350 −137 / −151 / +160 /
−208 B. Bench: netdemo/http_get/blinky rows on the W slot for WP5 (§2), the
blinky and helloworld rows on `testbench_rp2040` for the SPI lock (§2 WP11).
Still open after this session: WP0, G3/G4/G6 (`sched-diag`), the WP7 timer
half, WP9's ring, and the two bench items below.

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

## 1. The spin ledger's debts — PAID 2026-09-13 (0 `spin-todo`, 14 `spin-ok`)

Each `spin-todo:` named its finding; the work package that replaced one
lowered `EXPECTED_SPIN_TODO` in `spin_guard.rs`. Session 3 retired all five:

| Where | Finding | What replaced it |
|---|---|---|
| `uart.rs` `write_byte`, ×2 | F14 | `wait_tx_room` (`1254ce59`): sleeps a tick per retry while `TXFF` is set, drops the byte after 100 ms (one byte-time is all a slot needs; only a line not draining at all gets there) and warns once per boot; a pre-scheduler caller gets a capped `spin_until!`. WP9's TX ring + `UARTx_IRQ` still waits for a board that ships a serial app — this is the "at minimum" half. |
| `i2c/mod.rs` `apply_speed!`, `write_internal`, `read_internal` | F18 | `select_target` (`43274aa1`): the register is the cache — when `IC_ENABLE` is set and `IC_TAR` already holds the address there is nothing to do. The genuine waits (a reconfigure, a real target change) share one out-of-line `disable()` with a `spin_until!` cap. A failed transfer leaves the controller disabled so the next select flushes the FIFOs the way the old unconditional disable did. |

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

### WP5 — gSPI DMA completion by interrupt (F7) — LANDED 2026-09-13 (session 3)

`pio_spi.rs`: channels 4/5 are on `INTE1`; `DMA_IRQ_1` (priority 0x10,
unmasked on the cyw43 task's core) gives a binary semaphore; per transfer
exactly one channel is left loud through `IRQ_QUIET` — RX in the read shape
(it finishes last), TX in the write shape — and only when the frame is
longer than 8 bytes and the scheduler is running. `wait_dma` takes the
semaphore (5 ms) and then confirms with the busy bit, which keeps the old
spin's correctness: the token only makes the wait cheap. A take can return
early on a token the previous transfer's interrupt left after its own spin
had already seen the channel idle (so every transfer drains the semaphore
first), and it can time out with the channel legitimately still running
when core 1 sat parked for a flash write longer than 5 ms with the
interrupt pending behind `cpsid` — either way the busy bit decides. The
TXSTALL wait after a write stays a spin: at most the FIFO's four words.
A fresh semaphore rather than a queue token: `pio_spi.rs` is W-board-only
(`network_cyw43`), so §4's RP2040 ISR-entry-point cost does not apply, and
the I²C/SPI drivers already link `give_from_isr` on every board. Frames
still complete autonomously under preemption (nothing about the SM/DMA
programming changed).

Validated on the W slot (`testbench_rp2350w` firmware): `netdemo` ×2 and
`http_get` ×2 `net` rows PASS (firmware load, join, DHCP, TCP echo, HTTP
GET), `blinky` `loop` + `pdb install-stress` (10/10) PASS. The bench misbehaved
mid-run — the probe dropped off USB during one `http_get` row and answered
"interface busy" on two `blinky` rows, and a no-shrink `install-stress`
failed only because its flash never happened (the board still ran the
previous shrink image and rejected the no-shrink PAPK) — those rows passed
on the re-run. The `instr_rx_*` gdb readback was not done.

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

### WP11 — hot-path polish (F18) — LANDED 2026-09-13 (session 3)

Parts 1 and 2: the I²C `IC_TAR` skip (§1, `43274aa1`) and the XPT2046
batch (`ab35c53f`): `sample()` sends its ten conversions as one 30-byte
transfer — the chip latches a control byte on the first clock after a
conversion's 24, so back-to-back frames under one CS are what the wire
already carried, minus the gaps — which takes the RP driver's
interrupt-driven path instead of ten trips through the polled small path.
The sim's `FakeXptSpi` answers every frame of a batch now (it answered only
the first), and driver tests over a recording bus pin the shape.

Part 3, the SPI small path: see the correction below.

**The 2026-09-13 session-2 analysis of the small-path lock was wrong about
the task topology, and the bus-hold API it asked for is not needed.** The
touch sampler task only starts where `TOUCH_PRIVATE_BUS` is set
(`touch_sampler.rs::start`), and no XPT2046 board sets it — all three
(`testbench_rp2040`, `testbench_rp2350`, `testbench_rp2350w`) put the panel
on the display's bus, so on every one of them the panel is read inline on
the UI task, after the refresh has collected its last band. There is no
"touch task at priority 23" to race the JVM task's band, and the XPT2046
sample cannot interleave with a flush because the same task does both. What
the lock-free small path did leave open is a *different* task on the
display's bus — a Java `SpiDevice` on SPI0 — whose 1–8 byte poll could put
bytes into a running DMA band or run while `reconfigure` had the controller
disabled, and whose `write_raw_start` would collect the UI task's pending
band as if it were its own. Part 3 (`spi/mod.rs`): both polled paths take
`spi_lock`; a pending DMA write records its starter's task handle and only
that task collects it (`collect_own_write`, on every entry to the bus
including `reconfigure`) — another task takes the lock and blocks until the
owner has. Deadlock-free on the UI task by the panel driver's existing
contract: every command collects the in-flight band before it goes out.
Validated on `testbench_rp2040` (XPT2046 on the display's SPI0): the
`blinky` `loop` and `pdb install-stress` rows in both modes and every
`helloworld` row PASS, the display refreshing and the inline touch read
running throughout; the `pdb launch` rows SKIP on that slot's single-app
firmware, so scripted taps during a repaint remain unexercised there.

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
- WP9's TX ring + `UARTx_IRQ`: `write_byte` no longer spins (§1), but it
  still blocks the writer a tick at a time; the ring waits for a serial app.
- WP0 (ISR-safe seam primitives): WP5 stayed family code and used
  `freertos_rust` directly, like `gpio.rs`; still no shared consumer.
- The `blinky pdb launch` row on the W slot (§0).
