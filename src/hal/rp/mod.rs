//! RP-family HAL (RP2040 + RP2350).
//!
//! Chip-level differences (clock speed, RP2350 ISO bit) are handled via
//! `#[cfg(feature = "chip-rp2040")]` / `#[cfg(feature = "chip-rp2350")]`
//! within each module.

pub mod adc;
pub mod boot;
pub mod flash;
pub mod gpio;
pub mod i2c;
pub mod pdb_uart;
pub mod pwm;
pub mod spi;
pub mod system_clock;
pub mod uart;
