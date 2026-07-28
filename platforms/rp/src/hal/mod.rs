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
//! [`contract`] still asserts `boot` and `flash`: no trait covers those,
//! because they have no shared counterpart to be a contract with. (`pdb_usb`
//! graduated to `picodroid_core::pdb::PdbTransport` in stage 3d.) Both are
//! re-exported below under a device-only `cfg`, matching `contract`'s own
//! gate — the simulator has no reset vector and no XIP flash region.
//!
//! # Family-private wiring
//!
//! `delay`, `input_pin`, `output_pin` and `spi_bus` are concrete
//! `embedded_hal` implementations used to wire `picodroid_core::drivers`
//! generic drivers to this family's peripherals. They are not part of any
//! cross-crate contract — name and shape are ours to change.

// In sim mode OR test mode, use the shared simulator.
// (Tests run on the host where HAL crates like rp-pico are unavailable.)
//
// There is no `sim/` directory here any more. Every module it held is
// picodroid-core's, including the `pdb_usb` stub that used to stay behind.
// The `boot`/`flash` stubs that went with it are gone entirely: one empty
// function and two constants that no simulator build could reach.
#[cfg(any(feature = "sim", test))]
use picodroid_core::hal::sim as chip;

#[cfg(all(not(any(feature = "sim", test)), feature = "family-rp"))]
#[path = "rp/mod.rs"]
mod chip;

// Peripheral drivers
pub use chip::adc;
#[cfg_attr(any(feature = "sim", test), allow(unused_imports))]
pub use chip::delay;
pub use chip::display;
pub use chip::gpio;
pub use chip::i2c;
#[cfg_attr(any(feature = "sim", test, not(has_touch)), allow(unused_imports))]
pub use chip::input_pin;
#[cfg_attr(any(feature = "sim", test), allow(unused_imports))]
pub use chip::output_pin;
pub use chip::pwm;
pub use chip::spi;
#[cfg_attr(any(feature = "sim", test), allow(unused_imports))]
pub use chip::spi_bus;
pub use chip::system_clock;
pub use chip::touch;
pub use chip::uart;

// Boot & flash: device-only. The shared simulator has no counterpart — a
// reset vector and an XIP flash region are not things a host process has —
// and the empty stubs that used to stand in for them were reachable from
// nothing, so they are gone. Both consumers (`main`'s `#[entry]` and
// `packagemanager`) carry this same gate.
#[cfg_attr(any(feature = "sim", test), allow(unused_imports))]
pub use chip::pdb_usb;
#[cfg(all(not(any(feature = "sim", test)), feature = "family-rp"))]
pub use chip::{boot, flash};

#[cfg(has_network)]
pub use chip::net;

// The cyw43 bring-up task. Device-only: the shared simulator has no such
// module, and nothing in a simulator build would drive it. Exposed here
// rather than reached as `hal::rp::…` because `chip` is private — the same
// indirection every other peripheral goes through.
#[cfg(all(network_cyw43, not(any(test, feature = "sim"))))]
pub use chip::wifi_task;

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
