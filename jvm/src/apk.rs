//! PAPK (Picodroid APK) binary format parser.
//!
//! A zero-copy, `no_std`/`no_alloc` parser for `.papk` files — the packaging
//! format used to bundle compiled Java `.class` files with an app manifest.
//!
//! # Format overview
//!
//! A PAPK file is a flat binary container with a 24-byte file header followed
//! by a MANIFEST section and a CLASSES section (in that order).  All integers
//! are little-endian.
//!
//! ```text
//! File header (24 bytes):
//!   [0..4]   magic:           b"PAPK"
//!   [4..2]   version_major:   u16 LE  (currently 1)
//!   [6..2]   version_minor:   u16 LE  (currently 0)
//!   [8..4]   section_count:   u32 LE
//!   [12..4]  manifest_offset: u32 LE  (offset to MANIFEST section header)
//!   [16..4]  classes_offset:  u32 LE  (offset to CLASSES section header)
//!   [20..4]  reserved:        u32 LE  = 0
//!
//! Section header (16 bytes):
//!   [0..4]   tag:      u32 LE  ("MANI" or "CLSS")
//!   [4..4]   length:   u32 LE  (byte count of section data, NOT including header)
//!   [8..4]   crc32:    u32 LE  (0 = unchecked in v1)
//!   [12..4]  reserved: u32 LE  = 0
//!
//! MANIFEST section data:
//!   Sequence of [u16 key_len][key][u16 val_len][val] entries (UTF-8, no NUL).
//!   Walk until `length` bytes consumed.
//!
//! CLASSES section data:
//!   [u32 class_count]
//!   For each class:
//!     [u16 name_len][name bytes (JVM internal, no .class suffix)]
//!     [u32 data_len][raw .class file bytes]
//! ```
//!
//! # Lifetime
//!
//! [`Papk`] is lifetime-generic over `'a`, carrying `&'a [u8]`.  When the
//! backing buffer is `'static` (embedded via `include_bytes!` or a
//! `static mut` receive buffer), sub-slices returned by the iterators are
//! also `'static` and can be passed directly to [`crate::Jvm::load_class`].
//!
//! # Example
//!
//! ```rust,ignore
//! static APK: &[u8] = include_bytes!("app.papk");
//!
//! let papk = pico_jvm::apk::Papk::parse(APK).unwrap();
//! let main_class = papk.main_class().unwrap();
//! for entry in papk.classes().unwrap() {
//!     jvm.load_class(entry.data).unwrap();
//! }
//! jvm.invoke_static(main_class, "main", heap, &mut handler).unwrap();
//! ```

use core::str;

// ── Constants ─────────────────────────────────────────────────────────────────

const MAGIC: &[u8; 4] = b"PAPK";
const SUPPORTED_VERSION_MAJOR: u16 = 1;
const FILE_HEADER_LEN: usize = 24;
const SECTION_HEADER_LEN: usize = 16;
const TAG_MANIFEST: u32 = u32::from_le_bytes(*b"MANI");
const TAG_CLASSES: u32 = u32::from_le_bytes(*b"CLSS");

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors returned by the PAPK parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PapkError {
    /// The first four bytes are not `b"PAPK"`.
    BadMagic,
    /// The `version_major` field is not supported by this parser.
    UnsupportedVersion,
    /// The file is shorter than declared or an offset points past the end.
    Truncated,
    /// A required section (MANIFEST or CLASSES) is missing from the file.
    MissingSection,
}

// ── Public types ──────────────────────────────────────────────────────────────

/// One class entry from the CLASSES section.
pub struct ClassEntry<'a> {
    /// JVM internal class name (e.g. `"helloworld/HelloWorld"`), UTF-8.
    pub name: &'a [u8],
    /// Raw `.class` file bytes, suitable for [`crate::Jvm::load_class`].
    pub data: &'a [u8],
}

/// Iterator over class entries in the CLASSES section.
pub struct ClassIter<'a> {
    data: &'a [u8], // slice covering just the CLASSES section data
    pos: usize,
    remaining: u32,
}

impl<'a> Iterator for ClassIter<'a> {
    type Item = ClassEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        // Read name: [u16 len][bytes]
        let name = read_bytes_u16(self.data, &mut self.pos)?;
        // Read class data: [u32 len][bytes]
        let class_data = read_bytes_u32(self.data, &mut self.pos)?;
        self.remaining -= 1;
        Some(ClassEntry {
            name,
            data: class_data,
        })
    }
}

/// Iterator over key/value pairs in the MANIFEST section.
pub struct ManifestIter<'a> {
    data: &'a [u8], // slice covering just the MANIFEST section data
    pos: usize,
}

/// One key/value pair from the MANIFEST section.
pub struct ManifestEntry<'a> {
    pub key: &'a [u8],
    pub value: &'a [u8],
}

impl<'a> Iterator for ManifestIter<'a> {
    type Item = ManifestEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.data.len() {
            return None;
        }
        let key = read_bytes_u16(self.data, &mut self.pos)?;
        let value = read_bytes_u16(self.data, &mut self.pos)?;
        Some(ManifestEntry { key, value })
    }
}

// ── Papk ─────────────────────────────────────────────────────────────────────

/// A zero-copy PAPK file parser.
///
/// Holds a reference to the underlying byte slice.  All sub-slices returned
/// by iterator methods share the same lifetime `'a`.
pub struct Papk<'a> {
    data: &'a [u8],
    manifest_offset: usize,
    classes_offset: usize,
}

impl<'a> Papk<'a> {
    /// Parse the PAPK file header from `data`.
    ///
    /// Only the 24-byte file header is read here; sections are read lazily.
    /// Returns [`PapkError::BadMagic`] if the magic bytes are wrong,
    /// [`PapkError::UnsupportedVersion`] if `version_major != 1`, or
    /// [`PapkError::Truncated`] if the file is shorter than 24 bytes.
    pub fn parse(data: &'a [u8]) -> Result<Self, PapkError> {
        if data.len() < FILE_HEADER_LEN {
            return Err(PapkError::Truncated);
        }
        if &data[0..4] != MAGIC {
            return Err(PapkError::BadMagic);
        }
        let version_major = read_u16_le(data, 4);
        if version_major != SUPPORTED_VERSION_MAJOR {
            return Err(PapkError::UnsupportedVersion);
        }
        let manifest_offset = read_u32_le(data, 12) as usize;
        let classes_offset = read_u32_le(data, 16) as usize;

        // Basic bounds check: both offsets must be within file and have room
        // for the 16-byte section header.
        if manifest_offset + SECTION_HEADER_LEN > data.len() {
            return Err(PapkError::Truncated);
        }
        if classes_offset + SECTION_HEADER_LEN > data.len() {
            return Err(PapkError::Truncated);
        }

        Ok(Self {
            data,
            manifest_offset,
            classes_offset,
        })
    }

    // ── Section accessors ────────────────────────────────────────────────────

    /// Returns the raw MANIFEST section data slice (excluding the section header).
    fn manifest_section_data(&self) -> Result<&'a [u8], PapkError> {
        section_data(self.data, self.manifest_offset, TAG_MANIFEST)
    }

    /// Returns the raw CLASSES section data slice (excluding the section header).
    fn classes_section_data(&self) -> Result<&'a [u8], PapkError> {
        section_data(self.data, self.classes_offset, TAG_CLASSES)
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Returns the `main-class` value from the MANIFEST section, or `None` if
    /// the key is absent or its value is not valid UTF-8.
    pub fn main_class(&self) -> Option<&'a str> {
        let mdata = self.manifest_section_data().ok()?;
        let mut pos = 0usize;
        while pos < mdata.len() {
            let key = read_bytes_u16(mdata, &mut pos)?;
            let val = read_bytes_u16(mdata, &mut pos)?;
            if key == b"main-class" {
                return str::from_utf8(val).ok();
            }
        }
        None
    }

    /// Returns an iterator over all key/value pairs in the MANIFEST section.
    pub fn manifest(&self) -> Result<ManifestIter<'a>, PapkError> {
        let mdata = self.manifest_section_data()?;
        Ok(ManifestIter {
            data: mdata,
            pos: 0,
        })
    }

    /// Returns an iterator over all class entries in the CLASSES section.
    pub fn classes(&self) -> Result<ClassIter<'a>, PapkError> {
        let cdata = self.classes_section_data()?;
        if cdata.len() < 4 {
            return Err(PapkError::Truncated);
        }
        let class_count = read_u32_le(cdata, 0);
        Ok(ClassIter {
            data: cdata,
            pos: 4,
            remaining: class_count,
        })
    }
}

// ── Low-level helpers ─────────────────────────────────────────────────────────

fn read_u16_le(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

fn read_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

/// Reads a length-prefixed byte slice using a `u16` length prefix.
/// Advances `pos` past the length and data. Returns `None` if truncated.
fn read_bytes_u16<'a>(buf: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    if *pos + 2 > buf.len() {
        return None;
    }
    let len = read_u16_le(buf, *pos) as usize;
    *pos += 2;
    if *pos + len > buf.len() {
        return None;
    }
    let slice = &buf[*pos..*pos + len];
    *pos += len;
    Some(slice)
}

/// Reads a length-prefixed byte slice using a `u32` length prefix.
/// Advances `pos` past the length and data. Returns `None` if truncated.
fn read_bytes_u32<'a>(buf: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    if *pos + 4 > buf.len() {
        return None;
    }
    let len = read_u32_le(buf, *pos) as usize;
    *pos += 4;
    if *pos + len > buf.len() {
        return None;
    }
    let slice = &buf[*pos..*pos + len];
    *pos += len;
    Some(slice)
}

/// Returns the section data slice for the section at `section_offset`,
/// verifying that its `tag` matches `expected_tag`.
fn section_data(file: &[u8], section_offset: usize, expected_tag: u32) -> Result<&[u8], PapkError> {
    if section_offset + SECTION_HEADER_LEN > file.len() {
        return Err(PapkError::Truncated);
    }
    let tag = read_u32_le(file, section_offset);
    if tag != expected_tag {
        return Err(PapkError::MissingSection);
    }
    let length = read_u32_le(file, section_offset + 4) as usize;
    let data_start = section_offset + SECTION_HEADER_LEN;
    let data_end = data_start + length;
    if data_end > file.len() {
        return Err(PapkError::Truncated);
    }
    Ok(&file[data_start..data_end])
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    // Build a minimal PAPK for testing without needing the papk-pack binary.
    fn build_test_papk(main_class: &str, classes: &[(&str, &[u8])]) -> Vec<u8> {
        // Manifest data
        let mut manifest_data: Vec<u8> = Vec::new();
        for (k, v) in &[
            ("main-class", main_class),
            ("package-name", "testpkg"),
            ("version", "1.0"),
        ] {
            let kb = k.as_bytes();
            let vb = v.as_bytes();
            manifest_data.extend_from_slice(&(kb.len() as u16).to_le_bytes());
            manifest_data.extend_from_slice(kb);
            manifest_data.extend_from_slice(&(vb.len() as u16).to_le_bytes());
            manifest_data.extend_from_slice(vb);
        }

        // Classes data
        let mut classes_data: Vec<u8> = Vec::new();
        classes_data.extend_from_slice(&(classes.len() as u32).to_le_bytes());
        for (name, bytes) in classes {
            let nb = name.as_bytes();
            classes_data.extend_from_slice(&(nb.len() as u16).to_le_bytes());
            classes_data.extend_from_slice(nb);
            classes_data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            classes_data.extend_from_slice(bytes);
        }

        // Section headers
        let mani_tag = u32::from_le_bytes(*b"MANI");
        let clss_tag = u32::from_le_bytes(*b"CLSS");

        let manifest_offset: u32 = 24; // right after file header
        let classes_offset: u32 = manifest_offset + 16 + manifest_data.len() as u32;

        let mut file: Vec<u8> = Vec::new();
        // File header
        file.extend_from_slice(b"PAPK");
        file.extend_from_slice(&1u16.to_le_bytes()); // version_major
        file.extend_from_slice(&0u16.to_le_bytes()); // version_minor
        file.extend_from_slice(&2u32.to_le_bytes()); // section_count
        file.extend_from_slice(&manifest_offset.to_le_bytes());
        file.extend_from_slice(&classes_offset.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes()); // reserved
                                                     // MANIFEST section header
        file.extend_from_slice(&mani_tag.to_le_bytes());
        file.extend_from_slice(&(manifest_data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes()); // crc32
        file.extend_from_slice(&0u32.to_le_bytes()); // reserved
        file.extend_from_slice(&manifest_data);
        // CLASSES section header
        file.extend_from_slice(&clss_tag.to_le_bytes());
        file.extend_from_slice(&(classes_data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes()); // crc32
        file.extend_from_slice(&0u32.to_le_bytes()); // reserved
        file.extend_from_slice(&classes_data);

        file
    }

    #[test]
    fn test_parse_header() {
        let papk = build_test_papk("test/Main", &[]);
        let p = Papk::parse(Box::leak(papk.into_boxed_slice())).unwrap();
        assert_eq!(p.manifest_offset, 24);
    }

    #[test]
    fn test_main_class() {
        let papk = build_test_papk("hello/World", &[]);
        let p = Papk::parse(Box::leak(papk.into_boxed_slice())).unwrap();
        assert_eq!(p.main_class(), Some("hello/World"));
    }

    #[test]
    fn test_manifest_iter() {
        let papk = build_test_papk("foo/Bar", &[]);
        let p = Papk::parse(Box::leak(papk.into_boxed_slice())).unwrap();
        let entries: Vec<_> = p.manifest().unwrap().collect();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].key, b"main-class");
        assert_eq!(entries[0].value, b"foo/Bar");
        assert_eq!(entries[1].key, b"package-name");
        assert_eq!(entries[2].key, b"version");
    }

    #[test]
    fn test_classes_iter() {
        let fake_class = b"\xCA\xFE\xBA\xBE hello world";
        let papk = build_test_papk("foo/Bar", &[("foo/Bar", fake_class), ("lib/Util", b"data")]);
        let p = Papk::parse(Box::leak(papk.into_boxed_slice())).unwrap();
        let classes: Vec<_> = p.classes().unwrap().collect();
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].name, b"foo/Bar");
        assert_eq!(classes[0].data, fake_class);
        assert_eq!(classes[1].name, b"lib/Util");
        assert_eq!(classes[1].data, b"data");
    }

    #[test]
    fn test_bad_magic() {
        let mut papk = build_test_papk("foo/Bar", &[]);
        papk[0] = 0xFF;
        let leaked: &'static [u8] = Box::leak(papk.into_boxed_slice());
        assert!(matches!(Papk::parse(leaked), Err(PapkError::BadMagic)));
    }

    #[test]
    fn test_truncated() {
        let papk = build_test_papk("foo/Bar", &[]);
        let short: Vec<u8> = papk[..10].to_vec();
        let leaked: &'static [u8] = Box::leak(short.into_boxed_slice());
        assert!(matches!(Papk::parse(leaked), Err(PapkError::Truncated)));
    }
}
