// SPDX-License-Identifier: GPL-3.0-only
//! Chip-agnostic device drivers, generic over `embedded-hal` traits.

// Panel and touch controllers are gated on the `display_<driver>` /
// `touch_<driver>` cfgs build.rs emits from board.toml's `driver` keys, so a
// board links only the controllers it has. `test` builds them all: their unit
// tests are host tests and must run on every `cargo test`, board or not.
#[cfg(any(touch_gt911, test))]
pub mod gt911;
#[cfg(any(display_st7789, test))]
pub mod st7789;
#[cfg(any(display_st7796, test))]
pub mod st7796;
#[cfg(any(touch_xpt2046, test))]
pub mod xpt2046;

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
