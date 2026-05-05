// SPDX-License-Identifier: GPL-3.0-only
// Simulator stubs — flash operations are not available in sim mode.
// These constants and functions exist only for module completeness;
// they are never called because packagemanager is gated by
// #[cfg(not(any(test, feature = "sim")))].

#[allow(dead_code)]
pub const PAPK_FLASH_MAGIC: u32 = 0x5044_4231;
#[allow(dead_code)]
pub const PAPK_MAX_DATA_SIZE: usize = 1020 * 1024;
