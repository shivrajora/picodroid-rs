// SPDX-License-Identifier: GPL-3.0-only

// Executors and the monitor table now live in picodroid-core, behind the
// RTOS seam. Re-exported at their original paths so existing
// `crate::system::executors::…` / `crate::system::monitor_store::…` call
// sites resolve unchanged — the established pattern for a module that has
// crossed into the shared crate (see main.rs's picodroid_core re-exports).
pub use picodroid_core::executors;
// Every call site is cfg(not(sim))-gated — the native handler only overrides
// monitor_enter/exit off-simulator — so the re-export carries the same gate.
// Wiring monitors up in the simulator is now a one-line change (the seam
// gives every platform a recursive mutex), but it is a behaviour change, not
// a move, so it is not part of this extraction.
#[cfg(not(feature = "sim"))]
pub use picodroid_core::monitor_store;

#[cfg(all(feature = "mem-diag", not(test)))]
pub mod mem_diag;
#[cfg(not(test))]
pub mod native_handler;
#[cfg(not(test))]
pub mod notification;
pub mod picodroid;
