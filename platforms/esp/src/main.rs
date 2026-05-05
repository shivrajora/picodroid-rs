// SPDX-License-Identifier: GPL-3.0-only
#![cfg_attr(not(any(test, feature = "sim")), no_std)]
#![cfg_attr(not(any(test, feature = "sim")), no_main)]

extern crate alloc;

mod app;
mod boards;
mod hal;

// Xtensa-specific runtime and allocator — only available on the ESP32-S3 target.
#[cfg(not(any(test, feature = "sim")))]
use embedded_alloc::Heap;
#[cfg(not(any(test, feature = "sim")))]
use esp_backtrace as _;
#[cfg(not(any(test, feature = "sim")))]
use xtensa_lx_rt::entry;

#[cfg(not(any(test, feature = "sim")))]
#[global_allocator]
static GLOBAL: Heap = Heap::empty();

// Static backing store for the ESP heap allocator. 256 KiB is intentionally
// conservative for the stub build; tune when FreeRTOS/PSRAM land (Milestone 3).
#[cfg(not(any(test, feature = "sim")))]
static mut ESP_HEAP: core::mem::MaybeUninit<[u8; 256 * 1024]> = core::mem::MaybeUninit::uninit();

#[cfg(not(any(test, feature = "sim")))]
#[entry]
fn main() -> ! {
    // Initialize heap allocator before any alloc usage.
    unsafe { GLOBAL.init(ESP_HEAP.as_mut_ptr() as usize, 256 * 1024) }

    hal::boot::clock_init();

    let boot_apk: &'static [u8] = unsafe { hal::flash::read_flash_papk() }.unwrap_or(&[]);

    hal::boot::start_tasks(boot_apk)
}
