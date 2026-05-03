// SPDX-License-Identifier: GPL-3.0-only
//! Hardware Abstraction Layer — ESP-only dispatcher.
//!
//! picodroid-esp only ever targets Xtensa ESP32-S3; there is no RP or sim
//! path in this workspace.  The HAL contract (compile-time assertions) is
//! enforced by the `contract` submodule, mirroring the pattern in the root
//! picodroid workspace.

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
