// SPDX-License-Identifier: GPL-3.0-only
//! The JVM run lock: one interpreting task at a time.
//!
//! The shared JVM heap has no locks of its own. Its contract is that the
//! tasks running Java — the UI task, `Thread.start` children, the
//! background-pool workers — change places only at blocking points: a
//! sleep, a monitor wait, a queue the task drains, a socket. Until now that
//! was a property of the scheduler configuration (equal priorities, time
//! slicing off) plus [`pico_jvm::atomic_section`] around the compound heap
//! mutations, and it held only as long as nothing else made the kernel pick
//! a *different* ready task at the JVM tier. Anything does: when a
//! higher-priority task wakes and blocks again, FreeRTOS resumes the *next*
//! ready task of the interrupted tier, not the interrupted one
//! (`listGET_OWNER_OF_NEXT_ENTRY` in `vTaskSwitchContext`). The simulator's
//! debug-bridge task, polling its socket at 100 Hz, did exactly that: a
//! child mid-`StringBuilder.append` lost the core to a sibling that then
//! collected the heap under its unrooted temporaries (`threadstress`, nightly
//! 2026-09-15: `InvalidReference`, spurious `OutOfMemoryError`, corrupted
//! rounds). A device's tick timer, sensor sampler and USB bridge can do the
//! same, more rarely.
//!
//! This module makes the contract structural. A task takes the lock before
//! it interprets ([`Held`]) and gives it up around every blocking wait
//! ([`unlocked`], applied inside the `rtos` seam wrappers and the network
//! facade, so no caller has to remember). A task the kernel rotates in
//! while another holds the lock blocks on the mutex — it is not in the
//! ready list, so it never touches the heap — and the holder resumes. What
//! a Java thread observes is unchanged: it always ran until it blocked.
//!
//! Rules: never take the lock inside an [`pico_jvm::atomic_section`] (the
//! kernel is suspended there), and never block while holding it other than
//! through a wrapper that releases it. With no kernel (`cargo test`) every
//! operation here is a no-op.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::rtos::{self, RawMutex, RawTask, Timeout};

/// The kernel mutex, or 0 before [`init`].
static LOCK: AtomicUsize = AtomicUsize::new(0);
/// The task that holds it, or 0.
static HOLDER: AtomicUsize = AtomicUsize::new(0);

/// Create the lock. Call once, before the first task exists; both boots do
/// so from `rtos::freertos::install_heap_atomic_hooks`.
pub fn init() {
    if LOCK.load(Ordering::Acquire) != 0 {
        return;
    }
    if let Some(m) = rtos::mutex_recursive_create() {
        LOCK.store(m, Ordering::Release);
    }
}

fn lock() -> Option<RawMutex> {
    let m = LOCK.load(Ordering::Acquire);
    (m != 0).then_some(m)
}

/// The calling task, or 0 when no task is running (boot code, a host
/// thread the kernel does not own).
fn current() -> RawTask {
    if rtos::scheduler_running() {
        rtos::task_current()
    } else {
        0
    }
}

/// Whether the calling task holds the run lock.
pub fn holds() -> bool {
    let me = current();
    me != 0 && HOLDER.load(Ordering::Acquire) == me
}

/// Take the lock for the calling task; `true` when this call took it (as
/// opposed to already holding it, or there being no lock to take).
fn take() -> bool {
    let Some(m) = lock() else {
        return false;
    };
    let me = current();
    if me == 0 || HOLDER.load(Ordering::Acquire) == me {
        return false;
    }
    // A `Forever` take fails only when the wait was ended from outside — the
    // debug bridge's app stop aborts every child's delay, a mutex wait
    // included. The task still has to unwind through the heap (monitors to
    // release, a frame to pop), so it waits for the lock regardless; the
    // holder gives it up at its next blocking point.
    // spin-ok: every iteration blocks in the kernel until the mutex is given
    while !rtos::mutex_recursive_lock_unhooked(m, Timeout::Forever) {}
    HOLDER.store(me, Ordering::Release);
    true
}

fn give() {
    let Some(m) = lock() else {
        return;
    };
    HOLDER.store(0, Ordering::Release);
    rtos::mutex_recursive_unlock(m);
}

/// The lock, held for a scope: a task about to interpret Java takes one at
/// the top of its Java life. Nested holds are free — only the outermost
/// releases.
pub struct Held {
    owned: bool,
}

impl Held {
    pub fn acquire() -> Self {
        Self { owned: take() }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if self.owned {
            give();
        }
    }
}

/// The lock given up for a scope — around a blocking wait — and taken back
/// when the scope ends. A no-op for a task that does not hold it.
pub struct Unlocked {
    reacquire: bool,
}

/// Release the run lock, if the calling task holds it, until the guard drops.
pub fn unlocked() -> Unlocked {
    if holds() {
        give();
        Unlocked { reacquire: true }
    } else {
        Unlocked { reacquire: false }
    }
}

/// [`unlocked`] for a wait of `t`: a non-blocking attempt keeps the lock —
/// giving it up would turn a poll into a switch point.
pub fn unlocked_for(t: Timeout) -> Unlocked {
    if matches!(t, Timeout::None) {
        Unlocked { reacquire: false }
    } else {
        unlocked()
    }
}

impl Drop for Unlocked {
    fn drop(&mut self) {
        if self.reacquire {
            take();
        }
    }
}
