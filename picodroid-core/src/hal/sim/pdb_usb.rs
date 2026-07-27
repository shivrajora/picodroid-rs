// SPDX-License-Identifier: GPL-3.0-only
//! Simulator debug-bridge stub.
//!
//! Deliberately empty: the simulator has no PDB at all. The `pdb` tree is
//! gated out of simulator and test builds, so there is nothing for a stub to
//! stand in for — the module exists only so `hal::pdb_usb` resolves.
//!
//! A consequence worth knowing: no line of the PDB or install code is
//! compiled on a host today, so none of it is covered by the test suite.
//! `docs/designs/family-neutral-residue.md` stage 3 is what changes that.
