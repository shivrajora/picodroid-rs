// SPDX-License-Identifier: GPL-3.0-only
//! Deadline table behind `picodroid.concurrent.MainScheduledExecutor`.
//!
//! A `ScheduledExecutorService` on the JDK owns a worker thread that sleeps
//! until the next deadline. Here a thread is a 16 KiB stack, and every
//! delayed task wants the main thread anyway (it exists to touch a widget
//! later), so the "worker" is this table: the lifecycle loop calls
//! [`fire_due`] on every `LvglTick`, and each due entry is posted to the
//! main queue like any `Executors.mainExecutor().execute(...)`. Resolution
//! is therefore the 16 ms frame, and nothing fires while the tick source
//! is paused for display sleep.
//!
//! Three kinds, as the Java side encodes them: a one-shot frees its slot
//! when posted; a fixed-rate entry advances its deadline by the period from
//! the previous *deadline* (one late run does not shift the schedule, and a
//! deadline already in the past is replaced by `now + period` rather than
//! producing a burst of catch-up runs); a fixed-delay entry is posted once
//! and waits for the Java task to report [`completed`], which sets the next
//! deadline from then.
//!
//! Slot ids carry a generation so a stale id from the Java side — a
//! `cancel()` after the slot was freed and reused — cannot touch the new
//! occupant. Every mutation is bracketed by an
//! [`pico_jvm::atomic_section::AtomicSection`] because `schedule` and
//! `cancel` run on whichever JVM task calls them while `fire_due` runs on
//! the UI task, the same discipline as the main queue's root shadow.

use core::cell::Cell;

use pico_jvm::atomic_section::AtomicSection;

use super::main_queue;

/// Slots shared by every executor instance. A slot is a scheduled task, not
/// an executor: sixteen concurrently armed delays is a lot for one screen.
pub const CAPACITY: usize = 16;

/// What a due entry does after posting. Numbered as
/// `MainScheduledExecutor.ONE_SHOT` / `FIXED_RATE` / `FIXED_DELAY`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    OneShot,
    FixedRate,
    FixedDelay,
}

impl Kind {
    pub fn from_java(kind: i32) -> Option<Self> {
        match kind {
            0 => Some(Kind::OneShot),
            1 => Some(Kind::FixedRate),
            2 => Some(Kind::FixedDelay),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Entry {
    runnable: u16,
    /// Wrapping ms on the elapsed-realtime clock, read as a signed
    /// difference like `tick_source`'s: exact for any delay under 24 days.
    due_ms: u32,
    period_ms: u32,
    kind: Kind,
    /// Fixed delay only: posted, and the Java task has not yet reported back.
    in_flight: bool,
    live: bool,
    /// Bumped whenever the slot is freed.
    generation: u8,
}

const EMPTY: Entry = Entry {
    runnable: 0,
    due_ms: 0,
    period_ms: 0,
    kind: Kind::OneShot,
    in_flight: false,
    live: false,
    generation: 0,
};

struct TableCell(Cell<[Entry; CAPACITY]>);
// SAFETY: every access is under an `AtomicSection` (see the module docs).
unsafe impl Sync for TableCell {}

static TABLE: TableCell = TableCell(Cell::new([EMPTY; CAPACITY]));

#[cfg(not(test))]
fn now_ms() -> u32 {
    (crate::hal::system_clock::elapsed_realtime_nanos() / 1_000_000) as u32
}

/// The test HAL's clock stands at zero, so the tests drive this one.
#[cfg(test)]
static TEST_NOW_MS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

#[cfg(test)]
fn now_ms() -> u32 {
    TEST_NOW_MS.load(core::sync::atomic::Ordering::Relaxed)
}

/// `due` is now or in the past.
fn is_due(now: u32, due: u32) -> bool {
    (now.wrapping_sub(due) as i32) >= 0
}

fn encode_id(slot: usize, generation: u8) -> i32 {
    slot as i32 | (generation as i32) << 8
}

/// The slot an id names, if that generation is still the live one.
fn resolve(table: &[Entry; CAPACITY], id: i32) -> Option<usize> {
    if id < 0 {
        return None;
    }
    let slot = (id & 0xFF) as usize;
    let generation = (id >> 8) as u8;
    let e = table.get(slot)?;
    (e.live && e.generation == generation).then_some(slot)
}

/// Forget every entry. Called with `main_queue::init` at each app run, so a
/// reloaded app never inherits deadlines pointing into the old heap.
pub fn init() {
    let _atomic = AtomicSection::enter();
    let mut table = TABLE.0.get();
    for e in table.iter_mut() {
        let generation = e.generation;
        *e = EMPTY;
        e.generation = generation;
    }
    TABLE.0.set(table);
}

/// Arm a slot for `runnable`, due `delay_ms` from now. `period_ms` is
/// ignored for a one-shot. Returns the slot id, or `None` when the table is
/// full.
pub fn schedule(runnable: u16, delay_ms: u32, period_ms: u32, kind: Kind) -> Option<i32> {
    let _atomic = AtomicSection::enter();
    let mut table = TABLE.0.get();
    let slot = table.iter().position(|e| !e.live)?;
    let e = &mut table[slot];
    e.runnable = runnable;
    e.due_ms = now_ms().wrapping_add(delay_ms);
    e.period_ms = if kind == Kind::OneShot {
        0
    } else {
        period_ms.max(1)
    };
    e.kind = kind;
    e.in_flight = false;
    e.live = true;
    let id = encode_id(slot, e.generation);
    TABLE.0.set(table);
    Some(id)
}

/// Free the slot `id` names. `false` when it names nothing live any more:
/// a one-shot already posted, or a stale id.
pub fn cancel(id: i32) -> bool {
    let _atomic = AtomicSection::enter();
    let mut table = TABLE.0.get();
    let Some(slot) = resolve(&table, id) else {
        return false;
    };
    free(&mut table[slot]);
    TABLE.0.set(table);
    true
}

fn free(e: &mut Entry) {
    let generation = e.generation.wrapping_add(1);
    *e = EMPTY;
    e.generation = generation;
}

/// A fixed-delay task's run is over: the next is one period from now. A
/// no-op for any other kind or a stale id.
pub fn completed(id: i32) {
    let _atomic = AtomicSection::enter();
    let mut table = TABLE.0.get();
    let Some(slot) = resolve(&table, id) else {
        return;
    };
    let e = &mut table[slot];
    if e.kind == Kind::FixedDelay {
        e.in_flight = false;
        e.due_ms = now_ms().wrapping_add(e.period_ms);
    }
    TABLE.0.set(table);
}

/// Milliseconds until the slot is due — negative once overdue — or `None`
/// for a stale id.
pub fn delay_ms(id: i32) -> Option<i32> {
    let _atomic = AtomicSection::enter();
    let table = TABLE.0.get();
    let slot = resolve(&table, id)?;
    Some(table[slot].due_ms.wrapping_sub(now_ms()) as i32)
}

/// Post every due entry to the main queue. UI task only, once per tick.
///
/// An entry the queue has no room for stays as it is and is tried again
/// next tick; a fixed-rate entry does not advance until it is posted.
pub fn fire_due() {
    let now = now_ms();
    let _atomic = AtomicSection::enter();
    let mut table = TABLE.0.get();
    let mut changed = false;
    for e in table.iter_mut() {
        if !e.live || e.in_flight || !is_due(now, e.due_ms) {
            continue;
        }
        // Nested in our section: `enqueue_runnable` enters one of its own
        // for the root shadow, and the platform's hooks nest.
        if !main_queue::enqueue_runnable(e.runnable) {
            continue;
        }
        changed = true;
        match e.kind {
            Kind::OneShot => free(e),
            Kind::FixedRate => {
                let next = e.due_ms.wrapping_add(e.period_ms);
                e.due_ms = if is_due(now, next) {
                    now.wrapping_add(e.period_ms)
                } else {
                    next
                };
            }
            Kind::FixedDelay => e.in_flight = true,
        }
    }
    if changed {
        TABLE.0.set(table);
    }
}

/// GC roots: every armed Runnable. Between `schedule` and the post nothing
/// on the Java side need hold it — `schedule(() -> ..., 5, SECONDS)` with
/// the future dropped is idiomatic.
pub fn visit_pending_runnable_roots(visit: &mut dyn FnMut(u16)) {
    let table = TABLE.0.get();
    for e in table.iter().filter(|e| e.live) {
        visit(e.runnable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executors::main_queue::{self, MainTask, TEST_GUARD};
    use core::sync::atomic::Ordering;
    use std::sync::MutexGuard;

    /// One static table posting into one static main queue: serialise on
    /// the queue's own test guard so the two modules' tests never overlap.
    fn acquire() -> MutexGuard<'static, ()> {
        let guard = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        main_queue::init();
        while main_queue::try_recv().is_some() {}
        init();
        TEST_NOW_MS.store(1_000, Ordering::Relaxed);
        guard
    }

    fn advance(ms: u32) {
        TEST_NOW_MS.fetch_add(ms, Ordering::Relaxed);
    }

    fn drain_runnables() -> Vec<u16> {
        let mut out = Vec::new();
        while let Some(t) = main_queue::try_recv() {
            if let MainTask::Runnable(r) = t {
                out.push(r);
            }
        }
        out
    }

    fn roots() -> Vec<u16> {
        let mut v = Vec::new();
        visit_pending_runnable_roots(&mut |r| v.push(r));
        v
    }

    #[test]
    fn one_shot_fires_once_when_due_and_frees_its_slot() {
        let _g = acquire();
        let id = schedule(7, 30, 0, Kind::OneShot).unwrap();
        advance(29);
        fire_due();
        assert!(drain_runnables().is_empty(), "not yet due");
        assert_eq!(roots(), vec![7], "rooted while armed");
        assert_eq!(delay_ms(id), Some(1));
        advance(1);
        fire_due();
        assert_eq!(drain_runnables(), vec![7], "due on the deadline itself");
        assert!(roots().is_empty(), "freed on post");
        assert!(!cancel(id), "a posted one-shot is gone");
        assert_eq!(delay_ms(id), None);
        advance(1_000);
        fire_due();
        assert!(drain_runnables().is_empty(), "fires once");
    }

    #[test]
    fn fixed_rate_keeps_its_schedule() {
        let _g = acquire();
        let id = schedule(9, 0, 20, Kind::FixedRate).unwrap();
        fire_due();
        assert_eq!(drain_runnables(), vec![9], "initial delay 0 fires at once");
        fire_due();
        assert!(drain_runnables().is_empty(), "then waits a period");
        assert_eq!(delay_ms(id), Some(20));
        // Drained 3 ms late: the next deadline is still 20 from the last one.
        advance(23);
        fire_due();
        assert_eq!(drain_runnables(), vec![9]);
        assert_eq!(delay_ms(id), Some(17), "the schedule, not the drain time");
        assert!(cancel(id));
        advance(100);
        fire_due();
        assert!(drain_runnables().is_empty(), "cancelled");
        assert!(roots().is_empty());
    }

    #[test]
    fn fixed_rate_late_by_more_than_a_period_does_not_burst() {
        let _g = acquire();
        let id = schedule(3, 0, 10, Kind::FixedRate).unwrap();
        advance(45); // four periods with no tick
        fire_due();
        assert_eq!(drain_runnables(), vec![3], "one run, not four");
        assert_eq!(delay_ms(id), Some(10), "rescheduled a full period from now");
    }

    #[test]
    fn fixed_delay_waits_for_completion() {
        let _g = acquire();
        let id = schedule(5, 0, 20, Kind::FixedDelay).unwrap();
        fire_due();
        assert_eq!(drain_runnables(), vec![5]);
        advance(30);
        fire_due();
        assert!(
            drain_runnables().is_empty(),
            "in flight until the task reports"
        );
        completed(id);
        assert_eq!(delay_ms(id), Some(20), "a full delay after completion");
        advance(19);
        fire_due();
        assert!(drain_runnables().is_empty());
        advance(1);
        fire_due();
        assert_eq!(drain_runnables(), vec![5]);
    }

    #[test]
    fn stale_ids_miss_a_reused_slot() {
        let _g = acquire();
        let first = schedule(1, 1000, 0, Kind::OneShot).unwrap();
        assert!(cancel(first));
        let second = schedule(2, 1000, 0, Kind::OneShot).unwrap();
        assert_eq!(first & 0xFF, second & 0xFF, "same slot");
        assert_ne!(first, second, "new generation");
        assert!(!cancel(first), "the old id is dead");
        assert_eq!(delay_ms(first), None);
        assert_eq!(delay_ms(second), Some(1000));
        assert!(cancel(second));
    }

    #[test]
    fn table_is_bounded_and_init_clears_it() {
        let _g = acquire();
        for i in 0..CAPACITY {
            assert!(
                schedule(i as u16, 1000, 0, Kind::OneShot).is_some(),
                "slot {i}"
            );
        }
        assert!(schedule(99, 1000, 0, Kind::OneShot).is_none(), "full");
        assert_eq!(roots().len(), CAPACITY);
        init();
        assert!(roots().is_empty());
        assert!(schedule(99, 1000, 0, Kind::OneShot).is_some());
    }

    #[test]
    fn a_full_main_queue_defers_rather_than_drops() {
        let _g = acquire();
        for i in 0..64u16 {
            assert!(main_queue::enqueue_runnable(100 + i));
        }
        let id = schedule(4, 0, 0, Kind::OneShot).unwrap();
        fire_due();
        assert_eq!(roots(), vec![4], "still armed");
        assert_eq!(drain_runnables().len(), 64);
        fire_due();
        assert_eq!(drain_runnables(), vec![4]);
        assert!(!cancel(id));
    }

    #[test]
    fn the_clock_wraps() {
        let _g = acquire();
        TEST_NOW_MS.store(u32::MAX - 5, Ordering::Relaxed);
        let id = schedule(8, 10, 0, Kind::OneShot).unwrap();
        assert_eq!(delay_ms(id), Some(10));
        advance(9);
        fire_due();
        assert!(drain_runnables().is_empty(), "not due across the wrap");
        advance(1);
        fire_due();
        assert_eq!(drain_runnables(), vec![8]);
    }

    #[test]
    fn kinds_match_the_java_constants() {
        assert_eq!(Kind::from_java(0), Some(Kind::OneShot));
        assert_eq!(Kind::from_java(1), Some(Kind::FixedRate));
        assert_eq!(Kind::from_java(2), Some(Kind::FixedDelay));
        assert_eq!(Kind::from_java(3), None);
    }
}
