//! RP-family HAL (RP2040 + RP2350).
//!
//! Chip-level differences (clock speed, RP2350 ISO bit) are handled via
//! `#[cfg(feature = "chip-rp2040")]` / `#[cfg(feature = "chip-rp2350-hal")]`
//! within each module.

pub mod adc;
pub mod boot;
pub mod clock;
pub mod delay;
pub mod display;
pub mod dma;
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

#[cfg(feature = "has-network")]
pub mod net;
#[cfg(feature = "net-cyw43")]
pub mod wifi_task;
