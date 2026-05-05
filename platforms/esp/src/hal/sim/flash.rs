// SPDX-License-Identifier: GPL-3.0-only
// flash is hardware-only; these stubs exist only for module completeness.
#[allow(dead_code)]
pub const PAPK_MAX_DATA_SIZE: usize = 1020 * 1024;
#[allow(dead_code)]
pub unsafe fn read_flash_papk() -> Option<&'static [u8]> {
    None
}
