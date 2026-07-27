// SPDX-License-Identifier: GPL-3.0-only
//! PAPK install orchestration — the three-phase sequence that replaces the
//! app on a device.
//!
//! It lives here rather than in a family crate because none of it is about
//! silicon: it is a wire protocol, a compatibility gate, a CRC and an
//! ordering discipline. What *is* family-specific — where the flash slot
//! sits, how to erase it, how to park the core that executes from it —
//! arrives through [`PapkFlash`] and [`install::CoreCoordinator`].
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
//! Phase A validates and *then* erases: an incompatible or oversized PAPK
//! must be refused while the installed one is still intact. Everything after
//! the erase runs with the JVM core parked, because on a family that executes
//! from the flash being erased, it cannot be otherwise. The tests in
//! [`orchestrator`] pin both.

mod orchestrator;
pub mod transport;

pub use orchestrator::{run_install, CoreCoordinator, PapkFlash};
pub use transport::{InstallError, InstallTransport, ReadError};
