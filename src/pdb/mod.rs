// SPDX-License-Identifier: GPL-3.0-only
#[cfg(not(any(test, feature = "sim")))]
mod cdc_transport;
#[cfg(not(any(test, feature = "sim")))]
pub mod pending;
// Protocol constants (status codes, INSTALL_PEEK_BYTES) are unconditionally
// available so non-pdb code (e.g. packagemanager::install) can reference
// them without a feature gate.
pub(crate) mod protocol;
#[cfg(not(any(test, feature = "sim")))]
pub mod sysmon;
#[cfg(not(any(test, feature = "sim")))]
mod task;

#[cfg(not(any(test, feature = "sim")))]
pub use task::run_pdb_task;
