//! Chip-agnostic device drivers, generic over `embedded-hal` traits.

pub mod st7789;
pub mod xpt2046;

#[cfg(feature = "net-cyw43")]
pub mod cyw43;

/// Extension trait for SPI buses that support runtime frequency switching.
///
/// `embedded_hal::spi::SpiBus` does not include reconfiguration, but shared
/// buses (e.g. display + touch on one SPI peripheral) need it.
pub trait SpiFreqSwitch {
    fn set_frequency(&mut self, freq_hz: u32);
}
