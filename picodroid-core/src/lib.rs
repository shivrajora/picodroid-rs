// SPDX-License-Identifier: GPL-3.0-only
#![cfg_attr(not(any(test, feature = "sim")), no_std)]

extern crate alloc;

pub mod board_cfg;
pub mod dispatch_sites;
#[allow(dead_code)]
pub mod drivers;
pub mod executors;
pub mod framework_classes;
pub mod gc_root_registration;
pub mod gc_roots;
pub mod graphics;
pub mod hal;
pub mod host;
pub mod lvgl_ffi;
pub mod monitor_store;
// Board-gated: `has_network` comes from board.toml via this crate's build.rs.
#[cfg(all(not(test), has_network))]
pub mod net;
// `cfg(not(test))` for the same reason it carried that gate in the binary
// crate: these reach the JVM natives and the HAL, not pure logic.
#[cfg(not(test))]
pub mod os;
pub mod pd_log;
// Ungated: `peripheral_manager`'s ref-name parsing is pure logic and carries
// host unit tests.
pub mod pio;
pub mod rtos;
pub mod shrink_names;
pub mod task_priority;
#[cfg(not(test))]
pub mod util;

#[cfg(test)]
mod test_platform;
