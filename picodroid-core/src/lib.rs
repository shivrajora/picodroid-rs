// SPDX-License-Identifier: GPL-3.0-only
#![cfg_attr(not(any(test, feature = "sim")), no_std)]

extern crate alloc;

pub mod board_cfg;
pub mod dispatch_sites;
#[allow(dead_code)]
pub mod drivers;
pub mod framework_classes;
pub mod hal;
pub mod host;
pub mod lvgl_ffi;
pub mod pd_log;
pub mod rtos;
pub mod shrink_names;
pub mod task_priority;

#[cfg(test)]
mod test_platform;
