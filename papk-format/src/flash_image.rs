// SPDX-License-Identifier: GPL-3.0-only
//! The on-flash boot-meta header that marks an installed PAPK.
//!
//! A device does not store a PAPK bare: it writes a 4 KB metadata sector
//! first, and the image follows immediately after. The sector's first twelve
//! bytes are the whole format —
//!
//! ```text
//! [magic: u32 LE = "PDB1"][flags: u32 LE][len: u32 LE]
//! ```
//!
//! — and the rest is erased flash. `flags` is reserved for A/B slot
//! selection and is written as zero.
//!
//! This lived in three places: the family HAL parsed it in `read_flash_papk`
//! and built it by hand in `flash_commit_metadata`, the simulator restated
//! the magic, and the build script assembled the same twelve bytes under a
//! comment reading "Layout matches read_flash_papk() on-device". A comment is
//! not a mechanism. It lives here because that is where the PAPK container
//! format already lives, and because the build script and the firmware can
//! both reach it — which was the property the comment was standing in for.

/// Magic at offset 0 of the boot-meta sector: `"PDB1"` as a little-endian
/// `u32`.
pub const MAGIC: u32 = 0x5044_4231;

/// Size of the boot-meta sector — one 4 KB flash erase sector. The image
/// starts at exactly this offset from the sector base.
pub const META_SIZE: usize = 4096;

/// Bytes of the sector that carry information; the remainder is left erased.
pub const HEADER_LEN: usize = 12;

/// A parsed boot-meta header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootMeta {
    /// Length of the PAPK image following the metadata sector.
    pub len: u32,
    /// Reserved for A/B slot selection; zero today.
    pub flags: u32,
}

/// Build the metadata page that commits an install.
///
/// Returns a full 256-byte flash program page rather than just the header:
/// the device programs a page at a time, and `0xFF` is the erased state, so
/// the padding leaves the rest of the page untouched-looking.
pub fn build_meta_page(len: u32) -> [u8; 256] {
    let mut page = [0xFFu8; 256];
    page[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    page[4..8].copy_from_slice(&0u32.to_le_bytes());
    page[8..12].copy_from_slice(&len.to_le_bytes());
    page
}

/// Parse a boot-meta header, or `None` if this is not an installed PAPK.
///
/// `max_len` is the largest image the caller's flash slot can hold; a length
/// beyond it means the sector is stale or corrupt rather than a valid
/// install. Erased flash reads as `0xFFFF_FFFF`, which fails the magic check,
/// so a never-installed device takes the `None` path naturally.
pub fn parse_meta(bytes: &[u8], max_len: usize) -> Option<BootMeta> {
    if bytes.len() < HEADER_LEN {
        return None;
    }
    let word = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    if word(0) != MAGIC {
        return None;
    }
    let len = word(8);
    if len == 0 || len as usize > max_len {
        return None;
    }
    Some(BootMeta {
        len,
        flags: word(4),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = 1020 * 1024;

    #[test]
    fn built_page_parses_back() {
        let page = build_meta_page(4321);
        assert_eq!(
            parse_meta(&page, MAX),
            Some(BootMeta {
                len: 4321,
                flags: 0
            })
        );
    }

    #[test]
    fn header_layout_is_magic_flags_len() {
        let page = build_meta_page(0x0001_2345);
        assert_eq!(&page[0..4], &MAGIC.to_le_bytes());
        assert_eq!(&page[4..8], &0u32.to_le_bytes());
        assert_eq!(&page[8..12], &0x0001_2345u32.to_le_bytes());
        // Everything past the header stays erased, so re-reading the page
        // cannot pick up stale bytes from a previous install.
        assert!(page[HEADER_LEN..].iter().all(|&b| b == 0xFF));
    }

    /// A device that has never been installed to reads erased flash. That
    /// must be "no PAPK", not a bogus one — this is the path every first boot
    /// takes.
    #[test]
    fn erased_flash_is_not_an_install() {
        assert_eq!(parse_meta(&[0xFFu8; 256], MAX), None);
    }

    #[test]
    fn wrong_magic_is_rejected() {
        let mut page = build_meta_page(64);
        page[0] ^= 0xFF;
        assert_eq!(parse_meta(&page, MAX), None);
    }

    /// Zero-length and over-long both mean a stale or half-written sector,
    /// and both would otherwise produce a slice the JVM then walks off.
    #[test]
    fn implausible_lengths_are_rejected() {
        assert_eq!(parse_meta(&build_meta_page(0), MAX), None);
        assert_eq!(parse_meta(&build_meta_page(MAX as u32 + 1), MAX), None);
        assert!(parse_meta(&build_meta_page(MAX as u32), MAX).is_some());
    }

    #[test]
    fn truncated_header_is_rejected() {
        assert_eq!(
            parse_meta(&build_meta_page(64)[..HEADER_LEN - 1], MAX),
            None
        );
    }
}
