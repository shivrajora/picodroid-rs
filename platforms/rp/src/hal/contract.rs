// SPDX-License-Identifier: GPL-3.0-only
//! Compile-time assertions for the parts of the HAL that no trait covers.
//!
//! What remains here is `boot` and `flash` — family-specific machinery with
//! no shared counterpart, so nothing else checks their shape. The function
//! below is dead code: it exists only to make the compiler verify those
//! symbols and signatures are present.
//!
//! # Why the rest is gone
//!
//! Everything else the HAL exposes is now bound by a
//! [`picodroid_core::hal`] trait, and `glue.rs` implements them by
//! delegating to `crate::hal::*` — so a drifted signature fails to compile at
//! the impl, which is a stronger and closer check than an assertion here.
//!
//! Keeping both was not free. These assertions were hand-maintained beside a
//! hand-written doc-block, and the conversion to traits found two places
//! where they had silently fallen behind the surface actually in use:
//! `net::udp_sendto` / `udp_recvfrom` (used by the net natives, never
//! asserted), and `i2c::{write,read}` / `spi::{transfer,write}` /
//! `uart::reconfigure` (used by the pio natives, never asserted). A trait
//! bound cannot fall behind that way.
//!
//! Two things the retired assertions covered are checked elsewhere rather
//! than dropped: the `display::WIDTH`/`HEIGHT`/`BAND_HEIGHT`/`SCROLL_LIMIT`
//! constants by the drift assertion in [`super`], and the family `gpio`
//! enums by `glue.rs`'s conversion functions.
//!
//! `pdb_usb` left for the same reason as the rest: its six functions are now
//! exactly the `picodroid_core::pdb::PdbTransport` surface, and
//! `pdb/platform.rs` implements it. The trait is the stronger check — it also
//! pins the *semantics* the assertions could not, such as `read_byte_timeout`
//! being the one that may return `None`.

#![allow(dead_code, unused_imports, clippy::let_unit_value)]

#[cfg(not(any(test, feature = "sim")))]
fn _assert_hardware_only() {
    use super::{boot, flash};

    // boot
    let _: fn() = boot::clock_init;
    let _: fn(&'static [u8]) -> ! = boot::start_tasks;

    // flash
    let _: usize = flash::PAPK_MAX_DATA_SIZE;
    let _: unsafe fn() -> Option<&'static [u8]> = flash::read_flash_papk;
}
