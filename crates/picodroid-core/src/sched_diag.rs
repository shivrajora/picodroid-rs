// SPDX-License-Identifier: GPL-3.0-only
//! The `sched-diag` scheduling monitor (docs/scheduling-diagnostics.md).
//!
//! Sibling of [`crate::mem_diag`]: opt-in, one greppable line per window,
//! a strict mode that turns a finding into an abort, and nothing in the
//! binary when the feature is off. Where the memory monitor samples the
//! heap from the UI tick, this one is fed by the kernel itself. The
//! `PICODROID_SCHED_DIAG` block of each `FreeRTOSConfig.h`
//! (`platforms/rp/mcus/rp/` for the device, `freertos-host/` for the
//! simulator) points FreeRTOS's three trace hooks and its tick hook at the
//! `extern "C"` functions below, so every context switch, every
//! ready transition and every millisecond tick reaches this file. From
//! those it keeps a table of live tasks and, once a window (1 s), publishes
//! a report for a printer to take. On a device that is the idle task: the
//! one task that runs whenever the system is healthy, on every kind of app
//! (an `Application` with no Activity has no UI tick to hang this on), and
//! a window it cannot print in time is itself a finding (`lost=`). The
//! simulator prints from a host thread instead — `hal/sim/rtos_freertos.rs`
//! says why its idle thread may not run Rust.
//!
//! # What it looks for (docs/scheduling-audit-2026-09.md, G3/G4)
//!
//! | Line | Rule |
//! |---|---|
//! | `HOG` | a task in the real-time band (priority ≥ [`HOG_PRIO_MIN`], the timer task included) seen running by [`HOG_TICKS`] consecutive tick hooks — more than 2 ms without blocking. Counted in ticks, not microseconds, so a simulator thread the host deschedules for a while (its tick signal is pended, not multiplied) does not read as a hog |
//! | `STARVE` | a task at the JVM tier or above that has been Ready, and not run, for [`STARVE_MS`]: with time slicing off an equal-priority task that never blocks starves its peers silently |
//! | `POLL` | a task with more than [`POLL_SWITCHES`] switch-ins in the window and under [`POLL_SHARE_PCT`] percent of it running — the sleep-poll storm shape |
//! | `BUSYDELAY` | the family's delay type burned a millisecond or more of cycles while the scheduler was running (G3: a driver wired to the wrong delay on a new board, which no text scan can see) |
//! | `SPIN` | a [`crate::spin_until!`] that exceeded [`SPIN_SOFT_ITERS`], by name |
//!
//! The window line carries the idle share per core and the switch count,
//! the figures that say whether the system is doing what it should between
//! the findings. Per-task CPU share on a device comes from `pdb sysmon`,
//! whose `run_time_counter` deltas this monitor does not duplicate; the
//! simulator prints a `tasks:` line because it has no bridge.
//!
//! # Threading
//!
//! The hooks run inside the kernel with its locks held: `vTaskSwitchContext`
//! takes the ISR lock (and the task lock, on SMP), every ready transition
//! happens inside a critical section, and the tick hook runs from the tick
//! interrupt under the same ISR lock on the SMP port and with signals
//! masked on the POSIX one. So the table below is single-writer by
//! construction, kept as plain fields (thumbv6m has no atomic
//! read-modify-write) behind a `static mut` reached through
//! `addr_of_mut!`, the `mem_diag` discipline. Two things reach it from
//! outside the kernel: [`note_spin`] and [`note_busy_delay`], which do
//! plain stores into counters the window reset also clears — a lost count
//! there is tolerated — and the printer, which reads a report the closing
//! hook wrote into a sequence-guarded buffer (odd while writing) and gives
//! up on a torn copy until the next idle pass. Nothing here allocates, and
//! the hooks use no stack to speak of: on Cortex-M they run on the handler
//! stack.

use core::ffi::{c_char, c_ulong};
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{compiler_fence, fence, Ordering};

use crate::task_priority::{PRIORITY_JVM_NORM, PRIORITY_RT_1};

// ── Rules ───────────────────────────────────────────────────────────────────

/// Monitor window when `PICODROID_SCHEDDIAG_WINDOW_MS` is unset (and the
/// device value, which has no env).
const DEFAULT_WINDOW_MS: u32 = 1000;
/// Shortest window the simulator env may ask for.
const MIN_WINDOW_MS: u32 = 100;

/// A task at this priority or above that runs continuously is a HOG: the
/// real-time band, pdb and above, plus the timer service task.
pub const HOG_PRIO_MIN: u8 = PRIORITY_RT_1;
/// Consecutive tick hooks (1 ms each) that must observe the same task
/// running before it is a HOG — three means "more than 2 ms".
pub const HOG_TICKS: u16 = 3;
/// A Ready task at the JVM tier or above that waits this long is STARVEd.
pub const STARVE_MS: u32 = 1000;
/// Lowest priority the STARVE rule watches: below the JVM tier a task is
/// *meant* to wait while Java runs.
pub const STARVE_PRIO_MIN: u8 = PRIORITY_JVM_NORM;
/// Switch-ins per window above which a task with a tiny run share POLLs.
pub const POLL_SWITCHES: u32 = 200;
/// Run share (percent of the window) below which those switch-ins are a
/// storm rather than work.
pub const POLL_SHARE_PCT: u32 = 5;
/// `spin_until!` iterations past which a spin is reported. Around a
/// millisecond of register reads on the RP family.
pub const SPIN_SOFT_ITERS: u32 = 10_000;

// ── Table geometry ──────────────────────────────────────────────────────────

/// Tasks tracked at once. Beyond this the least recently run slot is
/// reused; a Java `Thread` that ends frees nothing until then.
const MAX_SLOTS: usize = 24;
/// Cores the per-core records cover. The RP family's SMP kernel has two;
/// the POSIX port reports only core 0.
const MAX_CORES: usize = 2;
/// HOG/POLL/STARVE findings kept per window; later ones are counted only.
const MAX_FINDINGS: usize = 8;
/// `configMAX_TASK_NAME_LEN`, NUL included.
const NAME_LEN: usize = 16;
/// Task names [`exempt_task`] can hold.
const MAX_EXEMPT: usize = 2;

const NO_SLOT: u8 = 0xFF;
const NO_FINDING: u8 = 0xFF;

const KIND_HOG: u8 = 1;
const KIND_POLL: u8 = 2;
const KIND_STARVE: u8 = 3;

#[derive(Clone, Copy)]
struct Slot {
    /// The kernel's `uxTCBNumber`; unique per task creation. 0 = empty.
    tcb: u32,
    prio: u8,
    /// Core it last ran on.
    core: u8,
    /// Named by [`exempt_task`]: never a HOG or STARVE.
    exempt: bool,
    /// Ready and not running since `ready_since_us`.
    waiting: bool,
    name: [u8; NAME_LEN],
    /// This window.
    switch_ins: u32,
    /// This window, microseconds.
    run_us: u32,
    ready_since_us: u32,
    last_seen_us: u32,
}

impl Slot {
    const EMPTY: Slot = Slot {
        tcb: 0,
        prio: 0,
        core: 0,
        exempt: false,
        waiting: false,
        name: [0; NAME_LEN],
        switch_ins: 0,
        run_us: 0,
        ready_since_us: 0,
        last_seen_us: 0,
    };
}

/// What a core is running now.
#[derive(Clone, Copy)]
struct Running {
    slot: u8,
    prio: u8,
    /// Tick hooks that have seen this same occupancy.
    ticks_seen: u16,
    /// Index of the HOG finding this occupancy opened, if any.
    hog_idx: u8,
    since_us: u32,
}

impl Running {
    const NONE: Running = Running {
        slot: NO_SLOT,
        prio: 0,
        ticks_seen: 0,
        hog_idx: NO_FINDING,
        since_us: 0,
    };
}

#[derive(Clone, Copy)]
struct Finding {
    kind: u8,
    prio: u8,
    core: u8,
    name: [u8; NAME_LEN],
    /// HOG: ran µs. STARVE: waited µs. POLL: switch-ins.
    a: u32,
    /// POLL: ran µs.
    b: u32,
}

impl Finding {
    const EMPTY: Finding = Finding {
        kind: 0,
        prio: 0,
        core: 0,
        name: [0; NAME_LEN],
        a: 0,
        b: 0,
    };
}

/// One task's share of a window, for the simulator's `tasks:` line.
#[cfg(feature = "sim")]
#[derive(Clone, Copy)]
struct Row {
    name: [u8; NAME_LEN],
    run_us: u32,
    switch_ins: u32,
}

/// The window that just closed, as the printer sees it.
#[derive(Clone, Copy)]
struct Report {
    window: u32,
    elapsed_us: u32,
    /// Windows that closed unprinted since the last one the printer took.
    lost: u32,
    cores: u8,
    idle_us: [u32; MAX_CORES],
    switches: u32,
    hog: u32,
    poll: u32,
    starve: u32,
    n_findings: u8,
    findings: [Finding; MAX_FINDINGS],
    busy: u32,
    busy_max_us: u32,
    spin: u32,
    spin_last_iters: u32,
    spin_last_name: [u8; NAME_LEN],
    #[cfg(feature = "sim")]
    n_rows: u8,
    #[cfg(feature = "sim")]
    rows: [Row; MAX_SLOTS],
}

impl Report {
    const EMPTY: Report = Report {
        window: 0,
        elapsed_us: 0,
        lost: 0,
        cores: 0,
        idle_us: [0; MAX_CORES],
        switches: 0,
        hog: 0,
        poll: 0,
        starve: 0,
        n_findings: 0,
        findings: [Finding::EMPTY; MAX_FINDINGS],
        busy: 0,
        busy_max_us: 0,
        spin: 0,
        spin_last_iters: 0,
        spin_last_name: [0; NAME_LEN],
        #[cfg(feature = "sim")]
        n_rows: 0,
        #[cfg(feature = "sim")]
        rows: [Row {
            name: [0; NAME_LEN],
            run_us: 0,
            switch_ins: 0,
        }; MAX_SLOTS],
    };

    fn any_finding(&self) -> bool {
        self.hog + self.poll + self.starve + self.busy + self.spin > 0
    }
}

/// The hooks' table. Single-writer: see the module note on threading.
struct Live {
    window_us: u32,
    window_start_us: u32,
    window_index: u32,
    slots: [Slot; MAX_SLOTS],
    running: [Running; MAX_CORES],
    cores_seen: u8,
    switches: u32,
    idle_us: [u32; MAX_CORES],
    n_findings: u8,
    findings: [Finding; MAX_FINDINGS],
    hog: u32,
    poll: u32,
    starve: u32,
    /// From [`note_busy_delay`] / the C port — outside the kernel locks.
    busy: u32,
    busy_max_us: u32,
    /// From [`note_spin`] — outside the kernel locks.
    spin: u32,
    spin_last_iters: u32,
    spin_last_name: [u8; NAME_LEN],
    lost: u32,
    n_exempt: u8,
    exempt: [[u8; NAME_LEN]; MAX_EXEMPT],
}

impl Live {
    const fn new() -> Self {
        Live {
            window_us: DEFAULT_WINDOW_MS * 1000,
            window_start_us: 0,
            window_index: 0,
            slots: [Slot::EMPTY; MAX_SLOTS],
            running: [Running::NONE; MAX_CORES],
            cores_seen: 1,
            switches: 0,
            idle_us: [0; MAX_CORES],
            n_findings: 0,
            findings: [Finding::EMPTY; MAX_FINDINGS],
            hog: 0,
            poll: 0,
            starve: 0,
            busy: 0,
            busy_max_us: 0,
            spin: 0,
            spin_last_iters: 0,
            spin_last_name: [0; NAME_LEN],
            lost: 0,
            n_exempt: 0,
            exempt: [[0; NAME_LEN]; MAX_EXEMPT],
        }
    }
}

static mut LIVE: Live = Live::new();

/// Hook-side accessor (kernel locks held).
fn live() -> &'static mut Live {
    unsafe { &mut *addr_of_mut!(LIVE) }
}

/// The report the closing hook fills, guarded by [`PENDING_SEQ`] (odd while
/// being written).
static mut PENDING: Report = Report::EMPTY;
static mut PENDING_SEQ: u32 = 0;
/// Last sequence the printer took.
static mut CONSUMED_SEQ: u32 = 0;
/// The printer's copy. A static, not a stack frame: the idle task's stack
/// is the kernel's minimum.
static mut PRINTING: Report = Report::EMPTY;

struct Config {
    strict: bool,
    selftest: bool,
    selftest_done: bool,
}

static mut CFG: Config = Config {
    strict: false,
    selftest: false,
    selftest_done: false,
};

fn cfg() -> &'static mut Config {
    unsafe { &mut *addr_of_mut!(CFG) }
}

// ── Time ────────────────────────────────────────────────────────────────────

extern "C" {
    /// The run-time-stats counter every FreeRTOS-hosted platform provides:
    /// a free-running 32-bit microsecond count (the RP family's hardware
    /// timer, `Instant`-based in the simulator). Wraps every ~71 minutes;
    /// everything here is a wrapping difference.
    fn picodroid_get_runtime_counter() -> u32;
}

#[inline]
fn now_us() -> u32 {
    // SAFETY: a read of a free-running counter, valid from any context.
    unsafe { picodroid_get_runtime_counter() }
}

// ── Names ───────────────────────────────────────────────────────────────────

fn copy_name(dst: &mut [u8; NAME_LEN], src: *const c_char) {
    *dst = [0; NAME_LEN];
    if src.is_null() {
        return;
    }
    for (i, d) in dst.iter_mut().enumerate().take(NAME_LEN - 1) {
        // SAFETY: `src` is the kernel's NUL-terminated
        // `pcTaskName[configMAX_TASK_NAME_LEN]`, read up to its terminator.
        let c = unsafe { *src.add(i) } as u8;
        if c == 0 {
            break;
        }
        *d = c;
    }
}

fn name_from_str(s: &str) -> [u8; NAME_LEN] {
    let mut out = [0u8; NAME_LEN];
    for (d, b) in out.iter_mut().zip(s.bytes().take(NAME_LEN - 1)) {
        *d = b;
    }
    out
}

fn name_str(name: &[u8; NAME_LEN]) -> &str {
    let len = name.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
    core::str::from_utf8(&name[..len]).unwrap_or("?")
}

// ── Slots ───────────────────────────────────────────────────────────────────

fn is_exempt(live: &Live, name: &[u8; NAME_LEN]) -> bool {
    live.exempt[..live.n_exempt as usize].contains(name)
}

/// The slot for task `tcb`, created (evicting the least recently run
/// non-running slot) if it has none.
fn slot_for(live: &mut Live, tcb: u32, prio: u8, name: *const c_char, now: u32) -> usize {
    if let Some(i) = live.slots.iter().position(|s| s.tcb == tcb) {
        return i;
    }
    let mut victim = usize::MAX;
    let mut oldest = 0u32;
    for (i, s) in live.slots.iter().enumerate() {
        if s.tcb == 0 {
            victim = i;
            break;
        }
        if live.running.iter().any(|r| r.slot as usize == i) {
            continue;
        }
        let age = now.wrapping_sub(s.last_seen_us);
        if age >= oldest {
            oldest = age;
            victim = i;
        }
    }
    if victim == usize::MAX {
        // Every slot is running on some core — impossible with two cores
        // and twenty-four slots, but never index out of range for it.
        victim = 0;
    }
    let mut s = Slot::EMPTY;
    s.tcb = tcb;
    s.prio = prio;
    s.last_seen_us = now;
    copy_name(&mut s.name, name);
    s.exempt = is_exempt(live, &s.name);
    live.slots[victim] = s;
    victim
}

fn add_finding(live: &mut Live, f: Finding) -> u8 {
    match f.kind {
        KIND_HOG => live.hog += 1,
        KIND_POLL => live.poll += 1,
        _ => live.starve += 1,
    }
    let n = live.n_findings as usize;
    if n >= MAX_FINDINGS {
        return NO_FINDING;
    }
    live.findings[n] = f;
    live.n_findings += 1;
    n as u8
}

/// Credit the running task on `core` with the time since it was switched
/// in, and restart its clock at `now`.
fn settle_running(live: &mut Live, core: usize, now: u32) {
    let r = live.running[core];
    if r.slot == NO_SLOT {
        return;
    }
    let ran = now.wrapping_sub(r.since_us);
    let s = &mut live.slots[r.slot as usize];
    s.run_us = s.run_us.wrapping_add(ran);
    if s.prio == 0 {
        live.idle_us[core] = live.idle_us[core].wrapping_add(ran);
    }
    if r.hog_idx != NO_FINDING {
        live.findings[r.hog_idx as usize].a = live.findings[r.hog_idx as usize]
            .a
            .max(now.wrapping_sub(r.since_us));
    }
    live.running[core].since_us = now;
}

// ── Window ──────────────────────────────────────────────────────────────────

/// Close the window at `now`: scan for STARVE and POLL, write the report
/// into [`PENDING`], reset the per-window counters.
fn close_window(live: &mut Live, now: u32) {
    let elapsed = now.wrapping_sub(live.window_start_us);
    for c in 0..MAX_CORES {
        settle_running(live, c, now);
    }

    for i in 0..MAX_SLOTS {
        let s = live.slots[i];
        if s.tcb == 0 || s.exempt {
            continue;
        }
        if s.waiting
            && s.prio >= STARVE_PRIO_MIN
            && now.wrapping_sub(s.ready_since_us) >= STARVE_MS * 1000
        {
            add_finding(
                live,
                Finding {
                    kind: KIND_STARVE,
                    prio: s.prio,
                    core: s.core,
                    name: s.name,
                    a: now.wrapping_sub(s.ready_since_us),
                    b: 0,
                },
            );
        }
        if s.prio > 0
            && s.switch_ins > POLL_SWITCHES
            && (s.run_us as u64) * 100 < (elapsed as u64) * (POLL_SHARE_PCT as u64)
        {
            add_finding(
                live,
                Finding {
                    kind: KIND_POLL,
                    prio: s.prio,
                    core: s.core,
                    name: s.name,
                    a: s.switch_ins,
                    b: s.run_us,
                },
            );
        }
    }

    live.window_index = live.window_index.wrapping_add(1);

    // SAFETY: single writer (see the module note); the printer only reads
    // PENDING between two equal even reads of PENDING_SEQ.
    unsafe {
        let seq = read_volatile(addr_of!(PENDING_SEQ));
        if seq != read_volatile(addr_of!(CONSUMED_SEQ)) {
            live.lost = live.lost.wrapping_add(1);
        } else {
            live.lost = 0;
        }
        write_volatile(addr_of_mut!(PENDING_SEQ), seq.wrapping_add(1));
        fence(Ordering::SeqCst);
        let p = &mut *addr_of_mut!(PENDING);
        p.window = live.window_index;
        p.elapsed_us = elapsed;
        p.lost = live.lost;
        p.cores = live.cores_seen;
        p.idle_us = live.idle_us;
        p.switches = live.switches;
        p.hog = live.hog;
        p.poll = live.poll;
        p.starve = live.starve;
        p.n_findings = live.n_findings;
        p.findings = live.findings;
        p.busy = live.busy;
        p.busy_max_us = live.busy_max_us;
        p.spin = live.spin;
        p.spin_last_iters = live.spin_last_iters;
        p.spin_last_name = live.spin_last_name;
        #[cfg(feature = "sim")]
        {
            p.n_rows = 0;
            for s in live.slots.iter() {
                if s.tcb == 0 || (s.switch_ins == 0 && s.run_us == 0) {
                    continue;
                }
                p.rows[p.n_rows as usize] = Row {
                    name: s.name,
                    run_us: s.run_us,
                    switch_ins: s.switch_ins,
                };
                p.n_rows += 1;
            }
        }
        fence(Ordering::SeqCst);
        write_volatile(addr_of_mut!(PENDING_SEQ), seq.wrapping_add(2));
    }

    for s in live.slots.iter_mut() {
        s.switch_ins = 0;
        s.run_us = 0;
    }
    for r in live.running.iter_mut() {
        r.hog_idx = NO_FINDING;
    }
    live.idle_us = [0; MAX_CORES];
    live.switches = 0;
    live.n_findings = 0;
    live.hog = 0;
    live.poll = 0;
    live.starve = 0;
    live.busy = 0;
    live.busy_max_us = 0;
    live.spin = 0;
    live.window_start_us = now;
}

// ── Kernel hooks ────────────────────────────────────────────────────────────

/// `traceTASK_SWITCHED_IN`: `core` now runs task `tcb`.
#[no_mangle]
pub extern "C" fn picodroid_schedmon_switched_in(
    tcb: c_ulong,
    prio: c_ulong,
    name: *const c_char,
    core: c_ulong,
) {
    let core = core as usize;
    if core >= MAX_CORES {
        return;
    }
    let now = now_us();
    let live = live();
    let i = slot_for(live, tcb as u32, prio as u8, name, now);
    {
        let s = &mut live.slots[i];
        s.waiting = false;
        s.prio = prio as u8;
        s.core = core as u8;
        s.switch_ins = s.switch_ins.wrapping_add(1);
        s.last_seen_us = now;
    }
    live.switches = live.switches.wrapping_add(1);
    if core as u8 + 1 > live.cores_seen {
        live.cores_seen = core as u8 + 1;
    }
    // The tick hook reads this record from another core on the SMP port
    // (under the same lock, but keep the stores ordered anyway): the
    // timestamp first, so a reader can never pair a fresh occupancy with
    // an old clock.
    live.running[core].since_us = now;
    compiler_fence(Ordering::SeqCst);
    live.running[core].prio = prio as u8;
    live.running[core].ticks_seen = 0;
    live.running[core].hog_idx = NO_FINDING;
    live.running[core].slot = i as u8;

    if now.wrapping_sub(live.window_start_us) >= live.window_us {
        close_window(live, now);
    }
}

/// `traceTASK_SWITCHED_OUT`: `core` stops running its task; `still_ready`
/// is non-zero when it was preempted rather than blocked.
#[no_mangle]
pub extern "C" fn picodroid_schedmon_switched_out(core: c_ulong, still_ready: c_ulong) {
    let core = core as usize;
    if core >= MAX_CORES {
        return;
    }
    let now = now_us();
    let live = live();
    let r = live.running[core];
    if r.slot == NO_SLOT {
        return;
    }
    settle_running(live, core, now);
    let s = &mut live.slots[r.slot as usize];
    if still_ready != 0 {
        s.waiting = true;
        s.ready_since_us = now;
    }
    live.running[core].slot = NO_SLOT;
    live.running[core].hog_idx = NO_FINDING;
    live.running[core].ticks_seen = 0;
}

/// `traceMOVED_TASK_TO_READY_STATE`: task `tcb` became Ready.
#[no_mangle]
pub extern "C" fn picodroid_schedmon_ready(tcb: c_ulong, prio: c_ulong, name: *const c_char) {
    let now = now_us();
    let live = live();
    let i = slot_for(live, tcb as u32, prio as u8, name, now);
    let running = live.running.iter().any(|r| r.slot as usize == i);
    let s = &mut live.slots[i];
    s.prio = prio as u8;
    if !running {
        s.waiting = true;
        s.ready_since_us = now;
    }
}

/// `vApplicationTickHook`: once a millisecond, on the tick core. Looks at
/// what each core is running and opens a HOG finding when a real-time-band
/// task has been there [`HOG_TICKS`] ticks running; also closes the window
/// when it is due, so a task that runs a whole window without a single
/// context switch (the interpreter benchmark) still gets reported on time.
#[no_mangle]
pub extern "C" fn vApplicationTickHook() {
    let live = live();
    let now = now_us();
    let elapsed = now.wrapping_sub(live.window_start_us);
    // SAFETY: see `flush`.
    let flush = unsafe { read_volatile(addr_of!(FLUSH_REQUESTED)) };
    if elapsed >= live.window_us || (flush && elapsed > 0) {
        close_window(live, now);
    }
    if flush {
        unsafe { write_volatile(addr_of_mut!(FLUSH_REQUESTED), false) };
    }
    for core in 0..MAX_CORES {
        let r = live.running[core];
        if r.slot == NO_SLOT {
            continue;
        }
        let ticks = r.ticks_seen.saturating_add(1);
        live.running[core].ticks_seen = ticks;
        if r.prio < HOG_PRIO_MIN || ticks < HOG_TICKS {
            continue;
        }
        let s = live.slots[r.slot as usize];
        if s.exempt {
            continue;
        }
        let ran = now.wrapping_sub(r.since_us);
        if r.hog_idx == NO_FINDING && ticks == HOG_TICKS {
            let idx = add_finding(
                live,
                Finding {
                    kind: KIND_HOG,
                    prio: r.prio,
                    core: core as u8,
                    name: s.name,
                    a: ran,
                    b: 0,
                },
            );
            live.running[core].hog_idx = idx;
        } else if r.hog_idx != NO_FINDING {
            let f = &mut live.findings[r.hog_idx as usize];
            f.a = f.a.max(ran);
        }
    }
}

/// C-side BUSYDELAY counter (`cyw43_port.c`).
#[no_mangle]
pub extern "C" fn picodroid_schedmon_busydelay(us: c_ulong) {
    note_busy_delay(us as u32);
}

// ── Reports from outside the kernel ─────────────────────────────────────────

/// A delay type burned `us` microseconds of cycles while the scheduler was
/// running (G3). Any task context.
pub fn note_busy_delay(us: u32) {
    let live = live();
    live.busy = live.busy.wrapping_add(1);
    if us > live.busy_max_us {
        live.busy_max_us = us;
    }
}

/// A `spin_until!` named `name` ran `iters` iterations. Called by the macro
/// on every exit; only a spin past [`SPIN_SOFT_ITERS`] is recorded. Any
/// context, the interrupt handlers the parker and DMA aborts run in included.
pub fn note_spin(name: &'static str, iters: u32) {
    if iters < SPIN_SOFT_ITERS {
        return;
    }
    let live = live();
    live.spin = live.spin.wrapping_add(1);
    live.spin_last_iters = iters;
    live.spin_last_name = name_from_str(name);
}

/// Never report the task called `name` as a HOG or as STARVEd. For a task
/// whose job is to hold a core: the RP family's core-1 flash parker spins,
/// interrupts masked, for the length of every flash operation by design
/// (`platforms/rp/src/hal/rp/core1_park.rs`). Call before the scheduler
/// starts; a slot the task already has is marked too.
pub fn exempt_task(name: &str) {
    let live = live();
    let n = live.n_exempt as usize;
    if n >= MAX_EXEMPT {
        return;
    }
    let key = name_from_str(name);
    live.exempt[n] = key;
    live.n_exempt += 1;
    for s in live.slots.iter_mut() {
        if s.tcb != 0 && s.name == key {
            s.exempt = true;
        }
    }
}

// ── Configuration ───────────────────────────────────────────────────────────

#[cfg(feature = "sim")]
fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("on") | Ok("true") | Ok("yes")
    )
}

/// Read the monitor's configuration and print the `ACTIVE` banner. Called
/// once from boot, on the JVM task; the hooks run with the defaults until
/// then. Simulator: `PICODROID_SCHEDDIAG_WINDOW_MS`, `_STRICT`, `_SELFTEST`
/// from the environment. Device: `_STRICT` and `_SELFTEST` are baked at
/// build time (`option_env!`, the `mem_diag::apply_device_flags` pattern),
/// the window is fixed.
pub fn init() {
    let c = cfg();
    let live = live();
    #[cfg(feature = "sim")]
    {
        let window_ms = std::env::var("PICODROID_SCHEDDIAG_WINDOW_MS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|ms| *ms >= MIN_WINDOW_MS)
            .unwrap_or(DEFAULT_WINDOW_MS);
        live.window_us = window_ms * 1000;
        c.strict = env_flag("PICODROID_SCHEDDIAG_STRICT");
        c.selftest = env_flag("PICODROID_SCHEDDIAG_SELFTEST");
    }
    #[cfg(not(feature = "sim"))]
    {
        c.strict = matches!(option_env!("PICODROID_SCHEDDIAG_STRICT"), Some("1"));
        c.selftest = matches!(option_env!("PICODROID_SCHEDDIAG_SELFTEST"), Some("1"));
    }
    #[cfg(feature = "sim")]
    {
        let _b = crate::host::heap_bypass();
        println!(
            "[schedmon] scheddiag: ACTIVE (window={}ms strict={} selftest={})",
            live.window_us / 1000,
            if c.strict { "on" } else { "off" },
            if c.selftest { "on" } else { "off" },
        );
    }
    #[cfg(not(feature = "sim"))]
    defmt::info!(
        "scheddiag: ACTIVE (window={=u32}ms strict={=bool} selftest={=bool})",
        live.window_us / 1000,
        c.strict,
        c.selftest
    );
}

/// The detector self-test, from the tick source's callback — the timer
/// service task, priority 31, on both targets. Once the first window has
/// closed it holds that task for 5 ms, which the next window must report as
/// a `HOG` (and abort on, in strict mode). Proves the path end to end
/// rather than only that healthy apps stay quiet.
pub fn selftest_on_timer_task() {
    let c = cfg();
    if !c.selftest || c.selftest_done {
        return;
    }
    if live().window_index == 0 {
        return;
    }
    c.selftest_done = true;
    let t0 = now_us();
    // spin-ok: the sched-diag self-test hogs the timer task on purpose
    while now_us().wrapping_sub(t0) < 5_000 {
        core::hint::spin_loop();
    }
}

// ── Printer ─────────────────────────────────────────────────────────────────

/// From the idle hook: print the pending report, if there is one the printer
/// has not taken yet, and enforce strict mode. Two loads and a return when
/// there is nothing to do, so the idle task's `wfi` is not delayed.
pub fn idle_hook() {
    print_pending();
}

/// Whether there is a complete, unprinted report — the two loads
/// [`print_pending`] starts with, for the simulator's printer thread to
/// poll without taking its lock.
pub fn has_pending() -> bool {
    // SAFETY: plain reads of the two sequence words, see `close_window`.
    unsafe {
        let seq = read_volatile(addr_of!(PENDING_SEQ));
        seq & 1 == 0 && seq != read_volatile(addr_of!(CONSUMED_SEQ))
    }
}

/// Set by [`flush`], honoured by the next tick hook: close the window now.
static mut FLUSH_REQUESTED: bool = false;

/// App exit, from the JVM task: report the window in progress, so an app
/// that ran flat out for under a second (the interpreter benchmark) or
/// ended between two windows still leaves its figures behind. The hooks are
/// the table's only writers, so the close is delegated to the tick hook
/// through a flag; this task then sleeps two ticks, during which the tick
/// hook closes the window — whatever its length: `helloworld`'s whole run
/// is one — and the printer may take it, and prints itself whatever is
/// still pending. Strict mode applies to the partial window as to any other.
pub fn flush() {
    printer();
    // SAFETY: a plain store the tick hook reads; the only writer besides
    // the hook's clear is this exit path.
    unsafe { write_volatile(addr_of_mut!(FLUSH_REQUESTED), true) };
    crate::rtos::delay_ms(2);
    printer();
}

/// The exit flush's printer. On a device that is [`print_pending`] itself:
/// the idle hook and the JVM task cannot run at once on core 0, and the
/// other core's idle task does not print. The simulator prints from a host
/// thread instead (`hal/sim/rtos_freertos.rs` says why the idle thread
/// cannot), so the flush goes through that thread's lock.
#[cfg(feature = "sim")]
fn printer() {
    crate::hal::sim::rtos::sched_diag_print();
}

#[cfg(not(feature = "sim"))]
fn printer() {
    print_pending();
}

/// Take the pending report if it is complete and new, print it, and abort
/// in strict mode on a finding. The caller is the printer and must be the
/// only one at a time: the device's idle hook or its exit flush, the
/// simulator's printer thread or its flush under that thread's lock.
pub fn print_pending() {
    // SAFETY: reads guarded by PENDING_SEQ, see `close_window`; PRINTING is
    // touched only here.
    let report = unsafe {
        let seq = read_volatile(addr_of!(PENDING_SEQ));
        if seq & 1 == 1 || seq == read_volatile(addr_of!(CONSUMED_SEQ)) {
            return;
        }
        fence(Ordering::SeqCst);
        core::ptr::copy_nonoverlapping(addr_of!(PENDING), addr_of_mut!(PRINTING), 1);
        fence(Ordering::SeqCst);
        if read_volatile(addr_of!(PENDING_SEQ)) != seq {
            return;
        }
        write_volatile(addr_of_mut!(CONSUMED_SEQ), seq);
        &*addr_of!(PRINTING)
    };
    print_report(report);
    if cfg().strict && report.any_finding() {
        strict_abort(report);
    }
}

fn pct(part: u32, whole: u32) -> u32 {
    if whole == 0 {
        0
    } else {
        ((part as u64 * 100) / whole as u64) as u32
    }
}

#[cfg(feature = "sim")]
fn print_report(r: &Report) {
    use std::io::Write;
    let _b = crate::host::heap_bypass();
    // One lock for the whole report, and never a panic: a stdout that is
    // gone at exit would otherwise take the printer thread down with a
    // message nobody reads.
    let out = std::io::stdout();
    let mut o = out.lock();
    let ms = r.elapsed_us / 1000;
    let idle0 = pct(r.idle_us[0], r.elapsed_us);
    if r.cores >= 2 {
        let _ = writeln!(o,
            "[schedmon] w={} ms={} idle0={}% idle1={}% sw={} hog={} poll={} starve={} busy={} spin={} lost={}",
            r.window,
            ms,
            idle0,
            pct(r.idle_us[1], r.elapsed_us),
            r.switches,
            r.hog,
            r.poll,
            r.starve,
            r.busy,
            r.spin,
            r.lost
        );
    } else {
        let _ = writeln!(o,
            "[schedmon] w={} ms={} idle0={}% sw={} hog={} poll={} starve={} busy={} spin={} lost={}",
            r.window, ms, idle0, r.switches, r.hog, r.poll, r.starve, r.busy, r.spin, r.lost
        );
    }
    // Top tasks by run time — what `pdb sysmon` answers on a device.
    let mut rows: std::vec::Vec<&Row> = r.rows[..r.n_rows as usize].iter().collect();
    rows.sort_unstable_by_key(|row| core::cmp::Reverse(row.run_us));
    let _ = write!(o, "[schedmon] tasks:");
    for row in rows.iter().take(6) {
        let _ = write!(
            o,
            " {}={}%/{}",
            name_str(&row.name),
            pct(row.run_us, r.elapsed_us),
            row.switch_ins
        );
    }
    let _ = writeln!(o);
    for f in &r.findings[..r.n_findings as usize] {
        let _ = match f.kind {
            KIND_HOG => writeln!(
                o,
                "[schedmon] HOG {} prio={} core={} ran={} ms",
                name_str(&f.name),
                f.prio,
                f.core,
                f.a / 1000
            ),
            KIND_POLL => writeln!(
                o,
                "[schedmon] POLL {} prio={} core={} switch_ins={} ran={} ms",
                name_str(&f.name),
                f.prio,
                f.core,
                f.a,
                f.b / 1000
            ),
            _ => writeln!(
                o,
                "[schedmon] STARVE {} prio={} ready={} ms",
                name_str(&f.name),
                f.prio,
                f.a / 1000
            ),
        };
    }
    if r.busy > 0 {
        let _ = writeln!(
            o,
            "[schedmon] BUSYDELAY n={} max={} us",
            r.busy, r.busy_max_us
        );
    }
    if r.spin > 0 {
        let _ = writeln!(
            o,
            "[schedmon] SPIN {} iters={} (n={})",
            name_str(&r.spin_last_name),
            r.spin_last_iters,
            r.spin
        );
    }
}

#[cfg(not(feature = "sim"))]
fn print_report(r: &Report) {
    defmt::info!(
        "schedmon: w={=u32} ms={=u32} idle0={=u32}% idle1={=u32}% sw={=u32} hog={=u32} poll={=u32} starve={=u32} busy={=u32} spin={=u32} lost={=u32}",
        r.window,
        r.elapsed_us / 1000,
        pct(r.idle_us[0], r.elapsed_us),
        pct(r.idle_us[1], r.elapsed_us),
        r.switches,
        r.hog,
        r.poll,
        r.starve,
        r.busy,
        r.spin,
        r.lost
    );
    for f in &r.findings[..r.n_findings as usize] {
        match f.kind {
            KIND_HOG => defmt::warn!(
                "schedmon: HOG {=str} prio={=u8} core={=u8} ran={=u32} ms",
                name_str(&f.name),
                f.prio,
                f.core,
                f.a / 1000
            ),
            KIND_POLL => defmt::warn!(
                "schedmon: POLL {=str} prio={=u8} core={=u8} switch_ins={=u32} ran={=u32} ms",
                name_str(&f.name),
                f.prio,
                f.core,
                f.a,
                f.b / 1000
            ),
            _ => defmt::warn!(
                "schedmon: STARVE {=str} prio={=u8} ready={=u32} ms",
                name_str(&f.name),
                f.prio,
                f.a / 1000
            ),
        }
    }
    if r.busy > 0 {
        defmt::warn!(
            "schedmon: BUSYDELAY n={=u32} max={=u32} us",
            r.busy,
            r.busy_max_us
        );
    }
    if r.spin > 0 {
        defmt::warn!(
            "schedmon: SPIN {=str} iters={=u32} (n={=u32})",
            name_str(&r.spin_last_name),
            r.spin_last_iters,
            r.spin
        );
    }
}

fn strict_abort(r: &Report) -> ! {
    let n = r.hog + r.poll + r.starve + r.busy + r.spin;
    #[cfg(feature = "sim")]
    {
        {
            let _b = crate::host::heap_bypass();
            println!(
                "[schedmon] STRICT: aborting on {n} finding(s) in window {}",
                r.window
            );
        }
        std::process::abort()
    }
    #[cfg(not(feature = "sim"))]
    {
        defmt::panic!(
            "schedmon: STRICT: {=u32} finding(s) in window {=u32}",
            n,
            r.window
        )
    }
}
