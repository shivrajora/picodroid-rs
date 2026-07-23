// SPDX-License-Identifier: GPL-3.0-only
pub const FRAME_MAGIC: &[u8; 4] = b"PDBP";

pub const CMD_PING: u8 = 0x00;
pub const CMD_INSTALL: u8 = 0x01;
pub const CMD_SYSMON: u8 = 0x02;
/// Inject a synthetic input event (button/D-pad key, or touch tap/swipe) so a
/// host — including an AI agent — can drive a real device the way `adb shell
/// input …` drives Android. The device turns the verb into HAL-level input
/// (GPIO edge or touch sample), so the whole on-device pipeline runs unchanged.
pub const CMD_INPUT: u8 = 0x03;

// ── CMD_INPUT payload subtypes (first payload byte) ──────────────────────────
/// KEY: `[keycode: i32 LE][meta: u8]`, meta 0=down+up, 1=down-only, 2=up-only.
pub const INPUT_KEY: u8 = 0x01;
/// TAP: `[x: i32 LE][y: i32 LE]`.
pub const INPUT_TAP: u8 = 0x02;
/// SWIPE: `[x1 i32][y1 i32][x2 i32][y2 i32][duration_ms: u32 LE]` (all LE).
pub const INPUT_SWIPE: u8 = 0x03;

/// CMD_INPUT KEY `meta` values.
pub const KEY_META_DOWN_UP: u8 = 0;
pub const KEY_META_DOWN: u8 = 1;
pub const KEY_META_UP: u8 = 2;

pub const STATUS_OK: u8 = 0x00;
pub const STATUS_READY: u8 = 0x01; // device has erased flash and is ready to receive data
pub const STATUS_ERR: u8 = 0xFF;
pub const STATUS_TOO_LARGE: u8 = 0xFE;
pub const STATUS_CRC_FAIL: u8 = 0xFD;
/// Device refused the install: PAPK's framework-map-version is incompatible
/// with this firmware (asymmetric shrunk/unshrunk, or PAPK > firmware).
/// Returned in install Phase A *before* any flash erase, so the existing
/// PAPK on the device is unaffected.
pub const STATUS_INCOMPAT: u8 = 0xFC;

/// Number of PAPK bytes the host inlines immediately after the install
/// header (Phase A) so the device can run a pre-erase compat check
/// against the PAPK manifest without blocking. Sized comfortably above
/// the 24-byte file header + 16-byte section header + a typical manifest.
pub const INSTALL_PEEK_BYTES: usize = 512;

/// CRC32 over [cmd byte][4-byte len LE][payload] — protects all variable fields.
pub fn crc32_frame(cmd: u8, len: u32, payload: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(&[cmd]);
    h.update(&len.to_le_bytes());
    h.update(payload);
    h.finalize()
}
