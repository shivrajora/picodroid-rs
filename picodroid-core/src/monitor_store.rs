// SPDX-License-Identifier: GPL-3.0-only
//! Global monitor table shared across all JVM threads.
//!
//! Each Java `synchronized` block lazily creates one recursive mutex, keyed
//! by the monitor's identity and shared across threads — Java's per-object
//! monitor semantics.
//!
//! This used to carry two implementations: real FreeRTOS mutexes under
//! `family-rp`, and no-ops for every other build (where the JVM ran
//! single-threaded and `synchronized` was entered and exited without ever
//! locking anything). Both are gone: [`crate::rtos`] gives every platform a
//! recursive mutex, so there is one implementation and the simulator now
//! enforces the same monitor discipline the device does.
//!
//! Recursion matters here. `monitorenter` on a monitor the current thread
//! already holds must succeed, or a `synchronized` method calling another
//! `synchronized` method on the same object would deadlock — hence
//! `mutex_recursive_*` rather than a plain mutex.

use alloc::vec::Vec;

use pico_jvm::types::{JvmError, MonitorKey};

use crate::rtos::{self, RawMutex, Timeout};

struct MonitorStoreCell(core::cell::UnsafeCell<Vec<(MonitorKey, RawMutex)>>);

// SAFETY: same single-core guarantee the object heap relies on (see app.rs's
// SharedHeapCell). The Vec mutation is a short find-or-create critical
// region; the lock/unlock operations themselves are handled by the
// platform's own synchronisation.
unsafe impl Sync for MonitorStoreCell {}

static MONITORS: MonitorStoreCell = MonitorStoreCell(core::cell::UnsafeCell::new(Vec::new()));

fn monitors() -> &'static mut Vec<(MonitorKey, RawMutex)> {
    unsafe { &mut *MONITORS.0.get() }
}

/// `monitorenter`: acquire the monitor for `key`, creating it on first use.
pub fn enter(key: MonitorKey) -> Result<(), JvmError> {
    let table = monitors();
    let idx = match table.iter().position(|(k, _)| *k == key) {
        Some(i) => i,
        None => {
            let mutex = rtos::mutex_recursive_create().ok_or(JvmError::StackOverflow)?;
            table.push((key, mutex));
            table.len() - 1
        }
    };
    if rtos::mutex_recursive_lock(table[idx].1, Timeout::Forever) {
        Ok(())
    } else {
        Err(JvmError::IllegalMonitorState)
    }
}

/// `monitorexit`: release the monitor for `key`.
///
/// Exiting a monitor that was never entered is an
/// [`JvmError::IllegalMonitorState`], matching the JVMS.
pub fn exit(key: MonitorKey) -> Result<(), JvmError> {
    let table = monitors();
    match table.iter().position(|(k, _)| *k == key) {
        Some(i) => {
            rtos::mutex_recursive_unlock(table[i].1);
            Ok(())
        }
        None => Err(JvmError::IllegalMonitorState),
    }
}

/// Drop every monitor. Called when the heap is reset between app runs.
pub fn clear() {
    monitors().clear();
}
