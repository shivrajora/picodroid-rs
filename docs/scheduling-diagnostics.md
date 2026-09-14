# Scheduling Diagnostics (`sched-diag`)

Opt-in instrumentation for the scheduling smells the 2026-09 audit ranked
(`docs/scheduling-audit-2026-09.md`): a real-time task that holds its core,
a task that sleep-polls, an equal-priority task starved by a peer that
never blocks, a delay type that burns cycles instead of sleeping, a
`spin_until!` that spins for longer than it should. Everything here is
gated behind the `sched-diag` cargo feature — **when the feature is off,
none of it exists in the binary**: the kernel is compiled without the trace
and tick hooks, and the monitor module is not built (see "Zero-cost
guarantee" below).

The memory monitor (`docs/memory-diagnostics.md`) samples the heap from the
UI tick. This monitor is fed by the kernel itself: FreeRTOS's trace hooks
report every context switch and every ready transition, its tick hook looks
at what each core is running once a millisecond, and a printer — the idle
task on a device, a host thread in the simulator — prints the result. So it
works for an `Application` with no Activity and no UI tick, and a window
the printer could not get to in time is itself a finding.

Audience: developers and AI agents. Every command below is copy-pasteable.

## Enabling

| Where | How |
|---|---|
| Simulator | `./scripts/sim.sh --app <app> --sched-diag` |
| Firmware (any board) | `PICODROID_EXTRA_FEATURES=sched-diag ./scripts/flash.sh -b testbench_rp2350 -a <app>` |
| Firmware (RP2040) | Same, but a **manual opt-in only**, like mem-diag: the monitor adds ~7.0 KB of flash and ~0.6 KB of RAM to the debug image (measured 2026-09-14 on `testbench_rp2040` + helloworld: 817,520 → 824,572 B flash, 244,392 → 245,000 B RAM), so check the flash gate's headroom first |
| Soak suite | `./scripts/test-scheddiag.sh` (also runs as a `sim-run.sh` lane, once per nightly cycle) |
| Per-task CPU share on a device | `./scripts/pdb.sh sysmon` — `CPU%` is the delta of the kernel's run-time counter between two calls; it needs no feature |

Runtime toggles within a `--sched-diag` sim build (read once at boot):

| Env var | Default | Effect |
|---|---|---|
| `PICODROID_SCHEDDIAG_WINDOW_MS` | `1000` | Window length (min 100 ms) |
| `PICODROID_SCHEDDIAG_STRICT` | off | Any finding → `abort()` right after the report (turns soaks into hard failures) |
| `PICODROID_SCHEDDIAG_SELFTEST` | off | Once the first window has closed, hold the timer task for 5 ms — the next window must print `HOG Tmr Svc` (detector self-test) |

On device there are no env vars: the window is 1 s, and `STRICT` /
`SELFTEST` are baked at **build** time (`PICODROID_SCHEDDIAG_STRICT=1
PICODROID_EXTRA_FEATURES=sched-diag ./scripts/flash.sh …`, the
`mem_diag::apply_device_flags` pattern; strict on a device panics through
the normal panic handler). A sched-diag image always logs `scheddiag: ACTIVE`
at boot — a capture containing that line is a diag build.

## Reading the output

One line per window, greppable by `schedmon` (sim `[schedmon] …`, device
RTT `schedmon: …`), followed by one line per finding:

```text
[schedmon] scheddiag: ACTIVE (window=1000ms strict=off selftest=off)
[schedmon] w=1 ms=1016 idle0=99% sw=188 hog=0 poll=0 starve=0 busy=0 spin=0 lost=0
[schedmon] tasks: IDLE=99%/60 jvm=0%/61 Tmr Svc=0%/62 jvm-bg=0%/1 jvm-bg=0%/1 fs=0%/1
```

| Field | Meaning |
|---|---|
| `w` | Window index |
| `ms` | The window's real length. Windows close on the first context switch after the period, so a few ms over is normal; a window much longer than the period means nothing switched for that long |
| `idle0` / `idle1` | Idle task's share of the window on each core (the device prints both cores; the single-core simulator prints `idle0`) |
| `sw` | Context switches (switch-ins) in the window. A healthy Activity app at rest shows ~180: the 16 ms tick wakes the timer task and the JVM task 60× each, and each returns to idle |
| `hog` / `poll` / `starve` / `busy` / `spin` | Findings in the window, by rule (below). All zero is the expected steady state |
| `lost` | Windows that closed while the previous report was still unprinted — on a device, the idle task did not run between them. Non-zero means a core was saturated for over a window; the report that follows covers only the last window |
| `tasks:` (sim only) | Top tasks by run share, `name=share%/switch-ins`. On a device this is `pdb sysmon`'s job |

The finding lines:

| Line | Rule | Threshold |
|---|---|---|
| `HOG <task> prio=P core=C ran=N ms` | A task in the real-time band (priority ≥ 21 — pdb, cyw43, fs, touch, flash parker, and the timer service task at 31) seen running by three consecutive tick hooks, i.e. more than 2 ms without blocking. Counted in ticks, not microseconds: a simulator thread the host deschedules for a while has its tick signal pended, not multiplied, so it does not read as a hog. `ran` is the wall-clock length of the occupancy at the end of the window | `HOG_TICKS = 3` |
| `STARVE <task> prio=P ready=N ms` | A task at the JVM tier (15) or above that has been Ready, and not run, for a second. With time slicing off (`task_priority.rs`, "One tier for all Java") an equal-priority task that never reaches a blocking call starves its peers silently; this is what makes that visible. Below the JVM tier a task is *meant* to wait while Java runs, so the rule does not watch there | `STARVE_MS = 1000` |
| `POLL <task> prio=P core=C switch_ins=N ran=M ms` | More than 200 switch-ins in the window and under 5 % of it running — the shape of a task that sleeps a tick, checks, sleeps a tick. The idle task is exempt (returning to idle is what a switch is) | `POLL_SWITCHES = 200`, `POLL_SHARE_PCT = 5` |
| `BUSYDELAY n=N max=M us` | The RP family's `RpDelay` (or the CYW43 port's `cyw43_delay_us`) burned a millisecond or more of cycles while the scheduler was running. Both route millisecond waits through the kernel, so this only fires when a driver reaches the cycle-count path with a millisecond wait — the G3 case: a delay wired wrong on a new board, which no text scan can see. Device-only in practice (the simulator's `SimDelay` is a no-op, parity row TIM-04) | ≥ 1 ms with the scheduler running |
| `SPIN <name> iters=N (n=K)` | A `spin_until!` ran past its soft threshold; `name` is the macro's own tag, `n` how many such spins the window saw. The cap a site passes is a fault bound, not a cadence — a spin that gets anywhere near it is worth a look | `SPIN_SOFT_ITERS = 10 000` |

In strict mode the report is followed by `STRICT: aborting on N finding(s)
in window W` and the process aborts (`SIGABRT`, exit 134).

## What to expect on a device

Some findings are the honest cost of the hardware and will show up in a
device capture without being defects:

- **Flash writes hog core 0.** A LittleFS write or a PAPK install runs its
  erase/program with interrupts off on core 0 (`with_xip_disabled!`), a
  sector erase being ~45 ms on the RP2040. The fs worker (22) or the pdb task
  (21) therefore reports as a `HOG` for the length of it. WP6 keeps those
  windows to one sector at a time; the monitor shows how long each one was.
- **The flash parker is exempt.** `flashpark` (30, core 1) spins in RAM,
  interrupts masked, for the length of every flash operation by design
  (`platforms/rp/src/hal/rp/core1_park.rs`); `boot_tasks.rs` names it to
  `sched_diag::exempt_task` before spawning it, so it is never a `HOG` or
  `STARVE`. That is the only exemption.
- **Idle shares.** `idle1` on a network board is the cyw43 task's complement;
  during a firmware download expect it to drop for a few seconds.

Strict mode is therefore for the simulator soaks; on a device read the
findings against what the app was doing.

## How it is fed

The `PICODROID_SCHED_DIAG` block at the end of each `FreeRTOSConfig.h`
(`platforms/rp/mcus/rp/` for the device, `crates/picodroid-core/freertos-host/`
for the simulator — the same text in both, which
`task_affinity::sched_diag_hooks_are_identical_on_device_and_host` checks)
defines `traceTASK_SWITCHED_IN`, `traceTASK_SWITCHED_OUT` and
`traceMOVED_TASK_TO_READY_STATE` as calls into
`crates/picodroid-core/src/sched_diag.rs`, and turns `configUSE_TICK_HOOK`
on; the build scripts add the define to every C compile when the feature
is on. The hooks run inside the kernel with its locks held, so the task
table is single-writer; they keep, per task, the switch-ins and run time of
the current window and a "ready since" stamp, and per core what is running
and for how many ticks. A window closes when the tick hook or the next
context switch — whichever comes first — finds it due: the closing hook
scans the table for `STARVE` and `POLL`, writes the report into a
sequence-guarded buffer and resets the counters. On a device the idle hook
(`vApplicationIdleHook`, `platforms/rp/src/main.rs`) copies a complete
report and prints it. The simulator prints from a host thread that looks
for a closed window every 5 ms: its idle thread may not run Rust, because
the POSIX port deletes the idle task with `pthread_cancel` at scheduler
end, and a cancel whose forced unwind meets a Rust frame aborts the
process (`hal/sim/rtos_freertos.rs`). At app exit `boot.rs` calls
`sched_diag::flush`, which asks the tick hook to close the window in
progress, whatever its length, sleeps two ticks so the printer can take
it, and prints whatever is still pending itself — so a to-completion app
leaves its figures behind (`helloworld`'s one window is a few
milliseconds long).
`note_spin` and `note_busy_delay` are the two entry points from outside the
kernel.

The kernel's own run-time statistics (`configGENERATE_RUN_TIME_STATS`, on
for the device already) are not what this reads: they give cumulative
per-task run time to `pdb sysmon`; the monitor wants per-window figures and
the ready/preempted distinction, which only the trace hooks provide.

## Typical workflows

- **Is a new board's delay wired right?** Flash the diag image, drive the
  display and touch paths, grep the RTT capture for `BUSYDELAY`. Zero is the
  answer; anything else names a `DelayNs` that reached the cycle counter
  with a millisecond wait.
- **Something feels laggy on the UI thread.** `sw` and `idle0` first: a
  storm of switches with a full idle share is a poll; a low idle share with
  no findings is the JVM task genuinely busy (see `pdb sysmon` for which
  task); a `HOG` names a real-time task holding the core.
- **A Java thread never seems to get time.** Run the app under
  `--sched-diag` in the simulator and look for `STARVE`: the named task is
  Ready and a peer at its priority is not blocking.
- **A soak.** `PICODROID_SCHEDDIAG_STRICT=1 ./scripts/sim.sh --app <app>
  --sched-diag`: the first finding aborts the run with the report printed.
- **Is the detector alive?** `PICODROID_SCHEDDIAG_SELFTEST=1` must print
  `HOG Tmr Svc prio=31 core=0 ran=5 ms` in the second window; with `STRICT`
  it must abort. `scripts/test-scheddiag.sh` runs both.

## Zero-cost guarantee

With the feature off: the `PICODROID_SCHED_DIAG` define is absent from every
C compile, so the kernel carries no trace macros (they fall back to
FreeRTOS's empty defaults) and `configUSE_TICK_HOOK` is 0; `sched_diag.rs`
is not compiled; the `spin_until!` expansion is unchanged (its
`note_spin` call is behind `cfg(feature = "sched-diag")`); the device idle hook,
`tick_source::on_tick`, `RpDelay::delay_ns` and `boot.rs` contain no call.
The size ratchet measures the default image, and the `--full` pre-commit
tier builds the diag image (`build_rp2350w_scheddiag` — the network board,
so `cyw43_port.c`'s counter compiles too) and clippies its host arm
(`clippy_sim_scheddiag`).

## Contributor rules

- **A new `spin_until!` site needs no registration**: the macro reports
  itself. Size its cap against the expected wait, and treat a `SPIN` line
  for it as a fault to explain.
- **A task whose job is to hold a core** gets `sched_diag::exempt_task`
  next to its spawn, with the reason in a comment, and a mention in "What
  to expect on a device" above. There are two exempt slots; the parker uses
  one.
- **The hook block is one text in two files.** Edit both or the config guard
  fails. A hook the block calls must be an `extern "C" fn` in
  `sched_diag.rs`.
- **Thresholds live in `sched_diag.rs` as named constants** with the rule
  they serve; change one there and in the table above together.
- **Do not print from the hooks.** They run inside the kernel; the idle
  hook is the only printer.

## On hardware

`hil-run.sh` honours `PICODROID_EXTRA_FEATURES` like `flash.sh` does, so
any row can be run on a diag image:

```bash
PICODROID_EXTRA_FEATURES=sched-diag ./scripts/hil-run.sh --board testbench_rp2350 --app blinky --no-email
```

First run, 2026-09-14, `testbench_rp2350` firmware on the RP2350 slot,
blinky's `loop` row: the row PASSes and the RTT log reads

```text
scheddiag: ACTIVE (window=1000ms strict=false selftest=false)
schedmon: w=1 ms=1000 idle0=96% idle1=99% sw=14 hog=0 poll=0 starve=0 busy=0 spin=0 lost=0
schedmon: w=2 ms=1000 idle0=99% idle1=100% sw=4 hog=0 poll=0 starve=0 busy=0 spin=0 lost=0
```

— both cores reported, four switches a second while the app sleeps
between toggles, no findings, no lost windows.

## Not covered (yet)

- **HIL rows in the nightly.** `hil-tests.conf` has no sched-diag rows of
  its own yet: one blinky `loop` row has run on a diag image (above); the
  flash-write `HOG`s the device notes predict have not been observed yet
  because that row writes no flash. Read a storage-heavy row's findings
  before making any of them a failure pattern.
- **Idle wake-ups.** The window line shows idle *share*, not how many times
  the core woke. That question belongs to the tick-timebase work (WP7 in
  the audit) and tickless idle, which this monitor is a prerequisite for
  measuring.
- **Interrupt time** is charged to whichever task was running.
