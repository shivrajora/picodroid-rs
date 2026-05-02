// SPDX-License-Identifier: GPL-3.0-only
//! Global monitor table shared across all JVM threads.
//!
//! Each Java `synchronized` block creates a FreeRTOS recursive mutex lazily.
//! The mutex is shared across all threads via this global store, matching
//! Java's per-object monitor semantics.

use alloc::vec::Vec;
use freertos_rust::{MutexInnerImpl, MutexRecursive};
use pico_jvm::types::{JvmError, MonitorKey};

struct MonitorStoreCell(core::cell::UnsafeCell<Vec<(MonitorKey, MutexRecursive)>>);

// SAFETY: same single-core guarantee as SharedHeapCell in app.rs.  The Vec
// mutation (find-or-create) is a short critical region; the actual
// MutexRecursive::take/give calls are inherently thread-safe (FreeRTOS
// handles the synchronisation internally).
unsafe impl Sync for MonitorStoreCell {}

static MONITORS: MonitorStoreCell = MonitorStoreCell(core::cell::UnsafeCell::new(Vec::new()));

fn monitors() -> &'static mut Vec<(MonitorKey, MutexRecursive)> {
    unsafe { &mut *MONITORS.0.get() }
}

/// Acquire the monitor for `key`.  Creates a new recursive mutex on first use.
pub fn enter(key: MonitorKey) -> Result<(), JvmError> {
    let table = monitors();
    let pos = table.iter().position(|(k, _)| *k == key);
    let idx = match pos {
        Some(i) => i,
        None => {
            let mutex = MutexRecursive::create().map_err(|_| JvmError::StackOverflow)?;
            table.push((key, mutex));
            table.len() - 1
        }
    };
    table[idx]
        .1
        .take(freertos_rust::Duration::infinite())
        .map_err(|_| JvmError::IllegalMonitorState)
}

/// Release the monitor for `key`.
pub fn exit(key: MonitorKey) -> Result<(), JvmError> {
    let table = monitors();
    let pos = table.iter().position(|(k, _)| *k == key);
    match pos {
        Some(i) => {
            table[i].1.give();
            Ok(())
        }
        None => Err(JvmError::IllegalMonitorState),
    }
}

/// Drop all monitors.  Called on app reset before running a new APK.
pub fn clear() {
    monitors().clear();
}
