// SPDX-License-Identifier: GPL-3.0-only
//! The on-flash boot-meta sector that marks an installed PAPK.
//!
//! A device does not store a PAPK bare: it writes a 4 KB metadata sector
//! first, and the image follows immediately after. Sector plus image is one
//! *run*, placed at any sector of the app region. Two 256-byte pages of the
//! sector carry information; the rest is erased flash.
//!
//! ```text
//! page 0 (offset 0):   [magic: u32 LE = "PDB1"][flags: u32 LE][len: u32 LE][seq: u32 LE]
//! page 1 (offset 256): [magic: u32 LE = "PDBC"]
//! ```
//!
//! The header page says what the run is; the commit page says the image
//! behind it is complete. A run is an installed app only when both parse
//! ([`parse_meta`]). No page is ever programmed twice: an installer writes
//! the image and then both pages; a relocation writes the header page,
//! copies the image, then the commit page. A power loss anywhere therefore
//! leaves either a whole run or a commit-less one — which the next boot
//! erases — and never a half-copied image that reads as installed.
//!
//! `flags` bit 0 ([`FLAG_BOOT_DEFAULT`]) marks the app a device boots into
//! when nothing else is configured: the build script sets it on the app it
//! bakes into the region, and an install of the same package inherits it.
//! `seq` orders runs: every install or relocation writes one more than the
//! highest sequence number on the device, so when two runs name the same
//! package (a power loss between committing an upgrade and erasing the old
//! copy) the higher `seq` wins. The baked image is `seq` 0.
//!
//! This lived in three places: the family HAL parsed it in `read_flash_papk`
//! and built it by hand in `flash_commit_metadata`, the simulator restated
//! the magic, and the build script assembled the same bytes under a comment
//! reading "Layout matches read_flash_papk() on-device". A comment is not a
//! mechanism. It lives here because that is where the PAPK container format
//! already lives, and because the build script and the firmware can both
//! reach it — which was the property the comment was standing in for.

/// Magic at offset 0 of the header page: `"PDB1"` as a little-endian `u32`.
pub const MAGIC: u32 = 0x5044_4231;

/// Magic at offset 0 of the commit page: `"PDBC"` as a little-endian `u32`.
pub const COMMIT_MAGIC: u32 = 0x5044_4243;

/// Size of the boot-meta sector — one 4 KB flash erase sector. The image
/// starts at exactly this offset from the sector base.
pub const META_SIZE: usize = 4096;

/// A flash program page; each of the two written pages is one of these.
pub const PAGE_LEN: usize = 256;

/// Bytes of the header page that carry information.
pub const HEADER_LEN: usize = 16;

/// Offset of the commit page within the sector.
pub const COMMIT_OFFSET: usize = PAGE_LEN;

/// Bytes of the commit page that carry information.
pub const COMMIT_LEN: usize = 4;

/// Bytes of the sector a reader needs to decide whether a run is installed:
/// the header page and the commit magic behind it.
pub const META_READ_LEN: usize = COMMIT_OFFSET + COMMIT_LEN;

/// `flags` bit 0: boot into this app when no boot package is configured.
pub const FLAG_BOOT_DEFAULT: u32 = 1 << 0;

/// A parsed boot-meta header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootMeta {
    /// Length of the PAPK image following the metadata sector.
    pub len: u32,
    /// [`FLAG_BOOT_DEFAULT`] and reserved bits (written as zero).
    pub flags: u32,
    /// Install sequence number; higher is newer. Zero for the baked image.
    pub seq: u32,
}

/// Build the header page of a run.
///
/// A full 256-byte flash program page rather than just the header: the
/// device programs a page at a time, and `0xFF` is the erased state, so the
/// padding leaves the rest of the page untouched-looking.
pub fn build_header_page(len: u32, flags: u32, seq: u32) -> [u8; PAGE_LEN] {
    let mut page = [0xFFu8; PAGE_LEN];
    page[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    page[4..8].copy_from_slice(&flags.to_le_bytes());
    page[8..12].copy_from_slice(&len.to_le_bytes());
    page[12..16].copy_from_slice(&seq.to_le_bytes());
    page
}

/// Build the commit page that marks a run's image complete.
pub fn build_commit_page() -> [u8; PAGE_LEN] {
    let mut page = [0xFFu8; PAGE_LEN];
    page[0..4].copy_from_slice(&COMMIT_MAGIC.to_le_bytes());
    page
}

/// Both pages back to back, for an installer that writes them in one
/// program call after the image is complete.
pub fn build_meta_pages(len: u32, flags: u32, seq: u32) -> [u8; 2 * PAGE_LEN] {
    let mut pages = [0xFFu8; 2 * PAGE_LEN];
    pages[..PAGE_LEN].copy_from_slice(&build_header_page(len, flags, seq));
    pages[COMMIT_OFFSET..].copy_from_slice(&build_commit_page());
    pages
}

fn word(bytes: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]])
}

/// Parse the header page alone, or `None` if this is not a run at all.
///
/// A header that parses without a commit page ([`is_committed`]) is a
/// relocation or install that never finished: not an installed app, but
/// not free space either — the next boot erases it. `max_len` is the
/// largest image the region can hold from this sector on; a length beyond
/// it means the sector is stale or corrupt.
pub fn parse_header(bytes: &[u8], max_len: usize) -> Option<BootMeta> {
    if bytes.len() < HEADER_LEN {
        return None;
    }
    if word(bytes, 0) != MAGIC {
        return None;
    }
    let len = word(bytes, 8);
    if len == 0 || len as usize > max_len {
        return None;
    }
    Some(BootMeta {
        len,
        flags: word(bytes, 4),
        seq: word(bytes, 12),
    })
}

/// Whether the commit page carries its magic. Needs [`META_READ_LEN`] bytes.
pub fn is_committed(bytes: &[u8]) -> bool {
    bytes.len() >= META_READ_LEN && word(bytes, COMMIT_OFFSET) == COMMIT_MAGIC
}

/// Parse an installed run's boot-meta, or `None` if the sector holds no
/// committed run. Erased flash reads as `0xFFFF_FFFF`, which fails the
/// magic check, so a never-installed device takes the `None` path naturally.
pub fn parse_meta(bytes: &[u8], max_len: usize) -> Option<BootMeta> {
    let meta = parse_header(bytes, max_len)?;
    is_committed(bytes).then_some(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = 1020 * 1024;

    #[test]
    fn built_pages_parse_back() {
        let pages = build_meta_pages(4321, FLAG_BOOT_DEFAULT, 9);
        assert_eq!(
            parse_meta(&pages, MAX),
            Some(BootMeta {
                len: 4321,
                flags: FLAG_BOOT_DEFAULT,
                seq: 9,
            })
        );
    }

    #[test]
    fn header_layout_is_magic_flags_len_seq() {
        let page = build_header_page(0x0001_2345, 0x0000_0001, 0x0000_0007);
        assert_eq!(&page[0..4], &MAGIC.to_le_bytes());
        assert_eq!(&page[4..8], &1u32.to_le_bytes());
        assert_eq!(&page[8..12], &0x0001_2345u32.to_le_bytes());
        assert_eq!(&page[12..16], &7u32.to_le_bytes());
        // Everything past the header stays erased, so re-reading the page
        // cannot pick up stale bytes from a previous install.
        assert!(page[HEADER_LEN..].iter().all(|&b| b == 0xFF));
        let commit = build_commit_page();
        assert_eq!(&commit[0..4], &COMMIT_MAGIC.to_le_bytes());
        assert!(commit[COMMIT_LEN..].iter().all(|&b| b == 0xFF));
    }

    /// A header without its commit page is the state a relocation or an
    /// install leaves behind when power fails: visible as a header (so the
    /// next boot can erase it) but never as an installed app.
    #[test]
    fn a_commit_less_header_is_not_an_install() {
        let mut sector = [0xFFu8; META_READ_LEN];
        sector[..PAGE_LEN].copy_from_slice(&build_header_page(64, 0, 3));
        assert!(parse_header(&sector, MAX).is_some());
        assert!(!is_committed(&sector));
        assert_eq!(parse_meta(&sector, MAX), None);
        sector[COMMIT_OFFSET..].copy_from_slice(&build_commit_page()[..COMMIT_LEN]);
        assert_eq!(parse_meta(&sector, MAX).map(|m| m.seq), Some(3));
    }

    /// A device that has never been installed to reads erased flash. That
    /// must be "no PAPK", not a bogus one — this is the path every first boot
    /// takes.
    #[test]
    fn erased_flash_is_not_an_install() {
        assert_eq!(parse_header(&[0xFFu8; 512], MAX), None);
        assert_eq!(parse_meta(&[0xFFu8; 512], MAX), None);
    }

    #[test]
    fn wrong_magic_is_rejected() {
        let mut pages = build_meta_pages(64, 0, 0);
        pages[0] ^= 0xFF;
        assert_eq!(parse_meta(&pages, MAX), None);
    }

    /// Zero-length and over-long both mean a stale or half-written sector,
    /// and both would otherwise produce a slice the JVM then walks off.
    #[test]
    fn implausible_lengths_are_rejected() {
        assert_eq!(parse_meta(&build_meta_pages(0, 0, 0), MAX), None);
        assert_eq!(
            parse_meta(&build_meta_pages(MAX as u32 + 1, 0, 0), MAX),
            None
        );
        assert!(parse_meta(&build_meta_pages(MAX as u32, 0, 0), MAX).is_some());
    }

    #[test]
    fn truncated_buffers_are_rejected_not_read_past() {
        let pages = build_meta_pages(64, 0, 0);
        assert_eq!(parse_header(&pages[..HEADER_LEN - 1], MAX), None);
        // Header present, commit page cut off: a header, not an install.
        assert!(parse_header(&pages[..PAGE_LEN], MAX).is_some());
        assert!(!is_committed(&pages[..META_READ_LEN - 1]));
        assert_eq!(parse_meta(&pages[..META_READ_LEN - 1], MAX), None);
        assert!(parse_meta(&pages[..META_READ_LEN], MAX).is_some());
    }
}
