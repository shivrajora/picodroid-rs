// SPDX-License-Identifier: GPL-3.0-only
// ESP32-S3 flash stub — Milestone 1.
// read_flash_papk returns Some(&[]) so the boot path doesn't panic at startup;
// the JVM will fail to parse the empty APK at runtime (expected for stub builds).
// Real flash-mapped PAPK reading is a later milestone.

pub const PAPK_MAX_DATA_SIZE: usize = 1020 * 1024;

pub unsafe fn read_flash_papk() -> Option<&'static [u8]> {
    static EMPTY: [u8; 0] = [];
    Some(&EMPTY)
}

// ── fs/storage.rs compatibility stubs ───────────────────────────────────────
// FS is always mounted in init() but no real flash is available in M1;
// operations return errors so littlefs falls back to its in-RAM state.
pub const FLASH_SECTOR_SIZE: usize = 4096;
pub const FLASH_PAGE_SIZE: usize = 256;

pub fn fs_region_bounds() -> (u32, u32) {
    (0, 0)
}

pub fn flash_read(_offset: u32, _buf: &mut [u8]) {}

pub unsafe fn flash_program_range(_offset: u32, _data_ptr: *const u8, _len: usize) {}

pub unsafe fn flash_erase_range(_offset: u32, _len: usize) {}
