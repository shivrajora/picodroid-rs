// SPDX-License-Identifier: GPL-3.0-only
//! Simulator HAL — runs on the host for testing without hardware.

pub mod adc;
pub mod boot;
pub mod delay;
pub mod display;
pub mod flash;
pub mod gpio;
pub mod i2c;
pub mod input_pin;
pub mod output_pin;
pub mod pdb_usb;
pub mod pwm;
pub mod spi;
pub mod spi_bus;
pub mod system_clock;
pub mod touch;
pub mod uart;

#[cfg(has_network)]
pub mod net;
