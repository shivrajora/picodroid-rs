// SPDX-License-Identifier: GPL-3.0-only
//! The device serial's value type (docs/designs/device-serial-2026-10.md):
//! what a chip ID or a stored string must look like to be one. Where it
//! comes from and where it is published is `picodroid_core::identity`.
//!
//! A family formats its chip's ID the way the vendor's own USB bootloader
//! shows it, so `lsusb` reads the same in both modes: [`Serial::from_id_bytes`]
//! for the usual uppercase hex of a raw ID, [`Serial::from_ascii`] for
//! anything else. `/system/serial` holds an ASCII serial a chip without an
//! ID was given once; [`Serial::from_bytes`] reads it back.

pub use pdb_protocol::greeting::{SERIAL_MAX, UNKNOWN_SERIAL as UNKNOWN};

/// Raw chip IDs [`Serial::from_id_bytes`] accepts: 16 bytes is 32 hex
/// characters, the field's cap.
pub const ID_BYTES_MAX: usize = SERIAL_MAX / 2;

/// A validated serial: 1 to [`SERIAL_MAX`] characters of
/// `[0-9A-Za-z._:-]` — safe in a USB string, a shell word and a `pdb -s`
/// argument.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Serial {
    buf: [u8; SERIAL_MAX],
    len: u8,
}

impl Serial {
    /// Uppercase hex of a raw chip ID, most significant byte first as given.
    /// `None` for an empty or oversized ID, and for one that is all `0x00`
    /// or all `0xFF` — an unprogrammed part, or a flash that ignored the
    /// command — so the caller falls back rather than every such board
    /// sharing one serial.
    pub fn from_id_bytes(id: &[u8]) -> Option<Serial> {
        if id.is_empty() || id.len() > ID_BYTES_MAX {
            return None;
        }
        if id.iter().all(|&b| b == 0x00) || id.iter().all(|&b| b == 0xFF) {
            return None;
        }
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut s = Serial {
            buf: [0; SERIAL_MAX],
            len: (id.len() * 2) as u8,
        };
        for (i, &b) in id.iter().enumerate() {
            s.buf[2 * i] = HEX[usize::from(b >> 4)];
            s.buf[2 * i + 1] = HEX[usize::from(b & 0x0F)];
        }
        Some(s)
    }

    /// A family-formatted serial, for a vendor bootloader that shows
    /// something other than plain hex. `None` if it is empty, longer than
    /// [`SERIAL_MAX`], or has a character outside `[0-9A-Za-z._:-]`.
    pub fn from_ascii(s: &str) -> Option<Serial> {
        let bytes = s.as_bytes();
        if bytes.is_empty() || bytes.len() > SERIAL_MAX || !bytes.iter().all(|&b| allowed(b)) {
            return None;
        }
        let mut out = Serial {
            buf: [0; SERIAL_MAX],
            len: bytes.len() as u8,
        };
        out.buf[..bytes.len()].copy_from_slice(bytes);
        Some(out)
    }

    /// A serial back from a copy of [`Serial::as_bytes`] — what the
    /// firmware publishes at boot and reads on other tasks. Not checked:
    /// `buf[..len]` must be bytes a constructor above produced, and `len`
    /// at most [`SERIAL_MAX`] (clamped).
    #[inline]
    pub fn from_published(buf: [u8; SERIAL_MAX], len: u8) -> Serial {
        Serial {
            buf,
            len: len.min(SERIAL_MAX as u8),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..usize::from(self.len)]
    }

    pub fn as_str(&self) -> &str {
        // Every constructor admits ASCII only.
        core::str::from_utf8(self.as_bytes()).unwrap_or(UNKNOWN)
    }

    /// Whether this is a real serial rather than [`UNKNOWN`].
    #[inline]
    pub fn is_known(&self) -> bool {
        self.as_bytes() != UNKNOWN.as_bytes()
    }

    /// The serial of a device that has none.
    #[inline]
    pub fn unknown() -> Serial {
        // The constant is in the allowed set; a test pins that.
        Serial::from_ascii(UNKNOWN).unwrap_or(Serial {
            buf: [b'?'; SERIAL_MAX],
            len: 1,
        })
    }
}

impl core::fmt::Debug for Serial {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

fn allowed(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_uppercase_and_in_byte_order() {
        let s = Serial::from_id_bytes(&[0xE6, 0x63, 0x38, 0x61, 0xA3, 0x7B, 0x90, 0x38]).unwrap();
        assert_eq!(s.as_str(), "E6633861A37B9038");
        // An STM32-sized 96-bit ID fits too.
        let s = Serial::from_id_bytes(&[0x0A; 12]).unwrap();
        assert_eq!(s.as_str().len(), 24);
    }

    #[test]
    fn blank_empty_and_oversized_ids_are_refused() {
        assert_eq!(Serial::from_id_bytes(&[0x00; 8]), None);
        assert_eq!(Serial::from_id_bytes(&[0xFF; 8]), None);
        assert_eq!(Serial::from_id_bytes(&[]), None);
        assert_eq!(Serial::from_id_bytes(&[0x12; ID_BYTES_MAX + 1]), None);
        assert!(Serial::from_id_bytes(&[0x12; ID_BYTES_MAX]).is_some());
    }

    #[test]
    fn ascii_serials_keep_to_the_safe_set() {
        assert_eq!(
            Serial::from_ascii("emulator-4242").unwrap().as_str(),
            "emulator-4242"
        );
        assert!(Serial::from_ascii("AA:BB:CC:DD:EE:FF").is_some());
        assert!(Serial::from_ascii("206A36705430").is_some());
        assert_eq!(Serial::from_ascii(""), None);
        assert_eq!(Serial::from_ascii("has space"), None);
        assert_eq!(Serial::from_ascii("tab\t"), None);
        assert_eq!(Serial::from_ascii("ünï"), None);
        assert_eq!(Serial::from_ascii("a/b"), None);
        let max = "x".repeat(SERIAL_MAX);
        assert!(Serial::from_ascii(&max).is_some());
        assert_eq!(Serial::from_ascii(&(max + "x")), None);
    }

    #[test]
    fn a_published_copy_reads_back_as_itself() {
        let s = Serial::from_ascii("emulator-4242").unwrap();
        let mut buf = [0u8; SERIAL_MAX];
        buf[..s.as_bytes().len()].copy_from_slice(s.as_bytes());
        assert_eq!(Serial::from_published(buf, s.as_bytes().len() as u8), s);
        assert_eq!(
            Serial::from_published(buf, 200).as_bytes().len(),
            SERIAL_MAX
        );
    }

    #[test]
    fn unknown_is_a_valid_serial_that_says_it_is_unknown() {
        let u = Serial::unknown();
        assert_eq!(u.as_str(), UNKNOWN);
        assert!(!u.is_known());
        assert!(Serial::from_ascii(UNKNOWN).is_some());
    }
}
