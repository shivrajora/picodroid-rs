// SPDX-License-Identifier: GPL-3.0-only
//! PAPK install orchestration — the sequence that puts an app onto a device.
//!
//! It lives here rather than in a family crate because none of it is about
//! silicon: it is a wire protocol, a compatibility gate, a placement policy,
//! a CRC and an ordering discipline. What *is* family-specific — where the
//! app region sits, how to erase and program it, how to park the core that
//! executes from it — arrives through [`PapkFlash`] and [`CoreCoordinator`].
//!
//! # Why the seams are generic parameters
//!
//! Unlike the HAL, this does not register `__pd_*` shims. There is exactly
//! one caller (the family's debug-bridge task), no shared statics needing
//! family types, and a family may reasonably ship no installer at all. A
//! generic parameter costs no link surface, needs no cfg-gating for boards
//! without a transport, and monomorphises in the family crate — where the
//! single caller already is. It also makes the whole path testable with
//! mocks, which matters more than usual here: until this move, not one line
//! of it was ever compiled on a host.
//!
//! # Ordering is the correctness property
//!
//! Phase A validates, places, and *then* erases: an incompatible, oversized
//! or homeless PAPK must be refused while every installed one is still
//! intact. Everything after the erase runs with the JVM core parked, because
//! on a family that executes from the flash being erased, it cannot be
//! otherwise. A run's boot-meta pages are written last, so a run is either
//! whole or invisible. The tests in [`orchestrator`] and
//! [`crate::packages`] pin all of it.

#[cfg(any(test, feature = "sim"))]
pub mod mem_region;
mod orchestrator;
pub mod region;
pub mod transport;

pub use orchestrator::{
    install, run_install, run_uninstall, uninstall, CoreCoordinator, PapkFlash,
};
pub use region::{PapkRegion, PapkRegionFlash, PAGES_PER_SECTOR};
pub use transport::{InstallError, InstallTransport, ReadError};
