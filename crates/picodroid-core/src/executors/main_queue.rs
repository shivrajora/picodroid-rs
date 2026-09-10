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

/// Shadow of every Runnable obj_ref currently sitting in the backing queue,
/// in FIFO order. The RTOS queue cannot be iterated, so without this the GC
/// root scan cannot see in-flight Runnables — a lambda whose only reference
/// is the queued word was swept by any collection that ran between post and
/// drain (bugbash F2). Mutations are bracketed by an
/// [`pico_jvm::atomic_section::AtomicSection`], the same guard the heap's
/// compound operations use, because `enqueue_runnable` may run on a
/// different task than the draining UI loop.
struct ShadowCell(Cell<[u16; CAPACITY]>, Cell<usize>);
unsafe impl Sync for ShadowCell {}
static PENDING_RUNNABLES: ShadowCell = ShadowCell(Cell::new([0; CAPACITY]), Cell::new(0));

fn shadow_push(r: u16) {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let mut arr = PENDING_RUNNABLES.0.get();
    let len = PENDING_RUNNABLES.1.get();
    if len < CAPACITY {
        arr[len] = r;
        PENDING_RUNNABLES.0.set(arr);
        PENDING_RUNNABLES.1.set(len + 1);
    }
}

fn shadow_remove(r: u16) {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let mut arr = PENDING_RUNNABLES.0.get();
    let len = PENDING_RUNNABLES.1.get();
    if let Some(pos) = arr[..len].iter().position(|&x| x == r) {
        arr.copy_within(pos + 1..len, pos);
        arr[len - 1] = 0;
        PENDING_RUNNABLES.0.set(arr);
        PENDING_RUNNABLES.1.set(len - 1);
    }
}

fn shadow_clear() {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    PENDING_RUNNABLES.0.set([0; CAPACITY]);
    PENDING_RUNNABLES.1.set(0);
}

/// GC roots: every Runnable still in the queue. See [`PENDING_RUNNABLES`].
pub fn visit_pending_runnable_roots(visit: &mut dyn FnMut(u16)) {
    let arr = PENDING_RUNNABLES.0.get();
    let len = PENDING_RUNNABLES.1.get();
    for &r in &arr[..len] {
        visit(r);
    }
}

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
// Backing store.
//
// This was two `mod backing` blocks — a FreeRTOS queue on device, a
// `Mutex<VecDeque>` + `Condvar` in sim. Both are now the platform's
// `Rtos::queue_*`, so one implementation serves every target. The
// send-wakes-blocked-receiver property that `recv_blocking` depends on is
// part of the `Rtos` contract.
// ─────────────────────────────────────────────────────────────────────

mod backing {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::CAPACITY;
    use crate::rtos::{self, RawQueue, Timeout};

    /// Platform queue handle; `0` means "not created yet".
    static QUEUE: AtomicUsize = AtomicUsize::new(0);

    fn handle() -> Option<RawQueue> {
        match QUEUE.load(Ordering::Acquire) {
            0 => None,
            q => Some(q),
        }
    }

    pub fn init() {
        if let Some(q) = handle() {
            // Re-init between app runs drains rather than recreates, so the
            // handle stays valid for any task that already captured it.
            while rtos::queue_recv(q, Timeout::None).is_some() {}
            return;
        }
        // Leaves QUEUE at 0 if the platform could not allocate, which
        // `try_send` treats the same as "not initialised".
        QUEUE.store(rtos::queue_create(CAPACITY), Ordering::Release);
    }

    pub fn try_send(word: u32) -> bool {
        // Apps that loop forever inside `Application.onCreate` (e.g. blinky)
        // never reach `run_activity`, so the queue is never created.
        // Cross-task posters such as the debug bridge's wake nudge must
        // silently no-op rather than panic. `false` matches the queue-full
        // path and is harmless to every caller.
        let Some(q) = handle() else { return false };
        rtos::queue_send(q, word, Timeout::None)
    }

    pub fn try_recv() -> Option<u32> {
        rtos::queue_recv(handle()?, Timeout::None)
    }

    pub fn recv_blocking() -> u32 {
        // Only called from the UI thread, which always runs `init()` first.
        let q = handle().expect("main_queue not initialised");
        loop {
            if let Some(w) = rtos::queue_recv(q, Timeout::Forever) {
                return w;
            }
            // A blocking receive shouldn't fail, but looping is the
            // conservative choice — better than reporting a spurious
            // LvglTick (encoded as 0) to the dispatcher.
        }
    }
}

/// Initialise the queue backing store. Safe to call repeatedly: the first
/// call creates the queue, later ones drain it so a re-run starts empty.
pub fn init() {
    TICK_IN_QUEUE.0.set(false);
    backing::init();
    // init() drains any queue left from the previous app run, so the shadow
    // starts empty too (its refs point into the old heap).
    shadow_clear();
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
    let sent = backing::try_send(encode(MainTask::Runnable(obj_ref)));
    if sent {
        shadow_push(obj_ref);
    }
    sent
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
pub fn try_recv() -> Option<MainTask> {
    let word = backing::try_recv()?;
    let task = decode(word);
    if task == MainTask::LvglTick {
        TICK_IN_QUEUE.0.set(false);
    }
    if let MainTask::Runnable(r) = task {
        shadow_remove(r);
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
    if let MainTask::Runnable(r) = task {
        shadow_remove(r);
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
    fn queued_runnables_are_visited_as_gc_roots() {
        // A lambda whose only reference is the queued word must stay a root
        // from post to drain (bugbash F2).
        let _guard = acquire();
        init();
        let collect = || {
            let mut v = alloc::vec::Vec::new();
            visit_pending_runnable_roots(&mut |r| v.push(r));
            v
        };
        assert!(collect().is_empty());
        assert!(enqueue_runnable(7));
        assert!(enqueue_tick());
        assert!(enqueue_runnable(9));
        assert_eq!(collect(), alloc::vec![7, 9]);
        // Draining a tick leaves both; draining a runnable drops just it.
        while let Some(t) = try_recv() {
            if t == MainTask::Runnable(7) {
                break;
            }
        }
        assert_eq!(collect(), alloc::vec![9]);
        init();
        assert!(collect().is_empty());
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
