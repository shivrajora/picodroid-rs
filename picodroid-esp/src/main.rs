// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

extern crate alloc;

mod app;
mod boards;
mod hal;

use embedded_alloc::Heap;
use xtensa_lx_rt::entry;

#[global_allocator]
static GLOBAL: Heap = Heap::empty();

// Static backing store for the ESP heap allocator. 256 KiB is intentionally
// conservative for the stub build; tune when FreeRTOS/PSRAM land (Milestone 3).
static mut ESP_HEAP: core::mem::MaybeUninit<[u8; 256 * 1024]> = core::mem::MaybeUninit::uninit();

#[entry]
fn main() -> ! {
    // Initialize heap allocator before any alloc usage.
    unsafe { GLOBAL.init(ESP_HEAP.as_mut_ptr() as usize, 256 * 1024) }

    hal::boot::clock_init();

    let boot_apk: &'static [u8] =
        unsafe { hal::flash::read_flash_papk() }.unwrap_or(&[]);

    hal::boot::start_tasks(boot_apk)
}
