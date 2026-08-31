// SPDX-License-Identifier: GPL-3.0-only
//! FreeRTOS task priority tiers.
//!
//! Layout (low → high, configMAX_PRIORITIES = 32):
//!   0          : FreeRTOS idle (reserved)
//!   1–10       : Background native services  (BG_1..BG_10)
//!   15         : Every task that interprets Java (`PRIORITY_JVM_NORM`):
//!                jvm_task, `Thread.start` children, the background
//!                executor pool
//!   21–30      : Real-time native tasks      (RT_1..RT_10)
//!   31         : FreeRTOS timer task         (configMAX_PRIORITIES - 1)
//!
//! # One tier for all Java
//!
//! The shared JVM heap is lock-free on the strength of "a running JVM task
//! keeps the core until it blocks": `configUSE_TIME_SLICING 0`, plus every
//! interpreting task at *one* priority. A Java thread one notch above the UI
//! task would preempt it at any instruction — mid-heap-mutation, mid-LVGL
//! call — and the `AtomicSection` guards cover only the compound heap
//! operations, not every store. So Android's `Thread.setPriority` is
//! advisory here (stored, reported by `getPriority`, never applied to the
//! task), and the 11–20 band this ladder once reserved for it is gone: the
//! band existed, the pool ran at 5, and either was enough to break the
//! contract. The alternative — a global interpreter lock — costs far more
//! than the fairness a priority band buys (docs/parity-audit.md THR-06).
//!
//! Only the rungs something actually stands on are declared below. The ladder
//! above is the map; adding `PRIORITY_BG_7 = 7` when a task needs it is a
//! one-line change, and declaring all thirty up front only bought a
//! file-wide `allow(dead_code)` that then hid real rot.

pub const PRIORITY_BG_6: u8 = 6;
/// Sensor sampler task (alias of BG_6): below every JVM tier so it can never
/// preempt the interpreter, one notch above the background executor pool so
/// long Java background jobs don't add sampling jitter.
pub const PRIORITY_SENSOR: u8 = PRIORITY_BG_6;

/// The one JVM tier. `build_support/board_cfg.rs` pins the background pool
/// to the same number; change both or neither.
pub const PRIORITY_JVM_NORM: u8 = 15;

pub const PRIORITY_RT_1: u8 = 21; // pdb task lives here
pub const PRIORITY_RT_2: u8 = 22; // cyw43 WiFi task lives here
pub const PRIORITY_FS_WORKER: u8 = 22; // fs worker task (alias of RT_2)

pub const PRIORITY_RT_10: u8 = 30;
/// Core-1 flash parker (alias of RT_10): top of the RT band so a park
/// request preempts anything schedulable on core 1; only the FreeRTOS timer
/// task (31) sits above, and it blocks again within microseconds.
pub const PRIORITY_FLASH_PARK: u8 = PRIORITY_RT_10;

#[cfg(test)]
mod tests {
    use super::*;

    /// The pool's generated priority must sit on the JVM tier — the
    /// build-script default and the assertion there are what keep a
    /// `board.toml` from putting Java workers on another rung.
    #[test]
    fn background_pool_runs_on_the_jvm_tier() {
        assert_eq!(
            crate::board_cfg::background_pool::POOL_PRIORITY,
            PRIORITY_JVM_NORM
        );
    }
}
