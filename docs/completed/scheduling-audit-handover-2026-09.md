# Completed: Scheduling Audit Handover — 2026-09-13 (remaining work)

Items closed out of [scheduling-audit-handover-2026-09.md](../scheduling-audit-handover-2026-09.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

## 0. Where things stand

- **The touch kit has a soft-power button** (EP-0172 carrier): a USB port
  power-cycle switches it off, and it stays off until someone presses the
  button — no SWD, no USB, looks like a dead chip. `hil-run.sh` cycles the
  slot at startup and in `recover_probe`, so every harness run killed it
  (the 2026-09-11 nightly, twice on 2026-09-13). Fixed the same evening:
  `fleet-lib.sh` `cycle=probe` (cycle the probe port only) on the kit's
  fleet row. With that, the kit passes helloworld 25/28 (every install,
  uninstall and launch-soak reboot), animdemo 2/2 and alarmdemo 2/2 on
  main at `57f99632`. The two `settings-uninstall` rows fail on the kit:
  the flow reaches `Settings: apps 1` and the uninstall tap never lands —
  the row's tap coordinates are the 320×240 testbench's, the kit's panel
  is 320×480. A harness item. Also found on the way: `flash_trigger_reset`
  wrote the RP2040 `PSM.WDSEL` mask on RP2350, whose layout differs (the
  RP2040 value there resets the oscillators and not the cores); it now
  writes the SDK's "all but ROSC/XOSC" for chip-rp2350. Not the kit's
  killer (the RP2350A survived the old mask), but wrong.

## 1. The spin ledger's debts — PAID 2026-09-13 (0 `spin-todo`, 14 `spin-ok`)

Each `spin-todo:` named its finding; the work package that replaced one
lowered `EXPECTED_SPIN_TODO` in `spin_guard.rs`. Session 3 retired all five:

| Where | Finding | What replaced it |
|---|---|---|
| `uart.rs` `write_byte`, ×2 | F14 | `wait_tx_room` (`1254ce59`): sleeps a tick per retry while `TXFF` is set, drops the byte after 100 ms (one byte-time is all a slot needs; only a line not draining at all gets there) and warns once per boot; a pre-scheduler caller gets a capped `spin_until!`. WP9's TX ring + `UARTx_IRQ` still waits for a board that ships a serial app — this is the "at minimum" half. |
| `i2c/mod.rs` `apply_speed!`, `write_internal`, `read_internal` | F18 | `select_target` (`43274aa1`): the register is the cache — when `IC_ENABLE` is set and `IC_TAR` already holds the address there is nothing to do. The genuine waits (a reconfigure, a real target change) share one out-of-line `disable()` with a `spin_until!` cap. A failed transfer leaves the controller disabled so the next select flushes the FIFOs the way the old unconditional disable did. |

## 2. Open work packages, in the order I would take them

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
