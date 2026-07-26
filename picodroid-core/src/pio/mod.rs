// SPDX-License-Identifier: GPL-3.0-only
#[cfg(not(test))]
pub mod adc;
pub mod fields;
#[cfg(not(test))]
pub mod gpio;
pub mod helpers;
#[cfg(not(test))]
pub mod i2c;
pub mod peripheral_manager;
#[cfg(not(test))]
pub mod pwm;
#[cfg(not(test))]
pub mod spi;
#[cfg(not(test))]
pub mod uart;
