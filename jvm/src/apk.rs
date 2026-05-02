// SPDX-License-Identifier: GPL-3.0-only
//! PAPK (Picodroid APK) binary format parser.
//!
//! A zero-copy, `no_std`/`no_alloc` parser for `.papk` files — the packaging
//! format used to bundle compiled Java `.class` files with an app manifest.
//!
//! # Format overview
//!
//! A PAPK file is a flat binary container with a 24-byte file header followed
//! by a MANIFEST section, a CLASSES section, and (optionally, in v1.1+) an
//! ASSETS section.  All integers are little-endian.
//!
//! ```text
//! File header (24 bytes):
//!   [0..4]   magic:           b"PAPK"
//!   [4..2]   version_major:   u16 LE  (currently 1)
//!   [6..2]   version_minor:   u16 LE  (0 = no assets, 1 = ASSETS section may exist)
//!   [8..4]   section_count:   u32 LE
//!   [12..4]  manifest_offset: u32 LE  (offset to MANIFEST section header)
//!   [16..4]  classes_offset:  u32 LE  (offset to CLASSES section header)
//!   [20..4]  assets_offset:   u32 LE  (offset to ASSETS section header, 0 = absent)
//!
//! Section header (16 bytes):
//!   [0..4]   tag:      u32 LE  ("MANI", "CLSS", or "ASST")
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
//!
//! ASSETS section data (v1.1+):
//!   [u32 asset_count]
//!   For each asset:
//!     [u16 name_len][name bytes (UTF-8, e.g. "logo.png")]
//!     [u16 width][u16 height]
//!     [u8 cf][u8 reserved0][u16 stride]
//!     [u32 data_size]
//!     [pad 0..3 bytes so data starts at 4-byte offset within the section]
//!     [pixel bytes (data_size bytes; LVGL-native, not encoded)]
//!     [pad 0..3 bytes so next record starts at 4-byte offset within the section]
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
const TAG_ASSETS: u32 = u32::from_le_bytes(*b"ASST");

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
    /// The PAPK's `framework-map-version` manifest key is missing but the
    /// firmware requires one (caller opted into strict checking).
    FrameworkVersionMissing,
    /// The PAPK was built against a shrink-map version newer than the
    /// firmware's active version; the append-only invariant cannot cover
    /// the gap. Rebuild the app against matching firmware.
    FrameworkVersionMismatch,
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

/// One asset entry from the ASSETS section (v1.1+).
///
/// `data` is the raw, LVGL-native pixel buffer (already decoded by `papk-pack`
/// at build time — no PNG/JPEG decoder runs on the firmware). The slice is
/// guaranteed to start on a 4-byte boundary within the section, which means
/// at a 4-byte boundary within the file as long as the section header itself
/// lands on one.
pub struct AssetEntry<'a> {
    /// Asset name (e.g. `"logo.png"`). Lookup is by exact name match.
    pub name: &'a [u8],
    /// Pixel width.
    pub width: u16,
    /// Pixel height.
    pub height: u16,
    /// LVGL color format (`lv_color_format_t`).
    pub cf: u8,
    /// Bytes per row. `0` = computed by LVGL from `cf` and `width`.
    pub stride: u16,
    /// Raw pixel data (`width * height * bytes-per-pixel` for uncompressed CFs).
    pub data: &'a [u8],
}

/// Iterator over asset entries in the ASSETS section.
pub struct AssetIter<'a> {
    data: &'a [u8], // slice covering just the ASSETS section data
    pos: usize,
    remaining: u32,
}

impl<'a> Iterator for AssetIter<'a> {
    type Item = AssetEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        // [u16 name_len][name bytes]
        let name = read_bytes_u16(self.data, &mut self.pos)?;
        // [u16 width][u16 height][u8 cf][u8 reserved0][u16 stride][u32 data_size]
        if self.pos + 12 > self.data.len() {
            return None;
        }
        let width = read_u16_le(self.data, self.pos);
        let height = read_u16_le(self.data, self.pos + 2);
        let cf = self.data[self.pos + 4];
        // self.data[self.pos + 5] is reserved0 (must be 0).
        let stride = read_u16_le(self.data, self.pos + 6);
        let data_size = read_u32_le(self.data, self.pos + 8) as usize;
        self.pos += 12;
        // Pad up to a 4-byte boundary within the section before the data.
        self.pos = (self.pos + 3) & !3;
        if self.pos + data_size > self.data.len() {
            return None;
        }
        let data = &self.data[self.pos..self.pos + data_size];
        self.pos += data_size;
        // Pad up to a 4-byte boundary within the section before the next record.
        self.pos = (self.pos + 3) & !3;
        self.remaining -= 1;
        Some(AssetEntry {
            name,
            width,
            height,
            cf,
            stride,
            data,
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
    /// Offset to the ASSETS section header, or 0 if the section is absent
    /// (legacy v1.0 papks).
    assets_offset: usize,
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
        // 0 here means "no ASSETS section" — the field doubled as `reserved`
        // in v1.0, where it was always written as 0. So legacy papks parse
        // without surprise.
        let assets_offset = read_u32_le(data, 20) as usize;

        // Basic bounds check: both offsets must be within file and have room
        // for the 16-byte section header.
        if manifest_offset + SECTION_HEADER_LEN > data.len() {
            return Err(PapkError::Truncated);
        }
        if classes_offset + SECTION_HEADER_LEN > data.len() {
            return Err(PapkError::Truncated);
        }
        if assets_offset != 0 && assets_offset + SECTION_HEADER_LEN > data.len() {
            return Err(PapkError::Truncated);
        }

        Ok(Self {
            data,
            manifest_offset,
            classes_offset,
            assets_offset,
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

    /// Returns the raw ASSETS section data slice (excluding the section header),
    /// or `None` if the file has no ASSETS section.
    fn assets_section_data(&self) -> Result<Option<&'a [u8]>, PapkError> {
        if self.assets_offset == 0 {
            return Ok(None);
        }
        section_data(self.data, self.assets_offset, TAG_ASSETS).map(Some)
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Returns the `main-class` value from the MANIFEST section, or `None` if
    /// the key is absent or its value is not valid UTF-8.
    pub fn main_class(&self) -> Option<&'a str> {
        self.manifest_value(b"main-class")
    }

    /// Returns the `activity` value from the MANIFEST section, or `None` if
    /// the key is absent or its value is not valid UTF-8.
    pub fn activity(&self) -> Option<&'a str> {
        self.manifest_value(b"activity")
    }

    /// Returns the `application` value from the MANIFEST section, or `None` if
    /// the key is absent or its value is not valid UTF-8.
    pub fn application(&self) -> Option<&'a str> {
        self.manifest_value(b"application")
    }

    /// Returns the `framework-map-version` value from the MANIFEST section,
    /// or `None` if the key is absent (legacy PAPK) or not valid UTF-8.
    ///
    /// The value is a semver string like `"0.1.0"`; `"0.0.0"` is the sentinel
    /// emitted when the firmware and PAPK were both built against no shrink
    /// map (default behavior until a release cut introduces one).
    pub fn framework_map_version(&self) -> Option<&'a str> {
        self.manifest_value(b"framework-map-version")
    }

    /// Verify this PAPK's shrink-map version is compatible with the firmware.
    ///
    /// Delegates to [`compat::check`] so the host-side `pdb install`
    /// pre-flight and this device-side load-time check share one rule
    /// implementation. See `compat` crate docs for the table.
    pub fn verify_compat(&self, firmware_version: &str) -> Result<(), PapkError> {
        compat::check(self.framework_map_version(), firmware_version).map_err(|e| match e {
            compat::CompatError::Missing => PapkError::FrameworkVersionMissing,
            compat::CompatError::Mismatch | compat::CompatError::BadVersion => {
                PapkError::FrameworkVersionMismatch
            }
        })
    }

    /// Look up a manifest key and return its value as a UTF-8 string.
    fn manifest_value(&self, target_key: &[u8]) -> Option<&'a str> {
        let mdata = self.manifest_section_data().ok()?;
        let mut pos = 0usize;
        while pos < mdata.len() {
            let key = read_bytes_u16(mdata, &mut pos)?;
            let val = read_bytes_u16(mdata, &mut pos)?;
            if key == target_key {
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

    /// Returns an iterator over all asset entries in the ASSETS section,
    /// or `None` if the papk has no ASSETS section.
    ///
    /// The pixel `data` slice for each asset shares the lifetime `'a` of the
    /// underlying papk buffer, so it can be handed directly to LVGL when the
    /// papk is `'static` (the embedded case — papk lives in XIP flash for
    /// the firmware's lifetime).
    pub fn assets(&self) -> Result<Option<AssetIter<'a>>, PapkError> {
        let Some(adata) = self.assets_section_data()? else {
            return Ok(None);
        };
        if adata.len() < 4 {
            return Err(PapkError::Truncated);
        }
        let asset_count = read_u32_le(adata, 0);
        Ok(Some(AssetIter {
            data: adata,
            pos: 4,
            remaining: asset_count,
        }))
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

    fn build_test_papk_application(application: &str, classes: &[(&str, &[u8])]) -> Vec<u8> {
        let mut manifest_data: Vec<u8> = Vec::new();
        for (k, v) in &[
            ("application", application),
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

        let mut classes_data: Vec<u8> = Vec::new();
        classes_data.extend_from_slice(&(classes.len() as u32).to_le_bytes());
        for (name, bytes) in classes {
            let nb = name.as_bytes();
            classes_data.extend_from_slice(&(nb.len() as u16).to_le_bytes());
            classes_data.extend_from_slice(nb);
            classes_data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            classes_data.extend_from_slice(bytes);
        }

        let mani_tag = u32::from_le_bytes(*b"MANI");
        let clss_tag = u32::from_le_bytes(*b"CLSS");
        let manifest_offset: u32 = 24;
        let classes_offset: u32 = manifest_offset + 16 + manifest_data.len() as u32;

        let mut file: Vec<u8> = Vec::new();
        file.extend_from_slice(b"PAPK");
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&0u16.to_le_bytes());
        file.extend_from_slice(&2u32.to_le_bytes());
        file.extend_from_slice(&manifest_offset.to_le_bytes());
        file.extend_from_slice(&classes_offset.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&mani_tag.to_le_bytes());
        file.extend_from_slice(&(manifest_data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&manifest_data);
        file.extend_from_slice(&clss_tag.to_le_bytes());
        file.extend_from_slice(&(classes_data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&classes_data);

        file
    }

    #[test]
    fn test_application() {
        let papk = build_test_papk_application("demo/MyApp", &[]);
        let p = Papk::parse(Box::leak(papk.into_boxed_slice())).unwrap();
        assert_eq!(p.application(), Some("demo/MyApp"));
        assert_eq!(p.main_class(), None);
        assert_eq!(p.activity(), None);
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

    /// Build a PAPK with a custom set of manifest key/value pairs (no classes).
    fn build_papk_with_manifest(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut manifest_data: Vec<u8> = Vec::new();
        for (k, v) in entries {
            let kb = k.as_bytes();
            let vb = v.as_bytes();
            manifest_data.extend_from_slice(&(kb.len() as u16).to_le_bytes());
            manifest_data.extend_from_slice(kb);
            manifest_data.extend_from_slice(&(vb.len() as u16).to_le_bytes());
            manifest_data.extend_from_slice(vb);
        }
        let classes_data: Vec<u8> = 0u32.to_le_bytes().to_vec();
        let mani_tag = u32::from_le_bytes(*b"MANI");
        let clss_tag = u32::from_le_bytes(*b"CLSS");
        let manifest_offset: u32 = 24;
        let classes_offset: u32 = manifest_offset + 16 + manifest_data.len() as u32;

        let mut file: Vec<u8> = Vec::new();
        file.extend_from_slice(b"PAPK");
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes()); // version_minor = 1
        file.extend_from_slice(&2u32.to_le_bytes());
        file.extend_from_slice(&manifest_offset.to_le_bytes());
        file.extend_from_slice(&classes_offset.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&mani_tag.to_le_bytes());
        file.extend_from_slice(&(manifest_data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&manifest_data);
        file.extend_from_slice(&clss_tag.to_le_bytes());
        file.extend_from_slice(&(classes_data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&classes_data);
        file
    }

    fn parse_leaked(papk: Vec<u8>) -> Papk<'static> {
        Papk::parse(Box::leak(papk.into_boxed_slice())).unwrap()
    }

    #[test]
    fn framework_map_version_reads_manifest_key() {
        let papk =
            build_papk_with_manifest(&[("main-class", "x/Y"), ("framework-map-version", "0.1.0")]);
        let p = parse_leaked(papk);
        assert_eq!(p.framework_map_version(), Some("0.1.0"));
    }

    #[test]
    fn verify_compat_accepts_equal_versions() {
        let papk =
            build_papk_with_manifest(&[("main-class", "x/Y"), ("framework-map-version", "0.1.0")]);
        assert_eq!(parse_leaked(papk).verify_compat("0.1.0"), Ok(()));
    }

    #[test]
    fn verify_compat_accepts_older_papk() {
        let papk =
            build_papk_with_manifest(&[("main-class", "x/Y"), ("framework-map-version", "0.1.0")]);
        assert_eq!(parse_leaked(papk).verify_compat("0.2.0"), Ok(()));
    }

    #[test]
    fn verify_compat_rejects_newer_papk() {
        let papk =
            build_papk_with_manifest(&[("main-class", "x/Y"), ("framework-map-version", "0.2.0")]);
        assert_eq!(
            parse_leaked(papk).verify_compat("0.1.0"),
            Err(PapkError::FrameworkVersionMismatch)
        );
    }

    #[test]
    fn verify_compat_accepts_unversioned_papk_against_sentinel_firmware() {
        // A legacy PAPK (pre-M1) is compatible with firmware that hasn't
        // cut any shrink-map release yet.
        let papk = build_papk_with_manifest(&[("main-class", "x/Y")]);
        assert_eq!(parse_leaked(papk).verify_compat("0.0.0"), Ok(()));
    }

    #[test]
    fn verify_compat_rejects_unversioned_papk_against_released_firmware() {
        let papk = build_papk_with_manifest(&[("main-class", "x/Y")]);
        assert_eq!(
            parse_leaked(papk).verify_compat("0.1.0"),
            Err(PapkError::FrameworkVersionMissing)
        );
    }

    #[test]
    fn verify_compat_rejects_unshrunk_papk_against_shrunk_firmware() {
        // PAPK built without --shrink carries version "0.0.0" (original
        // framework names in its CP). Firmware built with --shrink loads
        // only shrunk framework classes. Linkage would fail — reject.
        let papk =
            build_papk_with_manifest(&[("main-class", "x/Y"), ("framework-map-version", "0.0.0")]);
        assert_eq!(
            parse_leaked(papk).verify_compat("0.1.0"),
            Err(PapkError::FrameworkVersionMismatch)
        );
    }

    #[test]
    fn verify_compat_rejects_shrunk_papk_against_unshrunk_firmware() {
        // Symmetric guard: shrunk PAPK refers to shrunk names that the
        // unshrunk firmware simply doesn't have.
        let papk =
            build_papk_with_manifest(&[("main-class", "x/Y"), ("framework-map-version", "0.1.0")]);
        assert_eq!(
            parse_leaked(papk).verify_compat("0.0.0"),
            Err(PapkError::FrameworkVersionMismatch)
        );
    }

    // ── ASSETS section tests (v1.1+) ─────────────────────────────────────

    /// Build a v1.1 PAPK with empty manifest/classes and a populated ASSETS
    /// section. Each asset is `(name, w, h, cf, stride, data)`.
    fn build_papk_with_assets(assets: &[(&str, u16, u16, u8, u16, &[u8])]) -> Vec<u8> {
        // Empty manifest data + zero-class CLASSES section.
        let manifest_data: Vec<u8> = Vec::new();
        let classes_data: Vec<u8> = 0u32.to_le_bytes().to_vec();

        // ASSETS section data: [u32 count] then per-asset records.
        let mut assets_data: Vec<u8> = Vec::new();
        assets_data.extend_from_slice(&(assets.len() as u32).to_le_bytes());
        for (name, w, h, cf, stride, data) in assets {
            let nb = name.as_bytes();
            assets_data.extend_from_slice(&(nb.len() as u16).to_le_bytes());
            assets_data.extend_from_slice(nb);
            assets_data.extend_from_slice(&w.to_le_bytes());
            assets_data.extend_from_slice(&h.to_le_bytes());
            assets_data.push(*cf);
            assets_data.push(0); // reserved0
            assets_data.extend_from_slice(&stride.to_le_bytes());
            assets_data.extend_from_slice(&(data.len() as u32).to_le_bytes());
            // Pad to 4-byte boundary within the section before data.
            while assets_data.len() % 4 != 0 {
                assets_data.push(0);
            }
            assets_data.extend_from_slice(data);
            // Pad to 4-byte boundary before next record.
            while assets_data.len() % 4 != 0 {
                assets_data.push(0);
            }
        }

        let mani_tag = u32::from_le_bytes(*b"MANI");
        let clss_tag = u32::from_le_bytes(*b"CLSS");
        let asst_tag = u32::from_le_bytes(*b"ASST");

        let manifest_offset: u32 = 24;
        let classes_offset: u32 = manifest_offset + 16 + manifest_data.len() as u32;
        let assets_offset: u32 = classes_offset + 16 + classes_data.len() as u32;

        let mut file: Vec<u8> = Vec::new();
        file.extend_from_slice(b"PAPK");
        file.extend_from_slice(&1u16.to_le_bytes()); // version_major
        file.extend_from_slice(&1u16.to_le_bytes()); // version_minor
        file.extend_from_slice(&3u32.to_le_bytes()); // section_count
        file.extend_from_slice(&manifest_offset.to_le_bytes());
        file.extend_from_slice(&classes_offset.to_le_bytes());
        file.extend_from_slice(&assets_offset.to_le_bytes());
        // MANIFEST section (empty payload)
        file.extend_from_slice(&mani_tag.to_le_bytes());
        file.extend_from_slice(&(manifest_data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&manifest_data);
        // CLASSES section (zero classes)
        file.extend_from_slice(&clss_tag.to_le_bytes());
        file.extend_from_slice(&(classes_data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&classes_data);
        // ASSETS section
        file.extend_from_slice(&asst_tag.to_le_bytes());
        file.extend_from_slice(&(assets_data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&assets_data);
        file
    }

    #[test]
    fn assets_section_absent_in_legacy_papk() {
        // Any builder that writes 0 in the [20..24] header slot — the v1.0
        // `reserved` field — yields a papk with no ASSETS section.
        let papk = build_test_papk("foo/Bar", &[]);
        let p = parse_leaked(papk);
        assert!(p.assets().unwrap().is_none());
    }

    #[test]
    fn assets_section_iterates_records() {
        // Two assets: a 2x2 RGB565 (cf=18) and a 1x1 single-byte (cf=99).
        let asset_a: &[u8] = &[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80];
        let asset_b: &[u8] = &[0xAB];
        let papk = build_papk_with_assets(&[
            ("logo.png", 2, 2, 18, 0, asset_a),
            ("dot.png", 1, 1, 99, 1, asset_b),
        ]);
        let p = parse_leaked(papk);
        let mut iter = p.assets().unwrap().expect("ASSETS section present");
        let a = iter.next().unwrap();
        assert_eq!(a.name, b"logo.png");
        assert_eq!(a.width, 2);
        assert_eq!(a.height, 2);
        assert_eq!(a.cf, 18);
        assert_eq!(a.stride, 0);
        assert_eq!(a.data, asset_a);
        let b = iter.next().unwrap();
        assert_eq!(b.name, b"dot.png");
        assert_eq!(b.width, 1);
        assert_eq!(b.height, 1);
        assert_eq!(b.cf, 99);
        assert_eq!(b.stride, 1);
        assert_eq!(b.data, asset_b);
        assert!(iter.next().is_none());
    }

    #[test]
    fn assets_data_is_4_byte_aligned_within_section() {
        // Pixel data alignment matters for LVGL u16/u32 reads from XIP flash.
        // Names of odd length force the writer to insert padding; verify that
        // the iterator returns a `data` slice whose offset within the section
        // is a multiple of 4.
        let pixels: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];
        let papk = build_papk_with_assets(&[
            ("a.png", 1, 1, 18, 0, pixels), // odd-length-ish name
        ]);
        let p = parse_leaked(papk);
        let entry = p.assets().unwrap().unwrap().next().unwrap();
        let section = p.assets_section_data().unwrap().unwrap();
        let data_offset = (entry.data.as_ptr() as usize).wrapping_sub(section.as_ptr() as usize);
        assert_eq!(data_offset % 4, 0, "asset data must be 4-byte aligned");
    }

    #[test]
    fn assets_section_truncated_offset_rejected() {
        // Build a v1.1 papk, then corrupt assets_offset to point past EOF.
        let mut papk = build_papk_with_assets(&[("x.png", 1, 1, 18, 0, &[0, 0])]);
        let len = papk.len() as u32;
        let bad = (len + 100).to_le_bytes();
        papk[20..24].copy_from_slice(&bad);
        let leaked: &'static [u8] = Box::leak(papk.into_boxed_slice());
        assert!(matches!(Papk::parse(leaked), Err(PapkError::Truncated)));
    }
}
