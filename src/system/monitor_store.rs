// SPDX-License-Identifier: GPL-3.0-only
//! Global monitor table shared across all JVM threads.
//!
//! Each Java `synchronized` block creates a FreeRTOS recursive mutex lazily.
//! The mutex is shared across all threads via this global store, matching
//! Java's per-object monitor semantics.
//!
//! On non-RP families (currently ESP32-S3) the JVM runs single-threaded so
//! monitors are no-ops — `synchronized` blocks are still entered/exited but
//! no actual mutex is created.

// ── RP family: real FreeRTOS recursive mutexes ──────────────────────────────
#[cfg(feature = "family-rp")]
mod rp_impl {
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

    pub fn clear() {
        monitors().clear();
    }
}

// ── Non-RP families: single-threaded no-op monitors ─────────────────────────
#[cfg(not(feature = "family-rp"))]
mod stub_impl {
    use pico_jvm::types::{JvmError, MonitorKey};

    pub fn enter(_key: MonitorKey) -> Result<(), JvmError> {
        Ok(())
    }
    pub fn exit(_key: MonitorKey) -> Result<(), JvmError> {
        Ok(())
    }
    pub fn clear() {}
}

// ── Public API (delegates to active impl) ────────────────────────────────────
#[cfg(feature = "family-rp")]
pub use rp_impl::{clear, enter, exit};
#[cfg(not(feature = "family-rp"))]
pub use stub_impl::{clear, enter, exit};
