// SPDX-License-Identifier: GPL-3.0-only
//! ESP32-S3 family HAL — Milestone 1 stub.
//!
//! All peripherals are sim-grade no-ops. Real implementations land in
//! subsequent milestones as each peripheral driver is wired up to esp-hal.
// Peripheral structs, constants, and helpers are stubs not yet wired up (M2+).
#![allow(dead_code)]

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
