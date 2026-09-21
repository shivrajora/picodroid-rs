// SPDX-License-Identifier: GPL-3.0-only
//! Chip-agnostic device drivers, generic over `embedded-hal` traits:
//! ST7789 / ST7796 panels, XPT2046 and GT911 touch, BME688 and LTR559
//! sensors. No HAL, RTOS or allocator dependency.

#![cfg_attr(not(test), no_std)]

// Every driver is always compiled: each is generic over its bus, so a board
// instantiates -- and links -- only the controllers it names, and the unit
// tests below run on every `cargo test`, board or not.
pub mod bme688;
pub mod gt911;
pub mod ltr559;
pub mod st7789;
pub mod st7796;
pub mod xpt2046;

/// Minimal blocking I2C bus the I2C drivers are generic over. Both methods
/// return a negative value on a bus error, which is the shape of picodroid's
/// `HalI2c::write_slice` / `read_slice`; an `embedded-hal` bus adapts in four
/// lines. One trait for every driver, so one bus type serves them all.
pub trait I2cBus {
    fn write(&mut self, addr: u8, data: &[u8]) -> i32;
    fn read(&mut self, addr: u8, buf: &mut [u8]) -> i32;
}

/// Extension trait for SPI buses that support runtime frequency switching.
///
/// `embedded_hal::spi::SpiBus` does not include reconfiguration, but shared
/// buses (e.g. display + touch on one SPI peripheral) need it.
pub trait SpiFreqSwitch {
    fn set_frequency(&mut self, freq_hz: u32);
}

/// Extension trait for SPI buses that can run a write in the background.
///
/// `embedded_hal::spi::SpiBus::write` returns when the bytes are out. A panel
/// driver flushing LVGL bands wants the other shape: start the DMA, go and
/// render the next band, collect the completion later. The contract mirrors a
/// borrow the type system cannot express: `data` must stay alive and
/// unchanged from `start_write` until `wait_write` returns, and a bus with
/// nothing in flight returns from `wait_write` at once.
pub trait SpiAsyncWrite {
    fn start_write(&mut self, data: &[u8]);
    fn wait_write(&mut self);
}
