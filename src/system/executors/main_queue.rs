//! Unified FIFO queue driving the UI thread.
//!
//! One bounded FIFO holds both LVGL tick tokens (`MainTask::LvglTick`) and
//! user-submitted `Runnable` obj_refs (`MainTask::Runnable(u16)`). The event
//! loop in [`crate::lifecycle::run_activity`] enqueues one `LvglTick` per
//! frame boundary and then drains items in strict FIFO order within a
//! 16 ms budget, so LVGL work and app-posted work share one ordering
//! discipline.
//!
//! `LvglTick` coalescing (`TICK_IN_QUEUE`) prevents multiple ticks piling up
//! behind a slow `Runnable` — if a tick is already pending, `enqueue_tick`
//! is a no-op.
//!
//! The payload is encoded into a single `u32`:
//! - bit 31 set  → `Runnable(obj_ref)`, low 16 bits carry the heap index
//! - bit 31 clear → `LvglTick` sentinel (value must be `0`)

use core::cell::Cell;

/// Item kind drained from the main queue.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MainTask {
    /// Frame-boundary tick — drive LVGL + widget callback dispatch.
    LvglTick,
    /// User-submitted `Runnable.run()` dispatch.
    Runnable(u16),
}

const RUNNABLE_TAG: u32 = 0x8000_0000;
const TICK_SENTINEL: u32 = 0;
const CAPACITY: usize = 64;

/// `true` when an `LvglTick` is already enqueued and not yet drained.
/// Coalesces repeat `enqueue_tick` calls so slow Runnables cannot cause
/// ticks to queue up behind them.
///
/// Only touched by the main-loop thread (both `enqueue_tick` and the
/// `LvglTick` branch of `try_recv`), so a plain `Cell<bool>` is enough —
/// no atomic CAS is required, which matters because `thumbv6m-none-eabi`
/// (Cortex-M0+) lacks hardware compare-exchange.
struct TickFlagCell(Cell<bool>);
// SAFETY: write/read callers are both on the UI thread. Background thread
// workers touch `enqueue_runnable` / `backing::try_send` only, neither of
// which mutates this flag.
unsafe impl Sync for TickFlagCell {}

static TICK_IN_QUEUE: TickFlagCell = TickFlagCell(Cell::new(false));

fn encode(task: MainTask) -> u32 {
    match task {
        MainTask::LvglTick => TICK_SENTINEL,
        MainTask::Runnable(r) => RUNNABLE_TAG | r as u32,
    }
}

fn decode(word: u32) -> MainTask {
    if word & RUNNABLE_TAG != 0 {
        MainTask::Runnable((word & 0xFFFF) as u16)
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
/// UI-thread only. The check-and-set is not atomic; callers must not race
/// `enqueue_tick` against itself from multiple tasks.
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
