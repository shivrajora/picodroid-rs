// SPDX-License-Identifier: GPL-3.0-only
#![cfg_attr(not(any(test, feature = "sim")), no_std)]

extern crate alloc;

pub mod dispatch_sites;
#[allow(dead_code)]
pub mod drivers;
pub mod framework_classes;
#[cfg(not(test))]
pub mod lvgl_ffi;
pub mod shrink_names;
