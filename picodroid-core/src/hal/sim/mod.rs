// SPDX-License-Identifier: GPL-3.0-only
//! Simulator HAL — the host implementation of the hardware surface.
//!
//! This lives in `picodroid-core`, not in a platform crate, and that is the
//! point. Every family used to be told to copy `hal/sim/` into its own tree;
//! the ESP scaffold did exactly that and accumulated 17 stub modules that
//! drifted from the originals before it was removed. A simulator that is
//! shared cannot drift.
//!
//! `pdb_usb` is a stub for machinery no simulator can stand in for (the USB
//! debug bridge). There are deliberately no `boot` / `flash` stubs beside
//! it: an empty `clock_init` and two flash constants that no simulator build
//! could reach were shape parity and nothing more, so `crate::hal`'s
//! re-exports of those two are gated to device builds instead. A family
//! whose simulator models a real flash region defines its own module.
//!
//! Sibling modules here call each other directly (`super::gpio::inject`)
//! rather than through [`crate::hal`]'s facade: the facade would route back
//! out through the platform's registration and straight into these same
//! functions, which is a round-trip that buys nothing.

pub mod adc;
pub mod allocator;
pub mod delay;
pub mod display;
pub mod gpio;
pub mod heap4;
pub mod i2c;
pub mod input_pin;
pub mod output_pin;
pub mod pdb_usb;
pub mod platform;
pub mod pwm;
pub mod rtos;
pub mod spi;
pub mod spi_bus;
pub mod system_clock;
pub mod touch;
pub mod uart;

#[cfg(has_network)]
pub mod net;
