// SPDX-License-Identifier: GPL-3.0-only
//! ESP simulator HAL — no-op stubs for host builds (cargo test / --features sim).

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
