// SPDX-License-Identifier: GPL-3.0-only
//! This family's debug bridge.
//!
//! The protocol — resync, dispatch, framing, sysmon encoding, input
//! injection, install — is `picodroid_core::pdb`. What is left here is the
//! USB CDC pipe, the FreeRTOS statistics source, and the core-parking
//! handshake, which is this family's flash topology rather than anyone's
//! protocol.

#[cfg(not(any(test, feature = "sim")))]
mod coordinator;
#[cfg(not(any(test, feature = "sim")))]
pub mod pending;
#[cfg(not(any(test, feature = "sim")))]
mod platform;

#[cfg(not(any(test, feature = "sim")))]
pub fn run_pdb_task() -> ! {
    picodroid_core::pdb::run_pdb_task(
        platform::CdcTransport,
        coordinator::PdbCoreCoordinator,
        platform::FreeRtosSysmon,
        crate::packagemanager::RpPapkFlash,
    )
}
