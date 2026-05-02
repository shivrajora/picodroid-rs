// SPDX-License-Identifier: GPL-3.0-only
//! Unified FIFO queue driving the UI thread.
//!
//! One bounded FIFO holds three kinds of items: LVGL tick tokens
//! (`MainTask::LvglTick`), user-submitted `Runnable` obj_refs
//! (`MainTask::Runnable(u16)`), and cross-task wake nudges
//! (`MainTask::Wake`). The event loop in [`crate::lifecycle::run_activity`]
//! drains items in strict FIFO order, so LVGL work and app-posted work
//! share one ordering discipline.
//!
//! `LvglTick` coalescing (`TICK_IN_QUEUE`) prevents multiple ticks piling up
//! behind a slow `Runnable` — if a tick is already pending, `enqueue_tick`
//! is a no-op.
//!
//! Posters split by role:
//! - `enqueue_tick`: tick source only (FreeRTOS timer service task on device,
//!   `lvgl-tick` std::thread on sim). Touches `TICK_IN_QUEUE`.
//! - `enqueue_runnable`: any task posting a `Runnable` for UI dispatch
//!   (executors, lambda proxies). Bypasses `TICK_IN_QUEUE`.
//! - `enqueue_wake`: any task that needs the UI thread to re-check
//!   `handler.interrupted()` immediately (used by `pdb::pending::notify_jvm`
//!   so `STOP_JVM` is observed without waiting for the next 16 ms tick).
//!   Bypasses `TICK_IN_QUEUE`.
//!
//! The payload is encoded into a single `u32`:
//! - bit 31 set   → `Runnable(obj_ref)`, low 16 bits carry the heap index
//! - bit 30 set   → `Wake` sentinel
//! - all bits 0   → `LvglTick` sentinel

use core::cell::Cell;

/// Item kind drained from the main queue.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MainTask {
    /// Frame-boundary tick — drive LVGL + widget callback dispatch.
    LvglTick,
    /// User-submitted `Runnable.run()` dispatch.
    Runnable(u16),
    /// Cross-task wake nudge — UI loop should re-check interrupt state and
    /// drain pending lifecycle ops without doing tick work. Carries no
    /// payload; multiple wakes coalesce naturally because the loop's
    /// `handler.interrupted()` / pending-op drain is idempotent.
    Wake,
}

const RUNNABLE_TAG: u32 = 0x8000_0000;
const WAKE_SENTINEL: u32 = 0x4000_0000;
const TICK_SENTINEL: u32 = 0;
const CAPACITY: usize = 64;

/// `true` when an `LvglTick` is already enqueued and not yet drained.
/// Coalesces repeat `enqueue_tick` calls so slow Runnables cannot cause
/// ticks to queue up behind them.
///
/// Touched by the tick source (poster) and the UI thread (drainer in
/// `recv_blocking` / `try_recv`). The tick source runs at the FreeRTOS
/// timer-task priority (max), so its read-modify-write is atomic w.r.t.
/// the UI thread; the UI thread's unconditional `set(false)` after popping
/// any tick guarantees the flag converges even under preemption. A plain
/// `Cell<bool>` is enough — no atomic CAS is required, which matters
/// because `thumbv6m-none-eabi` (Cortex-M0+) lacks hardware compare-exchange.
///
/// Cross-task wakes (`enqueue_wake`) deliberately do NOT touch this flag —
/// they post a separate `Wake` sentinel so the tick-source-owns-coalescing
/// invariant holds regardless of how many tasks need to nudge the UI loop.
struct TickFlagCell(Cell<bool>);
// SAFETY: see TICK_IN_QUEUE doc comment above for the write/read discipline.
unsafe impl Sync for TickFlagCell {}

static TICK_IN_QUEUE: TickFlagCell = TickFlagCell(Cell::new(false));

fn encode(task: MainTask) -> u32 {
    match task {
        MainTask::LvglTick => TICK_SENTINEL,
        MainTask::Runnable(r) => RUNNABLE_TAG | r as u32,
        MainTask::Wake => WAKE_SENTINEL,
    }
}

fn decode(word: u32) -> MainTask {
    if word & RUNNABLE_TAG != 0 {
        MainTask::Runnable((word & 0xFFFF) as u16)
    } else if word & WAKE_SENTINEL != 0 {
        MainTask::Wake
    } else {
        MainTask::LvglTick
    }
}

// ─────────────────────────────────────────────────────────────────────
// Backing store: FreeRTOS queue on device, Mutex<VecDeque> in sim.
// ─────────────────────────────────────────────────────────────────────

#[cfg(not(any(test, feature = "sim")))]
mod backing {
    use core::cell::UnsafeCell;
    use freertos_rust::{Duration, Queue};

    use super::CAPACITY;

    struct QueueCell(UnsafeCell<Option<Queue<u32>>>);
    // SAFETY: installed exactly once in `init()` pre-scheduler; after that
    // `Queue` itself is thread-safe via FreeRTOS primitives.
    unsafe impl Sync for QueueCell {}

    static QUEUE: QueueCell = QueueCell(UnsafeCell::new(None));

    fn queue_opt() -> Option<&'static Queue<u32>> {
        // SAFETY: only mutated by `init()` pre-scheduler; read-only after.
        unsafe { (*QUEUE.0.get()).as_ref() }
    }

    pub fn init() {
        // SAFETY: pre-scheduler single-threaded initialisation.
        unsafe {
            if (*QUEUE.0.get()).is_none() {
                let q = Queue::<u32>::new(CAPACITY).expect("main queue alloc");
                *QUEUE.0.get() = Some(q);
            }
        }
    }

    pub fn try_send(word: u32) -> bool {
        // Apps that loop forever inside `Application.onCreate` (e.g. blinky)
        // never reach `run_activity`, so the queue is never init'd. Cross-task
        // posters such as `pdb::pending::notify_jvm` (called from pdb_task on
        // core 1) must silently no-op rather than panic. Returning `false`
        // matches the queue-full path and is harmless to all callers.
        let Some(q) = queue_opt() else { return false };
        // FreeRTOS Queue::send wakes any task blocked on Queue::receive,
        // which is how `recv_blocking` gets woken sub-ms when a poster runs.
        q.send(word, Duration::zero()).is_ok()
    }

    #[allow(dead_code)]
    pub fn try_recv() -> Option<u32> {
        queue_opt()?.receive(Duration::zero()).ok()
    }

    pub fn recv_blocking() -> u32 {
        // Only called from the UI thread, which always runs `init()` first.
        let q = queue_opt().expect("main_queue not initialised");
        loop {
            if let Ok(w) = q.receive(Duration::infinite()) {
                return w;
            }
            // `Duration::infinite()` shouldn't return Err in practice, but
            // looping is the conservative choice — better than reporting a
            // spurious LvglTick (encoded as 0) to the dispatcher.
        }
    }
}

#[cfg(any(test, feature = "sim"))]
mod backing {
    use std::collections::VecDeque;
    use std::sync::{Condvar, Mutex};

    use super::CAPACITY;

    struct SimQueue {
        queue: Mutex<VecDeque<u32>>,
        cv: Condvar,
    }

    static SIM_QUEUE: SimQueue = SimQueue {
        queue: Mutex::new(VecDeque::new()),
        cv: Condvar::new(),
    };

    pub fn init() {
        SIM_QUEUE.queue.lock().unwrap().clear();
    }

    pub fn try_send(word: u32) -> bool {
        let mut q = SIM_QUEUE.queue.lock().unwrap();
        if q.len() < CAPACITY {
            q.push_back(word);
            drop(q);
            // Wake `recv_blocking`, which provides the same send-wakes-receive
            // semantics that FreeRTOS Queue::send gives us on device.
            SIM_QUEUE.cv.notify_one();
            true
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn try_recv() -> Option<u32> {
        SIM_QUEUE.queue.lock().unwrap().pop_front()
    }

    pub fn recv_blocking() -> u32 {
        let mut guard = SIM_QUEUE
            .cv
            .wait_while(SIM_QUEUE.queue.lock().unwrap(), |q| q.is_empty())
            .unwrap();
        guard.pop_front().expect("queue non-empty after wait_while")
    }
}

/// Initialise the queue backing store. Safe to call repeatedly; subsequent
/// calls are no-ops on device and reset the sim queue.
pub fn init() {
    TICK_IN_QUEUE.0.set(false);
    backing::init();
}

/// Enqueue an `LvglTick` if one is not already pending. Returns `true` if
/// the tick was posted, `false` if coalesced (or if the queue was full).
///
/// **Tick-source only.** This is the only function that touches
/// `TICK_IN_QUEUE`; callers from other tasks must use [`enqueue_wake`]
/// instead so the coalescing invariant stays well-defined.
pub fn enqueue_tick() -> bool {
    if TICK_IN_QUEUE.0.get() {
        return false;
    }
    TICK_IN_QUEUE.0.set(true);
    if !backing::try_send(encode(MainTask::LvglTick)) {
        TICK_IN_QUEUE.0.set(false);
        return false;
    }
    true
}

/// Enqueue a `Runnable` obj_ref. Non-blocking; drops the item if the queue
/// is full (returns `false`). Caller is expected to log on drop.
pub fn enqueue_runnable(obj_ref: u16) -> bool {
    backing::try_send(encode(MainTask::Runnable(obj_ref)))
}

/// Post a `Wake` sentinel to the queue so the UI loop's `recv_blocking`
/// returns immediately and the next iteration re-checks
/// `handler.interrupted()` and drains pending lifecycle ops.
///
/// Safe to call from any FreeRTOS task. Bypasses `TICK_IN_QUEUE` entirely
/// — coalescing is the tick source's concern only. Returns `false` if the
/// queue is full or uninitialised; both cases are silently absorbed by
/// callers (the loop will still wake on the next tick).
///
/// `#[allow(dead_code)]` because the sole caller (`pdb::pending::notify_jvm`)
/// is gated out of sim builds; the sim still exercises this through the
/// unit tests below.
#[allow(dead_code)]
pub fn enqueue_wake() -> bool {
    backing::try_send(encode(MainTask::Wake))
}

/// Pop one `MainTask` without blocking. Returns `None` if the queue is
/// empty. Clears the tick-pending flag when an `LvglTick` is drained so
/// the next frame can post a fresh one. UI-thread only.
///
/// The activity loop uses [`recv_blocking`] in steady state; `try_recv`
/// is retained for tests and as a non-blocking peek primitive that
/// future callers may need (e.g. a draining shutdown helper).
#[allow(dead_code)]
pub fn try_recv() -> Option<MainTask> {
    let word = backing::try_recv()?;
    let task = decode(word);
    if task == MainTask::LvglTick {
        TICK_IN_QUEUE.0.set(false);
    }
    Some(task)
}

/// Block the calling task/thread until a `MainTask` is available, then
/// return it. UI-thread only — the unified queue assumes a single drainer.
///
/// This is the wake-on-post primitive: posters call `enqueue_runnable`
/// (FreeRTOS `Queue::send` on device, `Condvar::notify_one` on sim) and
/// the blocked drainer wakes within microseconds.
pub fn recv_blocking() -> MainTask {
    let word = backing::recv_blocking();
    let task = decode(word);
    if task == MainTask::LvglTick {
        TICK_IN_QUEUE.0.set(false);
    }
    task
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // Tests share the same static queue + tick flag. `cargo test` runs
    // tests concurrently, so serialise them behind this mutex.
    static TEST_GUARD: Mutex<()> = Mutex::new(());

    fn acquire() -> MutexGuard<'static, ()> {
        let guard = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        init();
        while try_recv().is_some() {}
        TICK_IN_QUEUE.0.set(false);
        guard
    }

    #[test]
    fn encode_decode_round_trip() {
        assert_eq!(decode(encode(MainTask::LvglTick)), MainTask::LvglTick);
        assert_eq!(decode(encode(MainTask::Wake)), MainTask::Wake);
        assert_eq!(decode(encode(MainTask::Runnable(0))), MainTask::Runnable(0));
        assert_eq!(
            decode(encode(MainTask::Runnable(0xFFFF))),
            MainTask::Runnable(0xFFFF)
        );
        assert_eq!(
            decode(encode(MainTask::Runnable(42))),
            MainTask::Runnable(42)
        );
    }

    #[test]
    fn wake_does_not_touch_tick_flag() {
        let _guard = acquire();
        // No tick is in the queue; the flag must remain false after a wake.
        assert!(enqueue_wake(), "wake should post");
        assert!(
            !TICK_IN_QUEUE.0.get(),
            "wake must not set TICK_IN_QUEUE — that's the tick source's job"
        );
        assert_eq!(try_recv(), Some(MainTask::Wake));
        assert!(
            !TICK_IN_QUEUE.0.get(),
            "draining wake must not flip the flag"
        );
    }

    #[test]
    fn wake_does_not_block_subsequent_tick_coalescing() {
        let _guard = acquire();
        // A wake in flight must not prevent the tick source from posting
        // a fresh tick (or coalescing repeats of one).
        assert!(enqueue_wake());
        assert!(enqueue_tick(), "tick post after wake");
        assert!(!enqueue_tick(), "tick still coalesces while wake is queued");
        assert_eq!(try_recv(), Some(MainTask::Wake));
        assert_eq!(try_recv(), Some(MainTask::LvglTick));
        // After the tick is drained, the next post succeeds again.
        assert!(enqueue_tick(), "post after drain succeeds");
        assert_eq!(try_recv(), Some(MainTask::LvglTick));
    }

    #[test]
    fn tick_coalesces_until_drained() {
        let _guard = acquire();
        assert!(enqueue_tick(), "first tick should post");
        assert!(!enqueue_tick(), "second tick coalesced");
        assert!(!enqueue_tick(), "third tick coalesced");
        assert_eq!(try_recv(), Some(MainTask::LvglTick));
        assert_eq!(try_recv(), None);
        // Drained — next tick post succeeds again.
        assert!(enqueue_tick(), "post after drain should succeed");
        assert_eq!(try_recv(), Some(MainTask::LvglTick));
    }

    #[test]
    fn fifo_ordering_mixed() {
        let _guard = acquire();
        assert!(enqueue_runnable(10));
        assert!(enqueue_tick());
        assert!(enqueue_runnable(20));
        assert!(enqueue_runnable(30));
        assert_eq!(try_recv(), Some(MainTask::Runnable(10)));
        assert_eq!(try_recv(), Some(MainTask::LvglTick));
        assert_eq!(try_recv(), Some(MainTask::Runnable(20)));
        assert_eq!(try_recv(), Some(MainTask::Runnable(30)));
        assert_eq!(try_recv(), None);
    }

    #[test]
    fn recv_blocking_returns_immediately_when_queue_has_item() {
        let _guard = acquire();
        assert!(enqueue_runnable(7));
        assert_eq!(recv_blocking(), MainTask::Runnable(7));
    }

    #[test]
    fn recv_blocking_wakes_when_runnable_is_posted() {
        let _guard = acquire();
        let posted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let posted_clone = posted.clone();
        // Spawn a poster that waits a moment and then enqueues. The main
        // thread blocks in `recv_blocking` until the post wakes it.
        let h = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            posted_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            assert!(enqueue_runnable(123));
        });
        let task = recv_blocking();
        assert!(
            posted.load(std::sync::atomic::Ordering::SeqCst),
            "recv_blocking returned before the post was made"
        );
        assert_eq!(task, MainTask::Runnable(123));
        h.join().unwrap();
    }

    #[test]
    fn overflow_drops_runnable() {
        let _guard = acquire();
        // Fill to capacity with runnables.
        for i in 0..CAPACITY {
            assert!(enqueue_runnable(i as u16), "fill slot {i}");
        }
        // One more must fail (bounded).
        assert!(!enqueue_runnable(999), "overflow should drop");
        // Drain everything so the next test starts clean.
        for _ in 0..CAPACITY {
            assert!(try_recv().is_some());
        }
        assert_eq!(try_recv(), None);
    }
}
