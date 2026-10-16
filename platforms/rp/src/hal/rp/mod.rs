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
pub mod init_gate;
pub mod input_pin;
pub mod output_pin;
pub mod pdb_usb;
pub(crate) mod periph;
// The QSPI PSRAM on a module whose MCU toml declares one (`psram_kb`): only
// the RP2350B carriers today.
#[cfg(all(feature = "chip-rp2350", has_psram))]
pub mod psram;
pub mod pwm;
pub mod spi;
pub mod spi_bus;
pub mod system_clock;
pub mod touch;
pub mod uart;
#[cfg(feature = "chip-rp2350")]
pub mod xip;

// The CYW43439 link driver: bindings plus `cyw43::link::Cyw43Link`, which
// core's `run_link_task` drives. Family code: the chip is a link driver,
// not a framework concern (docs/designs/network-seam-2026-09.md D6).
#[cfg(network_cyw43)]
pub mod cyw43;
#[cfg(network_cyw43)]
pub mod pio_spi;
// The chip's serial (`PlatformHooks::device_serial`).
pub mod chip_id;
// The family's entropy seam: picodroid_port_entropy_bytes, which every build
// links (core's `hal::entropy`: the TLS seed, the serial fallback), and
// picodroid_port_entropy32 for the shared FreeRTOS+TCP glue. The RP2350
// TRNG behind them (NET-6) is network builds only; elsewhere they answer
// as the RP2040 does, with no hardware source.
pub mod entropy;
#[cfg(all(has_network, feature = "chip-rp2350"))]
pub mod trng;
