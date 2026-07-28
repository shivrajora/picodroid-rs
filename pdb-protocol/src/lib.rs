// SPDX-License-Identifier: GPL-3.0-only
//! PDBP — the Picodroid Debug Bridge wire protocol.
//!
//! The device firmware and the host `pdb` CLI are two ends of one wire, so
//! they must agree byte for byte. They used to agree by hand: each kept its
//! own copy of these constants, the host's annotated "mirrors the device
//! constant". A disagreement there does not fail loudly — it fails as a
//! rejected install, or a corrupt one.
//!
//! So the constants live here, in a crate small enough that both ends can
//! depend on it: `no_std`, no dependencies, no features. A second MCU family
//! gets the protocol for free rather than copying it a third time.
//!
//! # Frames
//!
//! Request, for every command except the install data stream:
//!
//! ```text
//! [PDBP][cmd: u8][len: u32 LE][payload: len bytes][crc: u32 LE]
//! ```
//!
//! where the CRC covers `[cmd][len][payload]` — see [`crc32_frame`].
//!
//! Response:
//!
//! ```text
//! [PDBP][status: u8][len: u32 LE][payload: len bytes]
//! ```
//!
//! Responses carry no CRC: they travel over the same USB CDC pipe, which is
//! already error-checked, and the host has no way to ask for a retransmit.
//!
//! # Install
//!
//! [`CMD_INSTALL`] is the exception, because the device must erase flash
//! before it can buffer what follows. Phase A is a bare 9-byte header —
//! `[PDBP][CMD_INSTALL][papk_len: u32 LE]`, no payload and no CRC —
//! immediately followed by [`INSTALL_PEEK_BYTES`] of the PAPK inline, which
//! lets the device check compatibility *before* erasing. The device answers
//! [`STATUS_READY`] once erased, then Phase B streams the remaining
//! `papk_len - INSTALL_PEEK_BYTES` bytes and ends with a `u32` LE CRC over
//! the whole `[CMD_INSTALL][papk_len][papk bytes]`.

#![no_std]

pub mod crc32;
pub mod greeting;
pub mod sysmon;

pub use crc32::Crc32;

/// Every frame in either direction starts with these four bytes. A receiver
/// that loses sync scans for them to resynchronise.
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
/// KEY: `[keycode: i32 LE][meta: u8]`, meta per the `KEY_META_*` constants.
pub const INPUT_KEY: u8 = 0x01;
/// TAP: `[x: i32 LE][y: i32 LE]`.
pub const INPUT_TAP: u8 = 0x02;
/// SWIPE: `[x1 i32][y1 i32][x2 i32][y2 i32][duration_ms: u32 LE]` (all LE).
pub const INPUT_SWIPE: u8 = 0x03;

/// [`INPUT_KEY`] `meta` values. The CLI only emits `DOWN_UP`; `DOWN` and `UP`
/// complete the wire contract for held-key use.
pub const KEY_META_DOWN_UP: u8 = 0;
pub const KEY_META_DOWN: u8 = 1;
pub const KEY_META_UP: u8 = 2;

pub const STATUS_OK: u8 = 0x00;
/// Device has erased flash and is ready to receive the install data stream.
pub const STATUS_READY: u8 = 0x01;
pub const STATUS_ERR: u8 = 0xFF;
pub const STATUS_TOO_LARGE: u8 = 0xFE;
pub const STATUS_CRC_FAIL: u8 = 0xFD;
/// Device refused the install: the PAPK's framework-map-version is
/// incompatible with this firmware (asymmetric shrunk/unshrunk, or PAPK newer
/// than firmware). Returned in install Phase A *before* any flash erase, so
/// the PAPK already on the device is unaffected.
pub const STATUS_INCOMPAT: u8 = 0xFC;

/// Number of PAPK bytes the host inlines immediately after the install header
/// (Phase A) so the device can run a pre-erase compatibility check against the
/// PAPK manifest without blocking. Sized comfortably above the 24-byte file
/// header + 16-byte section header + a typical manifest.
pub const INSTALL_PEEK_BYTES: usize = 512;

/// CRC32 over `[cmd byte][4-byte len LE][payload]` — protects all variable
/// fields of a request frame.
///
/// The install stream feeds the same hasher incrementally, page by page, so
/// a one-shot call here and a chunked one there must agree; that they do is
/// a property of the streaming [`Crc32`], pinned by its tests.
pub fn crc32_frame(cmd: u8, len: u32, payload: &[u8]) -> u32 {
    let mut h = Crc32::new();
    h.update(&[cmd]);
    h.update(&len.to_le_bytes());
    h.update(payload);
    h.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The device streams the PAPK to its hasher in 256-byte pages while the
    /// host computes one shot over the whole image. If those ever disagreed
    /// every install would fail with `STATUS_CRC_FAIL`, so pin it here — on
    /// the shared implementation both ends now use.
    #[test]
    fn frame_crc_is_chunk_size_independent() {
        let papk = b"PAPK0123456789abcdefghijklmnopqrstuvwxyz".repeat(20);
        let len = papk.len() as u32;

        let one_shot = crc32_frame(CMD_INSTALL, len, &papk);

        let mut h = Crc32::new();
        h.update(&[CMD_INSTALL]);
        h.update(&len.to_le_bytes());
        for page in papk.chunks(256) {
            h.update(page);
        }

        assert_eq!(one_shot, h.finalize());
    }

    #[test]
    fn frame_crc_covers_cmd_and_len() {
        let payload = b"foobar";
        assert_ne!(
            crc32_frame(CMD_PING, 6, payload),
            crc32_frame(CMD_INSTALL, 6, payload)
        );
        assert_ne!(
            crc32_frame(CMD_INSTALL, 6, payload),
            crc32_frame(CMD_INSTALL, 7, payload)
        );
        assert_ne!(
            crc32_frame(CMD_INSTALL, 6, b"foobar"),
            crc32_frame(CMD_INSTALL, 6, b"foobaz")
        );
    }
}
