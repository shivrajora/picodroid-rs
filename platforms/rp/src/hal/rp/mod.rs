// SPDX-License-Identifier: GPL-3.0-only
//! RP-family HAL (RP2040 + RP2350).
//!
//! Chip-level differences (clock speed, RP2350 ISO bit) are handled via
//! `#[cfg(feature = "chip-rp2040")]` / `#[cfg(feature = "chip-rp2350")]`
//! within each module.

pub mod adc;
pub mod boot;
pub mod clock;
pub mod core1_park;
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

// CYW43439 driver bindings and the WiFi bring-up task. Family code: the
// chip is a link driver, not a framework concern (docs/designs/network-seam-2026-09.md D6).
#[cfg(network_cyw43)]
pub mod cyw43;
#[cfg(network_cyw43)]
pub mod pio_spi;
// The family's entropy seam for the shared FreeRTOS+TCP glue
// (picodroid_port_entropy32), and the RP2350 TRNG behind it (NET-6). Both
// exist only in network builds.
#[cfg(has_network)]
pub mod entropy;
#[cfg(all(has_network, feature = "chip-rp2350"))]
pub mod trng;
#[cfg(network_cyw43)]
pub mod wifi_task;
