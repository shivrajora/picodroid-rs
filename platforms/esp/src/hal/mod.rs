// SPDX-License-Identifier: GPL-3.0-only
//! Hardware Abstraction Layer — ESP family + sim dispatcher.
//!
//! A single `#[cfg]` dispatch mirrors `platforms/rp/src/hal/mod.rs`:
//! `sim` feature → host no-op stubs; `family-esp` → ESP32-S3 implementation.
//! The HAL CONTRACT v1 (compile-time assertions) is enforced by `contract`.

// In sim mode OR test mode, use the host no-op stubs so `cargo test` and
// `cargo test --features sim` compile on the build host without Xtensa toolchain.
#[cfg(any(feature = "sim", test))]
#[path = "sim/mod.rs"]
mod chip;

#[cfg(all(not(any(feature = "sim", test)), feature = "family-esp"))]
#[path = "esp/mod.rs"]
mod chip;

pub use chip::adc;
pub use chip::boot;
pub use chip::delay;
pub use chip::display;
pub use chip::flash;
pub use chip::gpio;
pub use chip::i2c;
pub use chip::input_pin;
pub use chip::output_pin;
pub use chip::pdb_usb;
pub use chip::pwm;
pub use chip::spi;
pub use chip::spi_bus;
pub use chip::system_clock;
pub use chip::touch;
pub use chip::uart;

// Compile-time HAL CONTRACT v1 enforcement.
mod contract;
