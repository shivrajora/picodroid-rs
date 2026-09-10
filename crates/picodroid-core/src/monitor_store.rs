// SPDX-License-Identifier: GPL-3.0-only
//! Global monitor table shared across all JVM threads.
//!
//! Each Java monitor — the lock behind a `synchronized` block or method —
//! is one recursive kernel mutex, created lazily and keyed by the identity
//! of the object it guards. The table tracks, in Rust, what the kernel does
//! not tell us: which task owns each monitor and how deep its recursion is.
//! That bookkeeping is what lets `monitorexit` reject a non-owner (JVMS
//! `IllegalMonitorStateException`), lets a task that is leaving release
//! everything it still holds, and lets `Object.wait` give a monitor up fully
//! and take it back at the same depth.
//!
//! # Keys are heap slots, and slots are recycled
//!
//! A [`MonitorKey`] is a heap slot index. The collector reuses slots, so a
//! monitor left in the table after its object died would be inherited by
//! whatever object lands in that slot next — false contention at best, a
//! permanent deadlock if the old entry was still marked held. The
//! interpreter therefore calls [`prune_dead`] straight after every
//! collection, before any allocation can recycle a slot, and the table
//! deletes every *free* monitor whose object is gone. A monitor that is
//! held cannot be pruned by construction: javac keeps the locked object in
//! a local for the matching `monitorexit`, so it is rooted for as long as
//! it is held. Pruning also bounds the table — without it every distinct
//! `synchronized` target ever seen kept a kernel mutex forever.
//!
//! # Concurrency
//!
//! Table mutation happens inside an `AtomicSection` (scheduler suspended):
//! the `Vec` may reallocate, and a task preempted mid-push by another JVM
//! task would corrupt it. Blocking on the kernel mutex happens *outside* the
//! section, on a handle copied out of the table, because nothing may block
//! inside one. The handle stays valid across the wait: the object whose
//! monitor it is sits on the waiting task's operand stack, so it is live and
//! [`prune_dead`] leaves it alone. Kernel give/delete calls are non-blocking
//! and are made inside the section, so an entry's kernel state and its Rust
//! bookkeeping can never be observed out of step.
//!
//! On FreeRTOS a recursive mutex can only be given by the task that took it,
//! which is why release-on-exit is [`release_all_held_by_current`] and not a
//! "release everything task X holds" that another task could call.

use alloc::vec::Vec;

use pico_jvm::atomic_section::AtomicSection;
use pico_jvm::types::{JvmError, MonitorKey};

use crate::rtos::{self, RawMutex, RawTask, Timeout};

struct Monitor {
    key: MonitorKey,
    mutex: RawMutex,
    /// Task holding the monitor; `0` when free.
    owner: RawTask,
    /// Recursion depth — the number of `monitorenter`s the owner still has to
    /// balance. `0` iff `owner == 0`.
    depth: u16,
}

struct MonitorStoreCell(core::cell::UnsafeCell<Vec<Monitor>>);

// SAFETY: every access goes through `table()`, whose callers hold an
// `AtomicSection` — see the module docs.
unsafe impl Sync for MonitorStoreCell {}

static MONITORS: MonitorStoreCell = MonitorStoreCell(core::cell::UnsafeCell::new(Vec::new()));

/// # Safety
/// The caller must hold an `AtomicSection` for as long as the reference
/// lives.
unsafe fn table() -> &'static mut Vec<Monitor> {
    &mut *MONITORS.0.get()
}

/// `monitorenter`: acquire the monitor for `key`, creating it on first use.
pub fn enter(key: MonitorKey) -> Result<(), JvmError> {
    enter_with(key, Timeout::Forever).map(|_| ())
}

/// [`enter`] with a bound on the wait. `Ok(false)` means the timeout elapsed
/// and nothing was acquired.
pub fn enter_with(key: MonitorKey, timeout: Timeout) -> Result<bool, JvmError> {
    let me = rtos::task_current();
    let mutex = {
        let _atomic = AtomicSection::enter();
        // SAFETY: inside the section.
        let table = unsafe { table() };
        match table.iter().find(|m| m.key == key) {
            Some(m) => m.mutex,
            None => {
                // Allocates from the kernel heap; heap_4's own suspension
                // nests inside ours.
                let mutex = rtos::mutex_recursive_create().ok_or(JvmError::StackOverflow)?;
                table.push(Monitor {
                    key,
                    mutex,
                    owner: 0,
                    depth: 0,
                });
                mutex
            }
        }
    };

    // Block outside the section, on the copied handle (module docs).
    if !rtos::mutex_recursive_lock(mutex, timeout) {
        return match timeout {
            Timeout::Forever => Err(JvmError::IllegalMonitorState),
            _ => Ok(false),
        };
    }

    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    match unsafe { table() }.iter_mut().find(|m| m.key == key) {
        Some(m) => {
            m.owner = me;
            m.depth += 1;
            Ok(true)
        }
        None => {
            // Unreachable while the invariant in the module docs holds: the
            // entry cannot be pruned while its object is on our stack.
            rtos::mutex_recursive_unlock(mutex);
            Err(JvmError::IllegalMonitorState)
        }
    }
}

/// `monitorexit`: release one level of the monitor for `key`.
///
/// Exiting a monitor the calling task does not hold — never entered, or
/// entered by another task — is an [`JvmError::IllegalMonitorState`], as
/// the JVMS requires.
pub fn exit(key: MonitorKey) -> Result<(), JvmError> {
    let me = rtos::task_current();
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let Some(m) = unsafe { table() }.iter_mut().find(|m| m.key == key) else {
        return Err(JvmError::IllegalMonitorState);
    };
    if m.owner != me || m.depth == 0 {
        return Err(JvmError::IllegalMonitorState);
    }
    m.depth -= 1;
    if m.depth == 0 {
        m.owner = 0;
    }
    // Non-blocking, so legal inside the section; a waiter it readies runs
    // when the scheduler resumes.
    rtos::mutex_recursive_unlock(m.mutex);
    Ok(())
}

/// Whether the calling task holds the monitor for `key`.
pub fn held_by_current(key: MonitorKey) -> bool {
    let me = rtos::task_current();
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    unsafe { table() }
        .iter()
        .any(|m| m.key == key && m.owner == me && m.depth > 0)
}

/// Give the monitor for `key` up completely, returning the recursion depth
/// that [`reacquire`] must restore — the first half of `Object.wait`.
/// Fails if the calling task does not hold it.
pub fn save_and_release(key: MonitorKey) -> Result<u16, JvmError> {
    let me = rtos::task_current();
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let Some(m) = unsafe { table() }.iter_mut().find(|m| m.key == key) else {
        return Err(JvmError::IllegalMonitorState);
    };
    if m.owner != me || m.depth == 0 {
        return Err(JvmError::IllegalMonitorState);
    }
    let depth = m.depth;
    for _ in 0..depth {
        rtos::mutex_recursive_unlock(m.mutex);
    }
    m.depth = 0;
    m.owner = 0;
    Ok(depth)
}

/// Take the monitor for `key` back at `depth` — the second half of
/// `Object.wait`. Blocks until it is free.
pub fn reacquire(key: MonitorKey, depth: u16) -> Result<(), JvmError> {
    for _ in 0..depth {
        enter(key)?;
    }
    Ok(())
}

/// Release every monitor the calling task still holds, returning how many
/// holds (summed over recursion depth) were dropped.
///
/// For a task that is leaving the interpreter through a non-Java error — a
/// debugger stop, an internal fault — which skipped the `monitorexit`
/// handlers javac emits. Zero on the normal path.
pub fn release_all_held_by_current() -> usize {
    let me = rtos::task_current();
    let _atomic = AtomicSection::enter();
    let mut released = 0usize;
    // SAFETY: inside the section.
    for m in unsafe { table() }.iter_mut() {
        if m.owner == me && m.depth > 0 {
            for _ in 0..m.depth {
                rtos::mutex_recursive_unlock(m.mutex);
            }
            released += m.depth as usize;
            m.depth = 0;
            m.owner = 0;
        }
    }
    released
}

/// Delete every free monitor whose object is no longer live. Called right
/// after a collection; see the module docs for why held monitors are never
/// candidates.
pub fn prune_dead(live: &dyn Fn(MonitorKey) -> bool) {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    let table = unsafe { table() };
    let mut i = 0;
    while i < table.len() {
        if table[i].depth == 0 && !live(table[i].key) {
            let dead = table.swap_remove(i);
            rtos::mutex_recursive_delete(dead.mutex);
        } else {
            i += 1;
        }
    }
}

/// Drop every monitor. Called when the heap is reset between app runs, once
/// every JVM task has drained.
pub fn clear() {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    for m in unsafe { table() }.drain(..) {
        rtos::mutex_recursive_delete(m.mutex);
    }
}

#[cfg(test)]
fn depth_of(key: MonitorKey) -> u16 {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    unsafe { table() }
        .iter()
        .find(|m| m.key == key)
        .map_or(0, |m| m.depth)
}

#[cfg(test)]
fn has(key: MonitorKey) -> bool {
    let _atomic = AtomicSection::enter();
    // SAFETY: inside the section.
    unsafe { table() }.iter().any(|m| m.key == key)
}

// Under `cargo test` the seam is the std backing (`hal/sim/rtos.rs`): a
// task is a host thread and the recursive mutex is owner/depth bookkeeping
// over `std::sync::Mutex`, so real contention between real threads is
// exercised here. The table is process-global, so the tests serialise on
// `SERIAL` and each uses keys of its own.
#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{mpsc, Mutex, MutexGuard};

    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn illegal<T>(r: Result<T, JvmError>) -> bool {
        matches!(r, Err(JvmError::IllegalMonitorState))
    }

    #[test]
    fn reentrant_enter_tracks_depth() {
        let _g = serial();
        let k = MonitorKey::Object(60_000);
        enter(k).unwrap();
        enter(k).unwrap();
        assert_eq!(depth_of(k), 2);
        assert!(held_by_current(k));
        exit(k).unwrap();
        assert_eq!(depth_of(k), 1);
        exit(k).unwrap();
        assert_eq!(depth_of(k), 0);
        assert!(!held_by_current(k));
        assert!(illegal(exit(k)));
    }

    #[test]
    fn exit_of_a_monitor_never_entered_is_illegal() {
        let _g = serial();
        assert!(illegal(exit(MonitorKey::Array(60_001))));
    }

    #[test]
    fn exit_by_non_owner_is_illegal() {
        let _g = serial();
        let k = MonitorKey::Object(60_002);
        enter(k).unwrap();
        let r = std::thread::spawn(move || exit(k)).join().unwrap();
        assert!(illegal(r));
        assert_eq!(depth_of(k), 1, "a rejected exit must not touch the depth");
        exit(k).unwrap();
    }

    #[test]
    fn contended_monitor_serialises_a_counter() {
        let _g = serial();
        let k = MonitorKey::Object(60_003);
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        COUNTER.store(0, Ordering::Relaxed);
        let workers: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(move || {
                    for _ in 0..500 {
                        enter(k).unwrap();
                        // Deliberately non-atomic read-modify-write: only the
                        // monitor keeps it correct.
                        let v = COUNTER.load(Ordering::Relaxed);
                        std::thread::yield_now();
                        COUNTER.store(v + 1, Ordering::Relaxed);
                        exit(k).unwrap();
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
        assert_eq!(COUNTER.load(Ordering::Relaxed), 2000);
        assert_eq!(depth_of(k), 0);
    }

    #[test]
    fn enter_with_timeout_reports_contention() {
        let _g = serial();
        let k = MonitorKey::Object(60_004);
        let (held_tx, held_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            enter(k).unwrap();
            held_tx.send(()).unwrap();
            done_rx.recv().unwrap();
            exit(k).unwrap();
        });
        held_rx.recv().unwrap();
        assert_eq!(enter_with(k, Timeout::Ms(20)).unwrap(), false);
        done_tx.send(()).unwrap();
        holder.join().unwrap();
        assert_eq!(enter_with(k, Timeout::Ms(20)).unwrap(), true);
        exit(k).unwrap();
    }

    #[test]
    fn release_all_held_by_current_frees_nested_holds() {
        let _g = serial();
        let a = MonitorKey::Object(60_005);
        let b = MonitorKey::String(60_006);
        let (held_tx, held_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            enter(a).unwrap();
            enter(a).unwrap();
            enter(b).unwrap();
            assert_eq!(release_all_held_by_current(), 3);
            assert_eq!(release_all_held_by_current(), 0);
            held_tx.send(()).unwrap();
            done_rx.recv().unwrap();
        });
        held_rx.recv().unwrap();
        // The holder is still alive but holds nothing: both are free.
        assert_eq!(enter_with(a, Timeout::Ms(20)).unwrap(), true);
        assert_eq!(enter_with(b, Timeout::Ms(20)).unwrap(), true);
        exit(a).unwrap();
        exit(b).unwrap();
        done_tx.send(()).unwrap();
        holder.join().unwrap();
    }

    #[test]
    fn save_and_release_then_reacquire_restores_depth() {
        let _g = serial();
        let k = MonitorKey::Object(60_007);
        assert!(illegal(save_and_release(k)));
        enter(k).unwrap();
        enter(k).unwrap();
        assert_eq!(save_and_release(k).unwrap(), 2);
        assert_eq!(depth_of(k), 0);
        // Fully released: another thread can take and give it meanwhile.
        std::thread::spawn(move || {
            assert_eq!(enter_with(k, Timeout::Ms(20)).unwrap(), true);
            exit(k).unwrap();
        })
        .join()
        .unwrap();
        reacquire(k, 2).unwrap();
        assert_eq!(depth_of(k), 2);
        exit(k).unwrap();
        exit(k).unwrap();
    }

    #[test]
    fn prune_dead_removes_only_free_dead_monitors() {
        let _g = serial();
        let held = MonitorKey::Object(60_008);
        let free = MonitorKey::Array(60_009);
        enter(held).unwrap();
        enter(free).unwrap();
        exit(free).unwrap();
        // Pretend `free`'s object died and `held`'s survived.
        prune_dead(&|k| k == held);
        assert!(has(held));
        assert!(!has(free));
        // A held monitor is never a candidate, whatever `live` says.
        prune_dead(&|_| false);
        assert!(has(held));
        exit(held).unwrap();
        prune_dead(&|_| false);
        assert!(!has(held));
    }
}
