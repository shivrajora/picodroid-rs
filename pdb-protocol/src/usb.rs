// SPDX-License-Identifier: GPL-3.0-only
//! The debug bridge's USB identity.
//!
//! A host finds a picodroid device by vendor and product ID before it sends a
//! single PDBP byte, so these two numbers are part of the wire contract in
//! the same way the frame magic is. They lived in two places — the device's
//! CDC descriptors and the host tool's port scan — and a change to one would
//! have failed as "no picodroid devices found" rather than loudly. Now both
//! ends read them here.
//!
//! The descriptor *tables* built from these (device, configuration, strings)
//! are not here: their endpoint layout depends on the family's USB stack, so
//! the reference set lives in `picodroid_core::pdb::usb_cdc` and a family
//! with different endpoint constraints builds its own from these constants.

/// Vendor ID: pid.codes, the open-source allocation.
pub const VID: u16 = 0x1209;
/// Product ID: picodroid's allocation under that vendor.
pub const PID: u16 = 0xCDC0;

/// iManufacturer / iProduct string. ASCII, so it can be encoded into a UTF-16LE
/// string descriptor at compile time.
pub const MANUFACTURER: &str = "Picodroid";
/// iInterface string for the CDC interface.
pub const INTERFACE: &str = "PDB (USB CDC)";
