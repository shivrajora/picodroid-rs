// SPDX-License-Identifier: GPL-3.0-only
#![cfg_attr(not(any(test, feature = "sim")), no_std)]
#![cfg_attr(not(any(test, feature = "sim")), no_main)]

extern crate alloc;

mod app;
mod boards;
mod hal;

// Xtensa-specific runtime and allocator — only available on the ESP32-S3 target.
// Use esp-hal's full runtime: its `#[esp_hal::main]` attribute hooks Reset,
// configures CPU caches, sets up interrupts, and places the app descriptor.
#[cfg(not(any(test, feature = "sim")))]
use embedded_alloc::Heap;
#[cfg(not(any(test, feature = "sim")))]
use esp_backtrace as _;

// ESP-IDF app descriptor — espflash refuses to flash esp-hal images without it.
#[cfg(not(any(test, feature = "sim")))]
esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(not(any(test, feature = "sim")))]
#[global_allocator]
static GLOBAL: Heap = Heap::empty();

// Static backing store for the ESP heap allocator. 256 KiB is intentionally
// conservative for the stub build; tune when FreeRTOS/PSRAM land (Milestone 3).
#[cfg(not(any(test, feature = "sim")))]
static mut ESP_HEAP: core::mem::MaybeUninit<[u8; 256 * 1024]> = core::mem::MaybeUninit::uninit();

#[cfg(not(any(test, feature = "sim")))]
#[esp_hal::main]
fn main() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());

    unsafe { GLOBAL.init(core::ptr::addr_of_mut!(ESP_HEAP) as usize, 256 * 1024) }

    hal::boot::clock_init();

    let boot_apk: &'static [u8] = unsafe { hal::flash::read_flash_papk() }.unwrap_or(&[]);

    hal::boot::start_tasks(boot_apk)
}
