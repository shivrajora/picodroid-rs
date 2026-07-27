// SPDX-License-Identifier: GPL-3.0-only
//! Hardware Abstraction Layer for this family — every chip-specific symbol
//! lives under here, behind one `#[cfg]` that selects `rp/` or `sim/`.
//!
//! # Where the contract lives
//!
//! It is not written here any more. `picodroid_core::hal`'s traits are the
//! contract (HAL CONTRACT v2), and `glue.rs` implements them by delegating
//! to the modules below — so a signature that drifts fails to compile at the
//! impl. What used to be a 117-line doc-block here, hand-kept in sync with a
//! matching list of assertions in [`contract`], is now one machine-checked
//! definition.
//!
//! That pairing had in fact fallen out of sync: converting it to traits
//! turned up `net::udp_sendto`/`udp_recvfrom` and `i2c::{write,read}` /
//! `spi::{transfer,write}` / `uart::reconfigure` all in live use by the
//! natives and named in neither half.
//!
//! [`contract`] still asserts `boot`, `flash` and `pdb_usb`: no trait covers
//! those, because they have no shared counterpart to be a contract with.
//!
//! # Family-private wiring
//!
//! `delay`, `input_pin`, `output_pin` and `spi_bus` are concrete
//! `embedded_hal` implementations used to wire `picodroid_core::drivers`
//! generic drivers to this family's peripherals. They are not part of any
//! cross-crate contract — name and shape are ours to change.

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

#[cfg(has_network)]
#[allow(unused_imports)]
pub use chip::net;

// Compile-time HAL CONTRACT v1 enforcement. Never executed; type-checked only.
mod contract;

// Display geometry is generated twice from one board.toml: this family's
// pin-bearing `display_config.rs`, and picodroid-core's neutral
// `display_dims.rs` (which shared code sizes its band buffer from). Both come
// from `build_support/board_cfg.rs`, but they land in different OUT_DIRs, so
// assert they agree — a mismatch would mean shared code and the HAL disagree
// about the framebuffer, which is a silent corruption, not a build error.
// See docs/designs/shared-core-extraction.md §3.D.
const _: () = {
    use picodroid_core::board_cfg::display as shared;
    assert!(display::WIDTH == shared::SCREEN_WIDTH);
    assert!(display::HEIGHT == shared::SCREEN_HEIGHT);
    assert!(display::BAND_HEIGHT == shared::BAND_HEIGHT);
    assert!(display::SCROLL_LIMIT == shared::SCROLL_LIMIT);
};
