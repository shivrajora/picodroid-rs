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
