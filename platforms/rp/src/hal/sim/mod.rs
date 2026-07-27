// SPDX-License-Identifier: GPL-3.0-only
//! Simulator HAL for this family.
//!
//! Almost all of it now lives in [`picodroid_core::hal::sim`] and is
//! re-exported here, so `crate::hal::<module>` keeps resolving through the
//! `mod chip` dispatch in the parent regardless of which half a module
//! came from.
//!
//! What remains are the three stubs a shared simulator cannot supply,
//! because they stand in for machinery that is specific to this family:
//! reset/boot entry, XIP flash, and the USB debug bridge.

pub use picodroid_core::hal::sim::{
    adc, delay, display, gpio, i2c, input_pin, output_pin, pwm, spi, spi_bus, system_clock, touch,
    uart,
};

#[cfg(has_network)]
pub use picodroid_core::hal::sim::net;

pub mod boot;
pub mod flash;
pub mod pdb_usb;
