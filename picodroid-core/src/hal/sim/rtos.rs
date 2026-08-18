// SPDX-License-Identifier: GPL-3.0-only
//! Host backing for [`crate::rtos::Rtos`] — `std` threads, condvars and
//! mutexes.
//!
//! # This is the `cargo test` backing, not a simulator runtime
//!
//! Simulator *runs* use the real FreeRTOS kernel ([`super::rtos_freertos`]);
//! this file is compiled only when this crate is itself the test target. The
//! reason is measured rather than stylistic: the test harness runs cases
//! concurrently on threads it owns and never starts a scheduler, so the
//! kernel's task APIs dereference a "current task" that does not exist and the
//! process segfaults. A backing made of host primitives has no such
//! precondition.
//!
//! What that costs is visible in `spawn` below: with no scheduler there is
//! nothing to make a Java thread safe, so it is refused. Tests that need real
//! threading belong in the simulator, not here.
//!
//! Handles are pointers to leaked boxes, which round-trip through the seam's
//! `usize` unchanged on a 64-bit host.
//!
//! These are free functions rather than an `Rtos` impl because two callers
//! want different subsets: [`crate::register_sim_platform`] generates a full
//! impl from all of them, while this crate's own test platform delegates the
//! queue, mutex and semaphore primitives but keeps a spawn that refuses and a
//! tick that does nothing. Before, those two lived in different crates and
//! duplicated the primitives; `test_platform.rs` said the dedup was due when
//! the simulator moved here, and this is it.
//!
//! Everything here is test-harness policy, not family policy — the
//! parity-strict refusal, the recursive-mutex bookkeeping Java monitors need,
//! the drift-free tick. A family that behaved differently on any of them
//! would have a parity bug, not a port.

use alloc::boxed::Box;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use super::allocator;
use crate::rtos::{RawMutex, RawQueue, RawSem, RawTask, TaskKind, TaskSpec, Timeout};

// Tick-source state. Separate from the queue/semaphore handles because the
// tick is a process singleton with a lifecycle (start/pause/stop), not
// something callers allocate.
static TICK_STARTED: AtomicBool = AtomicBool::new(false);
static TICK_PAUSED: AtomicBool = AtomicBool::new(false);
static TICK_STOPPING: AtomicBool = AtomicBool::new(false);
static TICK_HANDLE: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

/// Generic in the element type so the word and pointer queues are one
/// implementation. They differ only in what they carry, and a second copy of
/// the wait/timeout dance is exactly the kind of twin that drifts.
struct SimQueue<T> {
    items: Mutex<VecDeque<T>>,
    ready: Condvar,
    depth: usize,
}

/// Per-thread notification slot — this backing's stand-in for a FreeRTOS
/// direct-to-task notification.
///
/// The count is a count, not a flag, for the reason the seam gives: two
/// notifiers racing must not collapse into one wake. Taking it clears the
/// count rather than decrementing, matching `take_notification(clear = true)`
/// on both kernel arms.
struct Notif {
    count: Mutex<u32>,
    ready: Condvar,
}

thread_local! {
    /// Leaked so a `RawTask` stays valid for the process, which is the seam's
    /// contract: a handle may be notified by another thread after the owner
    /// has stopped waiting on it. Threads in a test binary are few and the
    /// slot is two words.
    static SELF_NOTIF: &'static Notif = {
        let _bypass = allocator::bypass();
        Box::leak(Box::new(Notif {
            count: Mutex::new(0),
            ready: Condvar::new(),
        }))
    };
}

struct SimSem {
    count: Mutex<u32>,
    ready: Condvar,
}

/// Recursive mutex over a host thread. Tracked by owner + depth because
/// `std::sync::Mutex` is not reentrant and Java monitors re-enter.
struct SimMutex {
    state: Mutex<(Option<std::thread::ThreadId>, u32)>,
    ready: Condvar,
}

fn wait_deadline(t: Timeout) -> Option<Duration> {
    match t {
        Timeout::None => Some(Duration::ZERO),
        Timeout::Ms(ms) => Some(Duration::from_millis(ms as u64)),
        Timeout::Forever => None,
    }
}

/// Spawn a host thread for `spec`, except for [`TaskKind::JvmChild`].
///
/// `charge_task` is called when a JVM child is refused, so the platform can
/// bill its boot budget for the stack and TCB the device would have
/// allocated. It is a parameter rather than a hook because the boot budget is
/// chip-gated platform data that shared code has no business reading. Its
/// return value — the device-modeled stack size — is only meaningful to the
/// FreeRTOS backing, which sizes a real task from it; here the charge is the
/// whole point and the number is discarded.
///
/// `release_task` is unused in this backing and exists so the two `rtos`
/// modules stay interchangeable: nothing is released because nothing ran.
/// A refused `Thread.start` has no exit to hook, and the host threads this
/// *does* spawn model device tasks that live for the process.
pub fn spawn(
    spec: &TaskSpec,
    body: Box<dyn FnOnce() + Send>,
    charge_task: fn(&TaskSpec) -> u32,
    _release_task: fn(&TaskSpec),
) -> bool {
    // A JVM child task must NOT become a host thread. The object heap is not
    // thread-safe; on device its safety comes from FreeRTOS cooperative
    // scheduling on a single pinned core, which preemptive host threads do
    // not provide. Running it here would trade a visible no-op for silent
    // heap corruption.
    //
    // Erroring would misrepresent the device worse than skipping, and running
    // it synchronously would invert concurrency ordering and can deadlock on
    // the main queue — so warn, charge, skip.
    if spec.kind == TaskKind::JvmChild {
        // Parity/CI lanes treat the no-op as fatal: an app whose threads
        // never ran must not report PASS (docs/parity-audit.md THR-01; real
        // sim threads are fix M7).
        if std::env::var("PICODROID_PARITY_STRICT").as_deref() == Ok("1") {
            panic!(
                "[sim] parity-strict: Thread.start({}) is a no-op in the \
                 simulator — this app cannot be validated here \
                 (docs/parity-audit.md THR-01)",
                spec.name
            );
        }
        eprintln!(
            "[sim] Thread.start: {}.run() will NOT run — threads are a \
             no-op in the simulator (on device they run as a FreeRTOS task)",
            spec.name
        );
        charge_task(spec);
        drop(body);
        return false;
    }

    // Thread internals are host-only, with no device counterpart, so they
    // must not be charged to the simulated device heap.
    let _bypass = allocator::bypass();
    std::thread::Builder::new()
        .name(spec.name.into())
        .spawn(body)
        .is_ok()
}

fn q_create<T>(depth: usize) -> RawQueue {
    let _bypass = allocator::bypass();
    Box::into_raw(Box::new(SimQueue::<T> {
        items: Mutex::new(VecDeque::with_capacity(depth)),
        ready: Condvar::new(),
        depth,
    })) as RawQueue
}

/// # Safety
///
/// `q` must be 0 or a handle from `q_create::<T>` with the same `T`.
unsafe fn q_send<T>(q: RawQueue, val: T, _t: Timeout) -> bool {
    if q == 0 {
        return false;
    }
    let queue = unsafe { &*(q as *const SimQueue<T>) };
    let mut items = queue
        .items
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if items.len() >= queue.depth {
        return false;
    }
    items.push_back(val);
    queue.ready.notify_one();
    true
}

/// # Safety
///
/// See [`q_send`].
unsafe fn q_recv<T>(q: RawQueue, t: Timeout) -> Option<T> {
    if q == 0 {
        return None;
    }
    let queue = unsafe { &*(q as *const SimQueue<T>) };
    let mut items = queue
        .items
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    loop {
        if let Some(v) = items.pop_front() {
            return Some(v);
        }
        match wait_deadline(t) {
            Some(d) if d.is_zero() => return None,
            Some(d) => {
                let (guard, timed_out) = queue
                    .ready
                    .wait_timeout(items, d)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                items = guard;
                if timed_out.timed_out() {
                    return items.pop_front();
                }
            }
            None => {
                items = queue
                    .ready
                    .wait(items)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
    }
}

pub fn queue_create(depth: usize) -> RawQueue {
    q_create::<u32>(depth)
}

pub fn queue_send(q: RawQueue, word: u32, t: Timeout) -> bool {
    // SAFETY: `q` came from `queue_create`, which is `q_create::<u32>`.
    unsafe { q_send(q, word, t) }
}

pub fn queue_recv(q: RawQueue, t: Timeout) -> Option<u32> {
    // SAFETY: see `queue_send`.
    unsafe { q_recv(q, t) }
}

pub fn task_current() -> RawTask {
    SELF_NOTIF.with(|n| (*n as *const Notif) as RawTask)
}

/// Always false: this backing exists precisely because `cargo test` never
/// starts a scheduler. Callers that branch on it take their inline path,
/// which is the correct one for a test binary.
pub fn scheduler_running() -> bool {
    false
}

pub fn task_notify(t: RawTask) {
    if t == 0 {
        return;
    }
    // SAFETY: `t` came from `task_current`, whose slot is leaked for the
    // process — so this is valid even if the notified thread has since exited.
    let notif = unsafe { &*(t as *const Notif) };
    let mut count = notif
        .count
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *count = count.saturating_add(1);
    notif.ready.notify_one();
}

pub fn task_wait_notification(t: Timeout) -> bool {
    SELF_NOTIF.with(|notif| {
        let mut count = notif
            .count
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if *count > 0 {
                *count = 0;
                return true;
            }
            match wait_deadline(t) {
                Some(d) if d.is_zero() => return false,
                Some(d) => {
                    let (guard, timed_out) = notif
                        .ready
                        .wait_timeout(count, d)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    count = guard;
                    if timed_out.timed_out() && *count == 0 {
                        return false;
                    }
                }
                None => {
                    count = notif
                        .ready
                        .wait(count)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
            }
        }
    })
}

pub fn queue_create_ptr(depth: usize) -> RawQueue {
    q_create::<usize>(depth)
}

pub fn queue_send_ptr(q: RawQueue, val: usize, t: Timeout) -> bool {
    // SAFETY: `q` came from `queue_create_ptr`, which is `q_create::<usize>`.
    unsafe { q_send(q, val, t) }
}

pub fn queue_recv_ptr(q: RawQueue, t: Timeout) -> Option<usize> {
    // SAFETY: see `queue_send_ptr`.
    unsafe { q_recv(q, t) }
}

pub fn mutex_recursive_create() -> Option<RawMutex> {
    let _bypass = allocator::bypass();
    Some(Box::into_raw(Box::new(SimMutex {
        state: Mutex::new((None, 0)),
        ready: Condvar::new(),
    })) as RawMutex)
}

pub fn mutex_recursive_lock(m: RawMutex, t: Timeout) -> bool {
    if m == 0 {
        return false;
    }
    let mutex = unsafe { &*(m as *const SimMutex) };
    let me = std::thread::current().id();
    let mut state = mutex
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    loop {
        match state.0 {
            // Free, or already ours — recursion just deepens.
            None => {
                *state = (Some(me), 1);
                return true;
            }
            Some(owner) if owner == me => {
                state.1 += 1;
                return true;
            }
            Some(_) => match wait_deadline(t) {
                Some(d) if d.is_zero() => return false,
                Some(d) => {
                    let (guard, timed_out) = mutex
                        .ready
                        .wait_timeout(state, d)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state = guard;
                    if timed_out.timed_out() && state.0.is_some() {
                        return false;
                    }
                }
                None => {
                    state = mutex
                        .ready
                        .wait(state)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
            },
        }
    }
}

pub fn mutex_recursive_unlock(m: RawMutex) {
    if m == 0 {
        return;
    }
    let mutex = unsafe { &*(m as *const SimMutex) };
    let mut state = mutex
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.1 > 1 {
        state.1 -= 1;
    } else {
        *state = (None, 0);
        mutex.ready.notify_one();
    }
}

pub fn sem_binary_create() -> RawSem {
    let _bypass = allocator::bypass();
    Box::into_raw(Box::new(SimSem {
        count: Mutex::new(0),
        ready: Condvar::new(),
    })) as RawSem
}

pub fn sem_give(s: RawSem) {
    if s == 0 {
        return;
    }
    let sem = unsafe { &*(s as *const SimSem) };
    let mut count = sem
        .count
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *count = 1; // binary
    sem.ready.notify_one();
}

pub fn sem_take(s: RawSem, t: Timeout) -> bool {
    if s == 0 {
        return false;
    }
    let sem = unsafe { &*(s as *const SimSem) };
    let mut count = sem
        .count
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    loop {
        if *count > 0 {
            *count = 0;
            return true;
        }
        match wait_deadline(t) {
            Some(d) if d.is_zero() => return false,
            Some(d) => {
                let (guard, timed_out) = sem
                    .ready
                    .wait_timeout(count, d)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                count = guard;
                if timed_out.timed_out() && *count == 0 {
                    return false;
                }
            }
            None => {
                count = sem
                    .ready
                    .wait(count)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
    }
}

pub fn tick_timer_start(period_ms: u32, cb: fn()) {
    if TICK_STARTED.swap(true, Ordering::SeqCst) {
        // Already running — treat as an unpause.
        TICK_PAUSED.store(false, Ordering::SeqCst);
        return;
    }
    TICK_STOPPING.store(false, Ordering::SeqCst);
    TICK_PAUSED.store(false, Ordering::SeqCst);
    // Host thread machinery has no device analog (the device tick is a
    // FreeRTOS software timer, whose cost enters via the boot budget), so
    // keep pthread internals and this thread's pacing state off the simulated
    // heap (docs/parity-audit.md M1).
    let _spawn_bypass = allocator::bypass();
    let handle = std::thread::Builder::new()
        .name("lvgl-tick".into())
        .spawn(move || {
            let _bypass = allocator::bypass();
            let period = Duration::from_millis(period_ms as u64);
            // Deadline-based rather than sleep-per-iteration, so a slow
            // callback does not make the tick drift.
            let mut next = std::time::Instant::now() + period;
            while !TICK_STOPPING.load(Ordering::SeqCst) {
                let now = std::time::Instant::now();
                if now < next {
                    std::thread::sleep(next - now);
                }
                next = std::time::Instant::now() + period;
                if !TICK_PAUSED.load(Ordering::SeqCst) && !TICK_STOPPING.load(Ordering::SeqCst) {
                    cb();
                }
            }
        })
        .expect("spawn lvgl-tick thread");
    *TICK_HANDLE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(handle);
}

pub fn tick_timer_pause() {
    TICK_PAUSED.store(true, Ordering::SeqCst);
}

pub fn tick_timer_resume() {
    TICK_PAUSED.store(false, Ordering::SeqCst);
}

/// Signal and join. Unlike a device arm, which parks a singleton timer, the
/// host tick owns a thread — leaving it running would leak one per app
/// reload.
pub fn tick_timer_stop() {
    TICK_STOPPING.store(true, Ordering::SeqCst);
    let handle = TICK_HANDLE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    if let Some(h) = handle {
        let _ = h.join();
    }
    TICK_STARTED.store(false, Ordering::SeqCst);
}

pub fn delay_ms(ms: u32) {
    std::thread::sleep(Duration::from_millis(ms as u64));
}

/// Whether the calling thread is a kernel task. This backing has no kernel —
/// tasks are host threads and std blocking is the real blocking — so the
/// answer is always no, and callers asking in order to pick a kernel-visible
/// block correctly fall through to std. Mirrors
/// `rtos_freertos::current_thread_is_task`.
pub fn current_thread_is_task() -> bool {
    false
}
