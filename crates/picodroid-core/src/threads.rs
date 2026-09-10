// SPDX-License-Identifier: GPL-3.0-only
//! Registry of Java threads: the task ↔ `Thread` object mapping behind
//! `currentThread`, `join`, `interrupt`, `isAlive`, `sleep` and
//! `Object.wait`/`notify`.
//!
//! One entry per task that has ever touched the Java thread API: every
//! `Thread.start` child (reserved before its task exists, so the `Thread`
//! object is a GC root from the first instruction), plus any task that
//! *adopts* itself — the UI task and the executor workers do that lazily,
//! the first time they call `currentThread()` or park. An entry is removed
//! when its thread terminates; `isAlive` is "has an entry and it is alive".
//!
//! # Parking
//!
//! Every blocking thread primitive is one loop over the kernel's task
//! notification (`rtos::task_wait_notification`), which any other task can
//! end with `rtos::task_notify`: `sleep` waits out its deadline, `join`
//! waits for the target to terminate, `wait` waits to be notified. Whoever
//! wakes a parked task — `interrupt`, `notify`, a terminating join target,
//! the app-stop path — sets the reason in the entry first and notifies
//! second; the parked task re-checks *why* it woke and loops on anything
//! it did not ask for (a stray notification from the debug bridge, say).
//! That is why `Object.wait` may wake spuriously, which the JLS permits.
//!
//! # Concurrency
//!
//! Every read or write of the table runs inside an `AtomicSection`
//! (scheduler suspended); the blocking wait runs outside one. Plain
//! loads/stores only — thumbv6m has no atomic RMW.
//!
//! Android's `Thread.setPriority` is advisory here — see `task_priority`.

use pico_jvm::atomic_section::AtomicSection;
use pico_jvm::types::MonitorKey;

use crate::rtos::{self, RawTask, Timeout};

/// Bound by stack, not by this table: each child costs 16 KB of arena.
pub const MAX_JAVA_THREADS: usize = 16;

/// Why a parked task is parked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Park {
    None,
    Sleep,
    Join,
    /// `Object.wait` on the monitor `key`; `seq` orders waiters so `notify`
    /// wakes the longest-waiting one first.
    Wait {
        key: MonitorKey,
        seq: u32,
    },
}

/// How a park ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// What the task waited for happened (join target gone, notified, …).
    Satisfied,
    TimedOut,
    /// `Thread.interrupt` — the flag has been cleared; throw
    /// `InterruptedException`.
    Interrupted,
    /// The app is being stopped; return quietly and let the interpreter's
    /// stop check unwind the thread.
    Stopped,
}

#[derive(Clone, Copy)]
struct Entry {
    /// `0` between `reserve` and `bind_current` (the task does not exist
    /// yet) — also a task that adopted itself before the scheduler ran.
    task: RawTask,
    /// The Java `Thread` object, a GC root while the entry exists. `None`
    /// for a task that parked before ever asking for one.
    obj: Option<u16>,
    alive: bool,
    interrupted: bool,
    /// Set by `notify` for a `Wait` parker; cleared when the park ends.
    notified: bool,
    /// Set by `wake_all_parked` (app stop).
    stopping: bool,
    park: Park,
    /// Tasks parked in `join` on this thread, notified on termination.
    /// Joiners past the array poll instead (their park has a timeout).
    joiners: [RawTask; MAX_JOINERS],
}

const MAX_JOINERS: usize = 4;

/// A joiner that found the array full re-checks on this cadence.
const JOIN_POLL_MS: u32 = 20;

struct Table {
    entries: [Option<Entry>; MAX_JAVA_THREADS],
    wait_seq: u32,
}

struct TableCell(core::cell::UnsafeCell<Table>);

// SAFETY: every access goes through `table()`, whose callers hold an
// `AtomicSection` — see the module docs.
unsafe impl Sync for TableCell {}

static TABLE: TableCell = TableCell(core::cell::UnsafeCell::new(Table {
    entries: [None; MAX_JAVA_THREADS],
    wait_seq: 0,
}));

/// # Safety
/// The caller must hold an `AtomicSection` for as long as the reference
/// lives.
unsafe fn table() -> &'static mut Table {
    &mut *TABLE.0.get()
}

fn blank(task: RawTask, obj: Option<u16>) -> Entry {
    Entry {
        task,
        obj,
        alive: true,
        interrupted: false,
        notified: false,
        stopping: false,
        park: Park::None,
        joiners: [0; MAX_JOINERS],
    }
}

fn slot_by_task(t: &Table, task: RawTask) -> Option<usize> {
    if task == 0 {
        return None;
    }
    t.entries
        .iter()
        .position(|e| e.is_some_and(|e| e.task == task))
}

fn slot_by_obj(t: &Table, obj: u16) -> Option<usize> {
    t.entries
        .iter()
        .position(|e| e.is_some_and(|e| e.obj == Some(obj)))
}

#[cfg(not(test))]
fn now_ns() -> i64 {
    crate::hal::system_clock::elapsed_realtime_nanos()
}

/// The test platform's clock is a constant 0, which would make every timed
/// park here spin forever; the parking tests need time to pass.
#[cfg(test)]
fn now_ns() -> i64 {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_nanos() as i64
}

// ── Lifecycle ───────────────────────────────────────────────────────────────

/// Reserve an entry for a `Thread` about to be started, before its task
/// exists, so `thread_obj` is rooted from here on. `None` when the table is
/// full.
pub fn reserve(thread_obj: u16) -> Option<usize> {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    let slot = t.entries.iter().position(Option::is_none)?;
    t.entries[slot] = Some(blank(0, Some(thread_obj)));
    Some(slot)
}

/// The child's first act: attach its task handle to its reserved entry.
pub fn bind_current(slot: usize) {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    if let Some(e) = unsafe { table() }.entries[slot].as_mut() {
        e.task = rtos::task_current();
    }
}

/// Make the calling task a Java thread (if it is not one yet) and, when
/// given, attach the `Thread` object that now represents it. Returns false
/// when the table is full.
pub fn adopt_current(obj: Option<u16>) -> bool {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    let me = rtos::task_current();
    let slot = match slot_by_task(t, me) {
        Some(s) => s,
        None => {
            let Some(s) = t.entries.iter().position(Option::is_none) else {
                return false;
            };
            t.entries[s] = Some(blank(me, None));
            s
        }
    };
    if obj.is_some() {
        if let Some(e) = t.entries[slot].as_mut() {
            e.obj = obj;
        }
    }
    true
}

/// The `Thread` object of the calling task, if it has one.
pub fn current_obj() -> Option<u16> {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    slot_by_task(t, rtos::task_current()).and_then(|s| t.entries[s].and_then(|e| e.obj))
}

/// Whether the `Thread` object `obj` is a started, unfinished thread.
pub fn is_alive(obj: u16) -> bool {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    slot_by_obj(t, obj).is_some_and(|s| t.entries[s].is_some_and(|e| e.alive))
}

/// The thread in `slot` has finished: release whatever it still holds,
/// wake its joiners, and drop the entry. Idempotent, and callable by the
/// thread itself (the normal path) or by the spawner when the task never
/// ran.
pub fn terminate(slot: usize) {
    // Monitors are the calling task's own to give back (FreeRTOS refuses a
    // give from another task), so this is meaningful only on the normal
    // path; on the spawn-failure path the thread never ran and holds none.
    crate::monitor_store::release_all_held_by_current();
    let joiners = {
        let _atomic = AtomicSection::enter();
        // SAFETY: inside the section.
        let t = unsafe { table() };
        let Some(e) = t.entries[slot] else {
            return;
        };
        t.entries[slot] = None;
        e.joiners
    };
    for j in joiners {
        rtos::task_notify(j);
    }
}

/// [`terminate`] by `Thread` object — the `exit0` native.
pub fn terminate_by_obj(obj: u16) {
    let slot = {
        let _atomic = AtomicSection::enter();
        // SAFETY: inside the section.
        slot_by_obj(unsafe { table() }, obj)
    };
    if let Some(s) = slot {
        terminate(s);
    }
}

// ── Interrupts ──────────────────────────────────────────────────────────────

/// `Thread.interrupt`: set the flag and wake the target if it is parked.
pub fn interrupt(obj: u16) {
    let task = {
        let _atomic = AtomicSection::enter();
        // SAFETY: inside the section.
        let t = unsafe { table() };
        let Some(s) = slot_by_obj(t, obj) else {
            return;
        };
        let Some(e) = t.entries[s].as_mut() else {
            return;
        };
        e.interrupted = true;
        if e.park == Park::None {
            0
        } else {
            e.task
        }
    };
    rtos::task_notify(task);
}

/// `Thread.isInterrupted`.
pub fn is_interrupted(obj: u16) -> bool {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    slot_by_obj(t, obj).is_some_and(|s| t.entries[s].is_some_and(|e| e.interrupted))
}

/// Static `Thread.interrupted`: the calling task's flag, cleared.
pub fn take_interrupted_current() -> bool {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    let Some(s) = slot_by_task(t, rtos::task_current()) else {
        return false;
    };
    let Some(e) = t.entries[s].as_mut() else {
        return false;
    };
    core::mem::replace(&mut e.interrupted, false)
}

// ── Parking ─────────────────────────────────────────────────────────────────

/// Ensure the calling task has an entry and mark it parked as `park`.
fn begin_park(park: Park) -> Option<usize> {
    if !adopt_current(None) {
        return None;
    }
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    let s = slot_by_task(t, rtos::task_current())?;
    let e = t.entries[s].as_mut()?;
    e.park = park;
    e.notified = false;
    Some(s)
}

/// Read-and-clear the wake reasons for `slot`; `satisfied` is evaluated
/// inside the same section so it sees a consistent table.
fn poll_park(slot: usize, satisfied: &dyn Fn(&Table) -> bool) -> Option<Outcome> {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    let e = t.entries[slot]?;
    if e.stopping {
        return Some(Outcome::Stopped);
    }
    if e.interrupted {
        // `Thread.interrupt` clears the flag when it is delivered as an
        // exception, as on Android.
        t.entries[slot].as_mut()?.interrupted = false;
        return Some(Outcome::Interrupted);
    }
    if satisfied(t) {
        return Some(Outcome::Satisfied);
    }
    None
}

fn end_park(slot: usize) {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    if let Some(e) = unsafe { table() }.entries[slot].as_mut() {
        e.park = Park::None;
        e.notified = false;
    }
}

/// The parking loop shared by sleep, join and wait. `poll_ms` bounds each
/// individual wait so a joiner the array could not hold still notices its
/// target going away.
fn park_loop(
    park: Park,
    timeout_ms: Option<u32>,
    poll_ms: Option<u32>,
    satisfied: &dyn Fn(&Table) -> bool,
) -> Outcome {
    let Some(slot) = begin_park(park) else {
        // No entry (table full): the only honest answer is to not block.
        return Outcome::Satisfied;
    };
    let deadline = timeout_ms.map(|ms| now_ns().saturating_add(i64::from(ms) * 1_000_000));
    let outcome = loop {
        if let Some(o) = poll_park(slot, satisfied) {
            break o;
        }
        if crate::host::stop_requested() {
            break Outcome::Stopped;
        }
        let mut wait = Timeout::Forever;
        if let Some(d) = deadline {
            let left = d - now_ns();
            if left <= 0 {
                break Outcome::TimedOut;
            }
            wait = Timeout::Ms(((left + 999_999) / 1_000_000).min(u32::MAX as i64) as u32);
        }
        if let Some(p) = poll_ms {
            wait = match wait {
                Timeout::Ms(ms) => Timeout::Ms(ms.min(p)),
                _ => Timeout::Ms(p),
            };
        }
        rtos::task_wait_notification(wait);
    };
    end_park(slot);
    outcome
}

/// `Thread.sleep`.
pub fn sleep_current(ms: u32) -> Outcome {
    match park_loop(Park::Sleep, Some(ms), None, &|_| false) {
        Outcome::TimedOut => Outcome::Satisfied,
        o => o,
    }
}

/// `Thread.join`: block until the thread `target` (a `Thread` object) has
/// terminated, or `timeout_ms` (`None` = forever) elapses. Returns at once
/// for a thread that was never started or has already finished.
pub fn join(target: u16, timeout_ms: Option<u32>) -> Outcome {
    let me = rtos::task_current();
    let registered = {
        let _atomic = AtomicSection::enter();
        // SAFETY: inside the section.
        let t = unsafe { table() };
        match slot_by_obj(t, target) {
            None => return Outcome::Satisfied,
            Some(s) => {
                let Some(e) = t.entries[s].as_mut() else {
                    return Outcome::Satisfied;
                };
                if !e.alive {
                    return Outcome::Satisfied;
                }
                match e.joiners.iter().position(|&j| j == 0) {
                    Some(i) => {
                        e.joiners[i] = me;
                        true
                    }
                    None => false,
                }
            }
        }
    };
    let gone = |t: &Table| slot_by_obj(t, target).is_none();
    let outcome = park_loop(
        Park::Join,
        timeout_ms,
        if registered { None } else { Some(JOIN_POLL_MS) },
        &gone,
    );
    if registered {
        let _atomic = AtomicSection::enter();
        // SAFETY: inside the section.
        let t = unsafe { table() };
        if let Some(s) = slot_by_obj(t, target) {
            if let Some(e) = t.entries[s].as_mut() {
                for j in e.joiners.iter_mut() {
                    if *j == me {
                        *j = 0;
                    }
                }
            }
        }
    }
    outcome
}

/// `Object.wait` (the monitor already released by the caller): block until
/// [`notify`] picks this task, `timeout_ms` elapses, or an interrupt.
pub fn wait_current(key: MonitorKey, timeout_ms: Option<u32>) -> Outcome {
    let seq = {
        let _atomic = AtomicSection::enter();
        // SAFETY: inside the section.
        let t = unsafe { table() };
        t.wait_seq = t.wait_seq.wrapping_add(1);
        t.wait_seq
    };
    let me = rtos::task_current();
    let notified = move |t: &Table| {
        slot_by_task(t, me).is_some_and(|s| t.entries[s].is_some_and(|e| e.notified))
    };
    park_loop(Park::Wait { key, seq }, timeout_ms, None, &notified)
}

/// `Object.notify` (`all == false`: the longest-waiting task on `key`) and
/// `Object.notifyAll`.
pub fn notify(key: MonitorKey, all: bool) {
    let mut to_wake: [RawTask; MAX_JAVA_THREADS] = [0; MAX_JAVA_THREADS];
    {
        let _atomic = AtomicSection::enter();
        // SAFETY: inside the section.
        let t = unsafe { table() };
        let mut n = 0;
        if all {
            for e in t.entries.iter_mut().flatten() {
                if matches!(e.park, Park::Wait { key: k, .. } if k == key) && !e.notified {
                    e.notified = true;
                    to_wake[n] = e.task;
                    n += 1;
                }
            }
        } else {
            let mut best: Option<(u32, usize)> = None;
            for (i, e) in t.entries.iter().enumerate() {
                if let Some(e) = e {
                    if let Park::Wait { key: k, seq } = e.park {
                        if k == key && !e.notified && best.is_none_or(|(bs, _)| seq < bs) {
                            best = Some((seq, i));
                        }
                    }
                }
            }
            if let Some((_, i)) = best {
                if let Some(e) = t.entries[i].as_mut() {
                    e.notified = true;
                    to_wake[0] = e.task;
                }
            }
        }
    }
    for task in to_wake {
        if task != 0 {
            rtos::task_notify(task);
        }
    }
}

/// App stop: end every park so the interpreter's stop check can unwind
/// each thread.
pub fn wake_all_parked() {
    let mut to_wake: [RawTask; MAX_JAVA_THREADS] = [0; MAX_JAVA_THREADS];
    {
        let _atomic = AtomicSection::enter();
        // SAFETY: inside the section.
        let t = unsafe { table() };
        for (i, e) in t.entries.iter_mut().enumerate() {
            if let Some(e) = e {
                if e.park != Park::None {
                    e.stopping = true;
                    to_wake[i] = e.task;
                }
            }
        }
    }
    for task in to_wake {
        if task != 0 {
            rtos::task_notify(task);
        }
    }
}

/// Forget every thread — heap reset between app runs, after every JVM task
/// has drained.
pub fn clear() {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    t.entries = [None; MAX_JAVA_THREADS];
    t.wait_seq = 0;
}

/// GC root provider: every live `Thread` object.
pub fn visit_thread_roots(visit: &mut dyn FnMut(u16)) {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    for e in unsafe { table() }.entries.iter().flatten() {
        if let Some(o) = e.obj {
            visit(o);
        }
    }
}

#[cfg(test)]
pub fn park_of(obj: u16) -> Park {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let t = unsafe { table() };
    slot_by_obj(t, obj)
        .and_then(|s| t.entries[s])
        .map_or(Park::None, |e| e.park)
}

// Under `cargo test` a task is a host thread and the notification is a
// condvar (`hal/sim/rtos.rs`), so the parking protocol runs for real.
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Mutex, MutexGuard};
    use std::time::Duration;

    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        let g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        g
    }

    fn roots() -> Vec<u16> {
        let mut v = Vec::new();
        visit_thread_roots(&mut |o| v.push(o));
        v
    }

    fn wait_until(mut cond: impl FnMut() -> bool) {
        for _ in 0..500 {
            if cond() {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("condition never held");
    }

    #[test]
    fn a_reserved_thread_is_rooted_and_alive_until_it_terminates() {
        let _g = serial();
        let slot = reserve(7).unwrap();
        assert_eq!(roots(), vec![7]);
        assert!(is_alive(7));
        assert_eq!(join(7, Some(10)), Outcome::TimedOut, "not finished yet");
        terminate(slot);
        assert!(!is_alive(7));
        assert!(roots().is_empty());
        terminate(slot); // idempotent
        assert_eq!(join(7, None), Outcome::Satisfied, "already gone");
    }

    #[test]
    fn join_returns_when_the_target_terminates() {
        let _g = serial();
        let slot = reserve(5).unwrap();
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let child = std::thread::spawn(move || {
            bind_current(slot);
            go_rx.recv().unwrap();
            terminate(slot);
        });
        let joiner = std::thread::spawn(|| join(5, None));
        wait_until(|| park_of(5) == Park::None && is_alive(5));
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            !joiner.is_finished(),
            "join must block while the target lives"
        );
        go_tx.send(()).unwrap();
        child.join().unwrap();
        assert_eq!(joiner.join().unwrap(), Outcome::Satisfied);
    }

    #[test]
    fn interrupt_wakes_a_sleeper_and_clears_the_flag() {
        let _g = serial();
        let sleeper = std::thread::spawn(|| {
            assert!(adopt_current(Some(9)));
            sleep_current(5_000)
        });
        wait_until(|| park_of(9) == Park::Sleep);
        interrupt(9);
        assert_eq!(sleeper.join().unwrap(), Outcome::Interrupted);
        assert!(!is_interrupted(9), "delivered interrupts clear the flag");
    }

    #[test]
    fn a_pending_interrupt_is_seen_by_the_next_park() {
        let _g = serial();
        let t = std::thread::spawn(|| {
            assert!(adopt_current(Some(10)));
            interrupt(10);
            assert!(is_interrupted(10));
            let o = sleep_current(1_000);
            (o, take_interrupted_current())
        });
        assert_eq!(t.join().unwrap(), (Outcome::Interrupted, false));
    }

    #[test]
    fn sleep_times_out_and_reports_satisfied() {
        let _g = serial();
        let t = std::thread::spawn(|| {
            assert!(adopt_current(Some(11)));
            sleep_current(10)
        });
        assert_eq!(t.join().unwrap(), Outcome::Satisfied);
    }

    #[test]
    fn notify_wakes_the_longest_waiter_first_and_notify_all_wakes_the_rest() {
        let _g = serial();
        let key = MonitorKey::Object(500);
        let spawn_waiter = |obj: u16| {
            std::thread::spawn(move || {
                assert!(adopt_current(Some(obj)));
                wait_current(key, None)
            })
        };
        let first = spawn_waiter(21);
        wait_until(|| matches!(park_of(21), Park::Wait { .. }));
        let second = spawn_waiter(22);
        wait_until(|| matches!(park_of(22), Park::Wait { .. }));
        let third = spawn_waiter(23);
        wait_until(|| matches!(park_of(23), Park::Wait { .. }));

        notify(key, false);
        assert_eq!(first.join().unwrap(), Outcome::Satisfied);
        std::thread::sleep(Duration::from_millis(20));
        assert!(!second.is_finished() && !third.is_finished());

        notify(key, true);
        assert_eq!(second.join().unwrap(), Outcome::Satisfied);
        assert_eq!(third.join().unwrap(), Outcome::Satisfied);
    }

    #[test]
    fn a_timed_wait_expires_without_a_notify() {
        let _g = serial();
        let key = MonitorKey::Array(600);
        let t = std::thread::spawn(move || {
            assert!(adopt_current(Some(31)));
            wait_current(key, Some(15))
        });
        assert_eq!(t.join().unwrap(), Outcome::TimedOut);
    }

    #[test]
    fn notify_on_another_key_does_not_wake_a_waiter() {
        let _g = serial();
        let key = MonitorKey::Object(700);
        let t = std::thread::spawn(move || {
            assert!(adopt_current(Some(41)));
            wait_current(key, Some(60))
        });
        wait_until(|| matches!(park_of(41), Park::Wait { .. }));
        notify(MonitorKey::Object(701), true);
        assert_eq!(t.join().unwrap(), Outcome::TimedOut);
    }

    #[test]
    fn app_stop_ends_every_park() {
        let _g = serial();
        let key = MonitorKey::Object(800);
        let a = std::thread::spawn(move || {
            assert!(adopt_current(Some(51)));
            wait_current(key, None)
        });
        let b = std::thread::spawn(|| {
            assert!(adopt_current(Some(52)));
            sleep_current(60_000)
        });
        wait_until(|| matches!(park_of(51), Park::Wait { .. }) && park_of(52) == Park::Sleep);
        wake_all_parked();
        assert_eq!(a.join().unwrap(), Outcome::Stopped);
        assert_eq!(b.join().unwrap(), Outcome::Stopped);
    }

    #[test]
    fn the_table_refuses_a_seventeenth_thread() {
        let _g = serial();
        let slots: Vec<usize> = (0..MAX_JAVA_THREADS as u16)
            .map(|i| reserve(1000 + i).unwrap())
            .collect();
        assert!(reserve(2000).is_none());
        for s in slots {
            terminate(s);
        }
        assert!(reserve(2000).is_some());
    }
}
