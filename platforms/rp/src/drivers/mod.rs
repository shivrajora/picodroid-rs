// SPDX-License-Identifier: GPL-3.0-only
//! Chip-agnostic device drivers, generic over `embedded-hal` traits.

pub mod st7789;
pub mod xpt2046;

#[cfg(network_cyw43)]
pub mod cyw43;

#[cfg(any(sensor_bme688, test))]
pub mod bme688;

#[cfg(any(sensor_ltr559, test))]
pub mod ltr559;

/// Extension trait for SPI buses that support runtime frequency switching.
///
/// `embedded_hal::spi::SpiBus` does not include reconfiguration, but shared
/// buses (e.g. display + touch on one SPI peripheral) need it.
pub trait SpiFreqSwitch {
    fn set_frequency(&mut self, freq_hz: u32);
}
