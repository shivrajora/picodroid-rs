//! Hardware Abstraction Layer — centralises all chip-specific code.
//!
//! A single `#[cfg]` dispatch selects the chip family module.  Each family
//! (rp, nrf52, …) provides the same set of public free functions so that the
//! rest of the crate can call `hal::uart::init()` etc. without knowing which
//! chip is underneath.

// In sim mode OR test mode, use the simulator stubs.
// (Tests run on the host where HAL crates like rp-pico are unavailable.)
#[cfg(any(feature = "sim", test))]
#[path = "sim/mod.rs"]
mod chip;

#[cfg(all(not(any(feature = "sim", test)), feature = "family-rp"))]
#[path = "rp/mod.rs"]
mod chip;

// Peripheral drivers
#[allow(unused_imports)]
pub use chip::adc;
#[allow(unused_imports)]
pub use chip::delay;
#[allow(unused_imports)]
pub use chip::display;
#[allow(unused_imports)]
pub use chip::gpio;
#[allow(unused_imports)]
pub use chip::i2c;
#[allow(unused_imports)]
pub use chip::input_pin;
#[allow(unused_imports)]
pub use chip::output_pin;
#[allow(unused_imports)]
pub use chip::pwm;
#[allow(unused_imports)]
pub use chip::spi;
#[allow(unused_imports)]
pub use chip::spi_bus;
#[allow(unused_imports)]
pub use chip::system_clock;
#[allow(unused_imports)]
pub use chip::touch;
#[allow(unused_imports)]
pub use chip::uart;

// Boot & flash (only meaningful on real hardware, but sim provides stubs
// for module completeness — suppress unused warnings in sim/test builds)
#[allow(unused_imports)]
pub use chip::boot;
#[allow(unused_imports)]
pub use chip::flash;
#[allow(unused_imports)]
pub use chip::pdb_usb;

#[cfg(feature = "has-network")]
#[allow(unused_imports)]
pub use chip::net;
