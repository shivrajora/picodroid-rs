# Nightly triage: the first three-slot HIL run — 2026-09-09

The 4 AM fleet run of 2026-09-09 (`6ad877b`, slots `testbench_rp2350`,
`pico_enviro_mon_w` and, for the first time in the nightly, `testbench_rp2040`)
came back with 20 FAILs on each RP2350 slot and 35 ERRORs + 9 FAILs on the
rp2040 slot. The 3 AM sim run had one ERROR. This note records what each of
those was, how it was proven, and what changed. Two root causes explain all of
it: the rp2040 firmware runs out of heap on several rows, and a dead USB device
anywhere on the bench stalls every `probe-rs` launch on the host.

## 1. RP2350 slots: 40 rows failed on an empty RTT log

**Symptom.** Every FAIL on both RP2350 slots had a 0-byte log: `probe-rs run`
ran for the full row budget and wrote nothing — not the flash's `Finished in`
line, not even its `plugdev` warning. The same row failed on both slots within
seconds of each other. Nothing in the firmware was involved.

**Mechanism.** A USB device that answers a connect with silence (the rp2040
board, see §2) puts the kernel through ~85 s of enumeration retries per connect
(`Device not responding to setup address`, `device descriptor read/64, error
-110`, `attempt power cycle`, `unable to enumerate USB device`), and for that
whole time the hub driver holds the **parent hub's** `usb_device` lock. probe-rs
starts by listing probes, which nusb does by reading every USB device's sysfs
string attributes — and those attribute reads take the same lock. A probe-rs
launched into a storm on hub `1-8` therefore blocks in `read(2)` on
`/sys/.../usb1/1-8/manufacturer` until the storm ends. Caught live with the
watcher script during a reproduction fleet run: the stuck process had
`fd 11 -> /sys/devices/.../usb1/1-8/manufacturer`, one thread in `read(11, …)`,
the rest parked on futexes. With a 30 s row (65 s budget with the flash
allowance) any storm that starts before probe-rs gets past its listing is
enough; the 85 s storms, one per kernel-driven reconnect of the dead board,
ran back to back all night.

The two RP2350 boards hang off hub `1-8.3`, a child of `1-8`; the sysfs read
that blocks is of `1-8` itself. Moving the rp2040 pair to another hub would
not help: probe-rs reads every device, so a storm on *any* hub blocks it.

**Fix (harness).** `fleet-lib.sh::wait_usb_quiet` reads every
`/sys/bus/usb/devices/*/manufacturer` in the background, treats one that has not
returned in 2 s as a storm in progress and waits (3 s polls, 150 s cap,
`PICODROID_USB_QUIET_MAX_S`) before letting the caller start probe-rs; the
blocked readers are disowned and finish when the kernel gives up. `hil-run.sh`
calls it (`usb_quiet`, logged into the console) before every `probe-rs`
launch: the row's `run`, `wait_for_probe`'s `list`, `hil_flash_elf`, and the
three `reset`s ahead of RTT attaches. The wait runs *before* the row's
`timeout` starts, so a storm costs wall time, never a verdict.
`test-device-lock.sh` §18 tests the guard against a fake sysfs
(`PICODROID_USB_SYSFS`) where a FIFO stands in for the storming device.

## 2. rp2040 slot: 35 probe-rs ERRORs, and the board that never stopped storming

**Symptom.** `Error: No connected probes were found.` on 35 rows; between
them, the slot's `uhubctl … -p 1,2 -a cycle` took 93 s and ended with the probe
port reading `Port 2: 0000 off`. `dmesg` carried 98 dead storms on `1-8.1` (the
board) that night, each 85–86 s.

**Mechanism.** The board on `1-8.1` was dead on USB for most of the night
because its firmware was faulting (§3): after a panic the RP2040 sits in the
HardFault handler with D+ still pulled up, and after a power cycle it boots
the same installed app and panics again. Each connect starts a storm; during a
storm `uhubctl`'s control requests to hub `1-8` queue behind the kernel's lock
(hence 93 s), and the kernel's own `attempt power cycle` of port 1 in the
middle of our two-port cycle left the hub reporting port 2 unpowered. With the
probe off, every row errored before flashing, so the faulting app was never
replaced, so the storms never stopped — the run's steady state.

**Fix (harness).** `power_cycle_slot` now reads back the board and probe ports
after the cycle and re-issues `-a on` for any that reads `off` (five tries, 2 s
apart, `ensure_port_powered`). Verified on the bench with the dead board
connected: the cycle blocks for the storm, then both ports read powered and
`probe-rs list` sees the probe.

**What the bench still owes.** After the fixes the board on `1-8.1` never
enumerated once in the morning — not with the new image, not with the
previous night's debug image, not after a clean cycle of its port or of the
whole hub `1-8` (which brought both RP2350 boards up at once). The kernel
sees a connect and gets no answer to SET_ADDRESS every time, while RTT shows
the firmware running; on the night before it enumerated 71 times in stretches.
That is the board's USB link (cable, connector, or the pack's USB path), a
hands-on job: reseat or swap the rp2040 board's USB cable. Until then its
`pdb` rows SKIP (`no CDC device`), and `term`/`loop` rows run over SWD as
before.

## 3. rp2040 firmware: out of heap

The rows that reached the board and failed all ran the RP2040's 128 KB
FreeRTOS arena dry, and every one of them reproduces in the simulator with
`--board testbench_rp2040` (the nightly sim matrix runs the RP2350 board only,
which is why none of this was seen before the board joined the bench):

| row | on the board | in the rp2040 sim (128 KB arena) |
|---|---|---|
| `langsuite_kt_stdlib` | `memory allocation of 2104 bytes failed` → HardFault after `CollectionsKt === ALL PASSED ===` | OOM at 1536 B with 3.5 KB free |
| `gcstress` | `Application.onCreate error: StackOverflow` after `object_churn` (the error a failed `mutex_recursive_create` / intern maps to) | OOM on a 20 KB array with 14 KB free |
| `threadstress`, `threadparity` | `Thread.start: task spawn failed` → `OutOfMemoryError` | the 16 KB child stack: OOM at 16,504 B; heap_4 fragmented into 13 KB holes |
| `imagedemo` | HardFault right after `ImageDemo ready` | — (LVGL pool) |
| `benchmark` | the 300 s budget ran out at `object_allocation` | — (the chip is ~2.2× slower: `int_arithmetic` 33.4 s vs 14.8 s) |

Where the RAM went: `.bss` was 227,936 of 262,144 B — the 128 KB arena, LVGL's
own `work_mem_int` pool at lv_conf.h's default **64 KB** (the Enviro boards run
their full UI on 48 KB), the 12.8 KB display band buffer. Inside the arena,
`pdb sysmon` after helloworld showed 51 KB free: the boot stacks (jvm 16 KB,
pdb 8, fs 8, four 4 KB pool workers, …) and the JVM's tables leave the app
some 35–50 KB, and the mem-diag sim put the JVM's live set at 22 KB with
115 KB of the arena in use when gcstress died.

**Fix (firmware).** `testbench_rp2040/board.toml` sets `lv_mem_kb = 48`;
`rp2040.toml` raises `heap_kb` to **160**; `boot_budget.rs` gives the RP2040
8 KB `Thread.start` stacks (half the chip's interpreter stack, the same ratio
as the RP2350's 16 KB of 32 KB — the pool workers run Java on 4 KB). The
release image is 244,384 B of RAM with 17,760 B of main-stack headroom above
the 8 KB floor. In the rp2040 sim threadstress and threadparity now pass;
gcstress and langsuite_kt_stdlib still do not (they were written against the
RP2350's 408 KB arena — see the peak figures in `hil-tests.conf` next to their
`rp2350` filter), so those rows are RP2350-only, like `jucdemo` and
`jsondemo` already were. `hil-run.sh` doubles term/loop/hw timeouts on an
rp2040 slot. On the board afterwards: threadstress, threadparity, benchmark
and gcstress_kt PASS.

**Still open: `imagedemo` on the rp2040.** With either RAM budget the board
HardFaults (pc 0, no panic message) right after `ImageDemo ready`, the
moment the image is first rendered to the ST7789 — the rp2040 sim, which has
no display driver, runs it clean. A device-only bug in the image render path
on this board; the row stays in the matrix and reports as a probe-rs ERROR
(`Firmware exited unexpectedly: Exception`) until it is fixed.

## 4. Sim run: `picoenvmon-enviro-w[no-shrink]` build error

`cannot find value RELEASE in crate::board_cfg::build_info`: the 3 AM run
builds from the live working tree, and the multi-app QA session was committing
`d9eb684` (Build.VERSION.RELEASE) between 03:38 and 03:50 — the lane compiled
a half-applied tree. The lane passes on `6ad877b`
(`sim-run.sh --app picoenvmon --mode no-shrink`: 2 PASS). Nothing to fix.

## What stays: one storm per rp2040 flash

Flashing halts the core with its USB pull-up still on, and the RP2040's USB
stack is firmware, so the kernel sees a connect it cannot enumerate at every
flash of that slot and storms for ~85 s (the RP2350 boards re-enumerate
within a second). `wait_usb_quiet` turns that into a wait — an rp2040 `pdb`
row after a flash waits ~87 s for the kernel to give up, and RP2350 rows on
the other slots may wait the same before their own flash — never a verdict.
The one host-side lever, not applied: `usbcore`'s
`initial_descriptor_timeout` (5000 ms by default,
`/sys/module/usbcore/parameters/initial_descriptor_timeout`) is most of each
retry; 1000 ms would cut a storm to ~20 s.

## Reproduction and tooling

- The storm: `sudo uhubctl -l 1-8 -p 1 -a cycle` with a faulting board on
  port 1 (or a core halted by the debugger), then `dmesg -T | grep 1-8`.
- Catching a stuck probe-rs: poll `pgrep -x probe-rs` for a process older
  than ~50 s whose stdout file is empty; `/proc/<pid>/fd` shows the sysfs
  file, `/proc/<pid>/task/*/stack` the `read`.
- The heap: `./scripts/sim.sh --app <app> --board testbench_rp2040`; add
  `--mem-diag` for the JVM live set (`[memmon] … live=… nused=…`).
