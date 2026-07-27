// SPDX-License-Identifier: GPL-3.0-only
#[cfg(not(any(test, feature = "sim")))]
mod cdc_transport;
#[cfg(not(any(test, feature = "sim")))]
mod input;
#[cfg(not(any(test, feature = "sim")))]
pub mod pending;
#[cfg(not(any(test, feature = "sim")))]
pub mod sysmon;
#[cfg(not(any(test, feature = "sim")))]
mod task;

#[cfg(not(any(test, feature = "sim")))]
pub use task::run_pdb_task;
