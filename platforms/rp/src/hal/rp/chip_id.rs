// SPDX-License-Identifier: GPL-3.0-only
//! This family's answer to `PlatformHooks::device_serial`
//! (docs/designs/device-serial-2026-10.md §2.2).
//!
//! - RP2350: the factory chip ID in OTP, through the bootrom's
//!   `get_sys_info(CHIP_INFO)` — a plain ROM call, XIP stays on. The same
//!   value as its BOOTSEL USB serial, so `lsusb` reads the same in both
//!   modes.
//! - RP2040: the QSPI flash's unique ID (`flash::flash_unique_id`), read
//!   with XIP off, which is why `identity::init` runs before core 1 starts.
//!   The Pico SDK's `pico_unique_board_id`, as an RP2040 Debug Probe's
//!   serial shows it; the RP2040 bootrom's BOOTSEL serial is one fixed
//!   string on every chip (`E0C9125B0D9B`), so it cannot be matched.

use picodroid_core::identity::Serial;

/// The wafer ID word, then the device ID word, each most significant byte
/// first: the order the bootrom's BOOTSEL descriptor prints them in
/// (checked on the bench 2026-10-14: BOOTSEL `68B1A1217192254D`, device ID
/// `0x7192254D`, wafer ID `0x68B1A121`).
#[cfg(feature = "chip-rp2350")]
pub fn device_serial() -> Option<Serial> {
    let info = rp235x_hal::rom_data::sys_info_api::chip_info()
        .ok()
        .flatten()?;
    let mut id = [0u8; 8];
    id[..4].copy_from_slice(&info.wafer_id.to_be_bytes());
    id[4..].copy_from_slice(&info.device_id.to_be_bytes());
    Serial::from_id_bytes(&id)
}

#[cfg(feature = "chip-rp2040")]
pub fn device_serial() -> Option<Serial> {
    // SAFETY: `identity::init`'s contract — boot calls it before the
    // scheduler starts, so no other flash operation can be in flight.
    let id = unsafe { super::flash::flash_unique_id() };
    Serial::from_id_bytes(&id)
}
