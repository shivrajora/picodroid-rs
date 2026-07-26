// SPDX-License-Identifier: GPL-3.0-only
//! PAPK (Picodroid APK) container format — the single source of truth for
//! the on-disk layout.
//!
//! A zero-copy, `no_std`/`no_alloc` parser for `.papk` files — the packaging
//! format used to bundle compiled Java `.class` files with an app manifest.
//! The alloc-based writer ([`PapkBuilder`]) lives behind the `write` feature.
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
//! also `'static` and can be passed directly to `pico_jvm::Jvm::load_class`.
//!
//! # Example
//!
//! ```rust,ignore
//! static APK: &[u8] = include_bytes!("app.papk");
//!
//! let papk = papk_format::Papk::parse(APK).unwrap();
//! let main_class = papk.main_class().unwrap();
//! for entry in papk.classes().unwrap() {
//!     jvm.load_class(entry.data).unwrap();
//! }
//! jvm.invoke_static(main_class, "main", heap, &mut handler).unwrap();
//! ```

#![cfg_attr(not(test), no_std)]

#[cfg(any(test, feature = "write"))]
extern crate alloc;

mod scan;
pub use scan::{
    find_manifest_value, find_manifest_value_in_prefix, validate_structure, StructuralError,
};

#[cfg(feature = "write")]
mod write;
#[cfg(feature = "write")]
pub use write::{AssetSpec, BuildError, EntryPoint, ManifestSpec, PapkBuilder};

use core::str;

// ── Constants ─────────────────────────────────────────────────────────────────

/// The four magic bytes at the start of every PAPK file.
pub const MAGIC: &[u8; 4] = b"PAPK";
/// The only `version_major` this parser accepts.
pub const SUPPORTED_VERSION_MAJOR: u16 = 1;
/// `version_major` the writer emits.
pub const VERSION_MAJOR: u16 = 1;
/// `version_minor` the writer emits (1 since the `framework-map-version`
/// manifest key / ASSETS section were introduced).
pub const VERSION_MINOR: u16 = 1;
/// Byte length of the fixed file header.
pub const FILE_HEADER_LEN: usize = 24;
/// Byte length of each section header.
pub const SECTION_HEADER_LEN: usize = 16;
/// Section tag for the MANIFEST section (`b"MANI"` as a LE u32).
pub const TAG_MANIFEST: u32 = u32::from_le_bytes(*b"MANI");
/// Section tag for the CLASSES section (`b"CLSS"` as a LE u32).
pub const TAG_CLASSES: u32 = u32::from_le_bytes(*b"CLSS");
/// Section tag for the ASSETS section (`b"ASST"` as a LE u32).
pub const TAG_ASSETS: u32 = u32::from_le_bytes(*b"ASST");

/// Well-known manifest keys — ends the `b"framework-map-version"` string
/// literals scattered across four crates.
pub mod keys {
    /// Entry point: a class with `public static void main(String[])`.
    pub const MAIN_CLASS: &[u8] = b"main-class";
    /// Entry point: an `Activity` subclass.
    pub const ACTIVITY: &[u8] = b"activity";
    /// Entry point: an `Application` subclass.
    pub const APPLICATION: &[u8] = b"application";
    /// The app's package name.
    pub const PACKAGE_NAME: &[u8] = b"package-name";
    /// The app's own version string.
    pub const VERSION: &[u8] = b"version";
    /// Shrink-map version the PAPK was built against (see the `compat` crate).
    pub const FRAMEWORK_MAP_VERSION: &[u8] = b"framework-map-version";
}

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

impl core::fmt::Display for PapkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::BadMagic => "magic bytes are not 'PAPK'",
            Self::UnsupportedVersion => "unsupported PAPK major version",
            Self::Truncated => "file is truncated or an offset points past the end",
            Self::MissingSection => "a required section (MANIFEST or CLASSES) is missing",
            Self::FrameworkVersionMissing => {
                "PAPK has no framework-map-version but the firmware requires one"
            }
            Self::FrameworkVersionMismatch => {
                "PAPK framework-map-version is incompatible with the firmware"
            }
        };
        f.write_str(s)
    }
}

// ── Public types ──────────────────────────────────────────────────────────────

/// Raw file header, as stored on disk.
///
/// Obtainable without full validation via [`FileHeader::parse`] (magic +
/// length check only — a dump tool can still show the header of a
/// future-versioned file), or from a validated [`Papk`] via
/// [`Papk::file_header`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileHeader {
    pub version_major: u16,
    pub version_minor: u16,
    pub section_count: u32,
    pub manifest_offset: u32,
    pub classes_offset: u32,
    /// Offset of the ASSETS section header; `0` = absent (v1.0 `reserved`).
    pub assets_offset: u32,
}

impl FileHeader {
    /// Parse the 24-byte file header from `data`.
    ///
    /// Checks the magic and the buffer length only — it does NOT enforce
    /// `version_major`, so callers (e.g. `papk-info`) can still dump a
    /// future-versioned file's header. Use [`Papk::parse`] for the
    /// version-checked entry point.
    pub fn parse(data: &[u8]) -> Result<Self, PapkError> {
        if data.len() < FILE_HEADER_LEN {
            return Err(PapkError::Truncated);
        }
        if &data[0..4] != MAGIC {
            return Err(PapkError::BadMagic);
        }
        Ok(Self {
            version_major: read_u16_le(data, 4),
            version_minor: read_u16_le(data, 6),
            section_count: read_u32_le(data, 8),
            manifest_offset: read_u32_le(data, 12),
            classes_offset: read_u32_le(data, 16),
            assets_offset: read_u32_le(data, 20),
        })
    }
}

/// A section header, as stored on disk (minus the always-zero `reserved`
/// trailing word).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionHeader {
    /// Section tag ([`TAG_MANIFEST`], [`TAG_CLASSES`], or [`TAG_ASSETS`]).
    pub tag: u32,
    /// Byte count of the section data, NOT including the header itself.
    pub length: u32,
    /// CRC32 of the section data; `0` = unchecked in v1.
    pub crc32: u32,
}

/// One class entry from the CLASSES section.
pub struct ClassEntry<'a> {
    /// JVM internal class name (e.g. `"helloworld/HelloWorld"`), UTF-8.
    pub name: &'a [u8],
    /// Raw `.class` file bytes, suitable for `pico_jvm::Jvm::load_class`.
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
        let fixed_end = self.pos.checked_add(12)?;
        if fixed_end > self.data.len() {
            return None;
        }
        let width = read_u16_le(self.data, self.pos);
        let height = read_u16_le(self.data, self.pos + 2);
        let cf = self.data[self.pos + 4];
        // self.data[self.pos + 5] is reserved0 (must be 0).
        let stride = read_u16_le(self.data, self.pos + 6);
        let data_size = read_u32_le(self.data, self.pos + 8) as usize;
        self.pos = fixed_end;
        // Pad up to a 4-byte boundary within the section before the data.
        self.pos = self.pos.checked_add(3)? & !3;
        let data_end = self.pos.checked_add(data_size)?;
        if data_end > self.data.len() {
            return None;
        }
        let data = &self.data[self.pos..data_end];
        self.pos = data_end;
        // Pad up to a 4-byte boundary within the section before the next record.
        self.pos = self.pos.checked_add(3)? & !3;
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
        // for the 16-byte section header. `checked_add` so a hostile offset
        // near the usize ceiling cannot wrap on 32-bit release builds.
        if manifest_offset
            .checked_add(SECTION_HEADER_LEN)
            .is_none_or(|end| end > data.len())
        {
            return Err(PapkError::Truncated);
        }
        if classes_offset
            .checked_add(SECTION_HEADER_LEN)
            .is_none_or(|end| end > data.len())
        {
            return Err(PapkError::Truncated);
        }
        if assets_offset != 0
            && assets_offset
                .checked_add(SECTION_HEADER_LEN)
                .is_none_or(|end| end > data.len())
        {
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
        Ok(section_at(self.data, self.manifest_offset, TAG_MANIFEST)?.1)
    }

    /// Returns the raw CLASSES section data slice (excluding the section header).
    fn classes_section_data(&self) -> Result<&'a [u8], PapkError> {
        Ok(section_at(self.data, self.classes_offset, TAG_CLASSES)?.1)
    }

    /// Returns the raw ASSETS section data slice (excluding the section header),
    /// or `None` if the file has no ASSETS section.
    fn assets_section_data(&self) -> Result<Option<&'a [u8]>, PapkError> {
        if self.assets_offset == 0 {
            return Ok(None);
        }
        section_at(self.data, self.assets_offset, TAG_ASSETS).map(|(_, d)| Some(d))
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Returns the parsed [`FileHeader`] of this (already validated) papk.
    pub fn file_header(&self) -> FileHeader {
        // `parse` verified length + magic, so this cannot fail.
        FileHeader {
            version_major: read_u16_le(self.data, 4),
            version_minor: read_u16_le(self.data, 6),
            section_count: read_u32_le(self.data, 8),
            manifest_offset: read_u32_le(self.data, 12),
            classes_offset: read_u32_le(self.data, 16),
            assets_offset: read_u32_le(self.data, 20),
        }
    }

    /// Returns the MANIFEST section header and its data slice.
    pub fn manifest_section(&self) -> Result<(SectionHeader, &'a [u8]), PapkError> {
        section_at(self.data, self.manifest_offset, TAG_MANIFEST)
    }

    /// Returns the CLASSES section header and its data slice.
    pub fn classes_section(&self) -> Result<(SectionHeader, &'a [u8]), PapkError> {
        section_at(self.data, self.classes_offset, TAG_CLASSES)
    }

    /// Returns the ASSETS section header and its data slice, or `None` if the
    /// papk has no ASSETS section.
    pub fn assets_section(&self) -> Result<Option<(SectionHeader, &'a [u8])>, PapkError> {
        if self.assets_offset == 0 {
            return Ok(None);
        }
        section_at(self.data, self.assets_offset, TAG_ASSETS).map(Some)
    }

    /// Returns the `main-class` value from the MANIFEST section, or `None` if
    /// the key is absent or its value is not valid UTF-8.
    pub fn main_class(&self) -> Option<&'a str> {
        self.manifest_value(keys::MAIN_CLASS)
    }

    /// Returns the `activity` value from the MANIFEST section, or `None` if
    /// the key is absent or its value is not valid UTF-8.
    pub fn activity(&self) -> Option<&'a str> {
        self.manifest_value(keys::ACTIVITY)
    }

    /// Returns the `application` value from the MANIFEST section, or `None` if
    /// the key is absent or its value is not valid UTF-8.
    pub fn application(&self) -> Option<&'a str> {
        self.manifest_value(keys::APPLICATION)
    }

    /// Returns the `framework-map-version` value from the MANIFEST section,
    /// or `None` if the key is absent (legacy PAPK) or not valid UTF-8.
    ///
    /// The value is a semver string like `"0.1.0"`; `"0.0.0"` is the sentinel
    /// emitted when the firmware and PAPK were both built against no shrink
    /// map (default behavior until a release cut introduces one).
    pub fn framework_map_version(&self) -> Option<&'a str> {
        self.manifest_value(keys::FRAMEWORK_MAP_VERSION)
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
    ///
    /// Returns `None` if the key is absent, its value is not valid UTF-8, or
    /// the MANIFEST section cannot be read.
    pub fn manifest_value(&self, target_key: &[u8]) -> Option<&'a str> {
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

    /// Returns the class count *declared* in the CLASSES section.
    ///
    /// [`ClassIter`] stops early (yields fewer entries) when the section is
    /// truncated mid-record; a dump tool can compare the yielded count with
    /// this declared count to surface the shortfall as an error.
    pub fn class_count(&self) -> Result<u32, PapkError> {
        let cdata = self.classes_section_data()?;
        if cdata.len() < 4 {
            return Err(PapkError::Truncated);
        }
        Ok(read_u32_le(cdata, 0))
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

    /// Returns the asset count *declared* in the ASSETS section, or `None`
    /// if the papk has no ASSETS section.
    ///
    /// See [`Papk::class_count`] for the yielded-vs-declared rationale.
    pub fn asset_count(&self) -> Result<Option<u32>, PapkError> {
        let Some(adata) = self.assets_section_data()? else {
            return Ok(None);
        };
        if adata.len() < 4 {
            return Err(PapkError::Truncated);
        }
        Ok(Some(read_u32_le(adata, 0)))
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
    let after_len = pos.checked_add(2)?;
    if after_len > buf.len() {
        return None;
    }
    let len = read_u16_le(buf, *pos) as usize;
    *pos = after_len;
    let end = pos.checked_add(len)?;
    if end > buf.len() {
        return None;
    }
    let slice = &buf[*pos..end];
    *pos = end;
    Some(slice)
}

/// Reads a length-prefixed byte slice using a `u32` length prefix.
/// Advances `pos` past the length and data. Returns `None` if truncated.
fn read_bytes_u32<'a>(buf: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    let after_len = pos.checked_add(4)?;
    if after_len > buf.len() {
        return None;
    }
    let len = read_u32_le(buf, *pos) as usize;
    *pos = after_len;
    let end = pos.checked_add(len)?;
    if end > buf.len() {
        return None;
    }
    let slice = &buf[*pos..end];
    *pos = end;
    Some(slice)
}

/// Returns the parsed [`SectionHeader`] and data slice for the section at
/// `section_offset`, verifying that its `tag` matches `expected_tag`.
fn section_at(
    file: &[u8],
    section_offset: usize,
    expected_tag: u32,
) -> Result<(SectionHeader, &[u8]), PapkError> {
    let header_end = section_offset
        .checked_add(SECTION_HEADER_LEN)
        .ok_or(PapkError::Truncated)?;
    if header_end > file.len() {
        return Err(PapkError::Truncated);
    }
    let tag = read_u32_le(file, section_offset);
    if tag != expected_tag {
        return Err(PapkError::MissingSection);
    }
    let length = read_u32_le(file, section_offset + 4);
    let crc32 = read_u32_le(file, section_offset + 8);
    let data_start = header_end;
    let data_end = data_start
        .checked_add(length as usize)
        .ok_or(PapkError::Truncated)?;
    if data_end > file.len() {
        return Err(PapkError::Truncated);
    }
    Ok((
        SectionHeader { tag, length, crc32 },
        &file[data_start..data_end],
    ))
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
    // Moved verbatim from jvm/src/apk.rs — keep the fixture builder
    // byte-level and clippy-quiet rather than restructured.
    #[allow(clippy::type_complexity, clippy::manual_is_multiple_of)]
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

    // ── New API surface (papk-format additions over jvm/src/apk.rs) ─────

    #[test]
    fn file_header_parse_reads_all_fields() {
        let papk = build_papk_with_assets(&[("x.png", 1, 1, 18, 0, &[0, 0, 0, 0])]);
        let hdr = FileHeader::parse(&papk).unwrap();
        assert_eq!(hdr.version_major, 1);
        assert_eq!(hdr.version_minor, 1);
        assert_eq!(hdr.section_count, 3);
        assert_eq!(hdr.manifest_offset, 24);
        assert_eq!(hdr.classes_offset, 40);
        assert_ne!(hdr.assets_offset, 0);
    }

    #[test]
    fn file_header_parse_does_not_enforce_version_major() {
        // A future-major file: FileHeader::parse still dumps the header,
        // while the full parser refuses it.
        let mut papk = build_test_papk("foo/Bar", &[]);
        papk[4..6].copy_from_slice(&2u16.to_le_bytes());
        let hdr = FileHeader::parse(&papk).unwrap();
        assert_eq!(hdr.version_major, 2);
        assert!(matches!(
            Papk::parse(&papk),
            Err(PapkError::UnsupportedVersion)
        ));
    }

    #[test]
    fn file_header_parse_checks_magic_and_length_only() {
        assert_eq!(FileHeader::parse(&[]), Err(PapkError::Truncated));
        assert_eq!(FileHeader::parse(&[0u8; 23]), Err(PapkError::Truncated));
        let mut papk = build_test_papk("foo/Bar", &[]);
        papk[0] = b'X';
        assert_eq!(FileHeader::parse(&papk), Err(PapkError::BadMagic));
    }

    #[test]
    fn file_header_accessor_matches_standalone_parse() {
        let papk = build_test_papk("foo/Bar", &[("foo/Bar", b"data")]);
        let p = Papk::parse(&papk).unwrap();
        assert_eq!(p.file_header(), FileHeader::parse(&papk).unwrap());
        assert_eq!(p.file_header().assets_offset, 0);
        assert_eq!(p.file_header().section_count, 2);
    }

    #[test]
    fn section_accessors_return_header_and_data() {
        let papk = build_papk_with_assets(&[("x.png", 1, 1, 18, 0, &[1, 2, 3, 4])]);
        let p = Papk::parse(&papk).unwrap();

        let (mh, mdata) = p.manifest_section().unwrap();
        assert_eq!(mh.tag, TAG_MANIFEST);
        assert_eq!(mh.length as usize, mdata.len());
        assert_eq!(mh.crc32, 0);

        let (ch, cdata) = p.classes_section().unwrap();
        assert_eq!(ch.tag, TAG_CLASSES);
        assert_eq!(ch.length as usize, cdata.len());
        assert_eq!(cdata, 0u32.to_le_bytes()); // zero classes

        let (ah, adata) = p.assets_section().unwrap().expect("ASST present");
        assert_eq!(ah.tag, TAG_ASSETS);
        assert_eq!(ah.length as usize, adata.len());
    }

    #[test]
    fn assets_section_accessor_none_for_legacy_papk() {
        let papk = build_test_papk("foo/Bar", &[]);
        let p = Papk::parse(&papk).unwrap();
        assert!(p.assets_section().unwrap().is_none());
    }

    #[test]
    fn manifest_value_is_public_generic_lookup() {
        let papk = build_papk_with_manifest(&[("main-class", "x/Y"), ("custom-key", "custom-v")]);
        let p = parse_leaked(papk);
        assert_eq!(p.manifest_value(b"custom-key"), Some("custom-v"));
        assert_eq!(p.manifest_value(keys::MAIN_CLASS), Some("x/Y"));
        assert_eq!(p.manifest_value(b"absent"), None);
    }

    #[test]
    fn class_count_reports_declared_count() {
        let papk = build_test_papk("foo/Bar", &[("foo/Bar", b"data"), ("lib/Util", b"x")]);
        let p = Papk::parse(&papk).unwrap();
        assert_eq!(p.class_count(), Ok(2));
        assert_eq!(
            p.classes().unwrap().count() as u32,
            p.class_count().unwrap()
        );
    }

    #[test]
    fn class_count_exposes_truncation_shortfall() {
        // Corrupt the declared count upward: the iterator stops early once the
        // section data runs out, and a dump tool can flag yielded < declared —
        // the diagnostic papk-info's old parse_classes gave via an Err.
        let mut papk = build_test_papk("foo/Bar", &[("foo/Bar", b"data")]);
        let cdata_start = {
            let p = Papk::parse(&papk).unwrap();
            p.classes_offset + SECTION_HEADER_LEN
        };
        papk[cdata_start..cdata_start + 4].copy_from_slice(&5u32.to_le_bytes());
        let p = Papk::parse(&papk).unwrap();
        assert_eq!(p.class_count(), Ok(5));
        let yielded = p.classes().unwrap().count();
        assert_eq!(yielded, 1);
        assert!((yielded as u32) < p.class_count().unwrap());
    }

    #[test]
    fn asset_count_reports_declared_or_none() {
        let legacy = build_test_papk("foo/Bar", &[]);
        assert_eq!(Papk::parse(&legacy).unwrap().asset_count(), Ok(None));

        let papk = build_papk_with_assets(&[
            ("a.png", 1, 1, 18, 0, &[0, 0]),
            ("b.png", 1, 1, 18, 0, &[1, 1]),
        ]);
        let p = Papk::parse(&papk).unwrap();
        assert_eq!(p.asset_count(), Ok(Some(2)));
        assert_eq!(
            p.assets().unwrap().unwrap().count() as u32,
            p.asset_count().unwrap().unwrap()
        );
    }

    #[test]
    fn hostile_section_offsets_rejected_not_wrapped() {
        // u32::MAX offsets must fail cleanly via checked_add — on a 32-bit
        // target the old `offset + SECTION_HEADER_LEN` could wrap in release.
        for field in [12usize, 16, 20] {
            let mut papk = build_test_papk("foo/Bar", &[]);
            papk[field..field + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(
                matches!(Papk::parse(&papk), Err(PapkError::Truncated)),
                "offset field at {field} must be rejected"
            );
        }
    }

    #[test]
    fn errors_have_display_impls() {
        use alloc::string::ToString;
        for e in [
            PapkError::BadMagic,
            PapkError::UnsupportedVersion,
            PapkError::Truncated,
            PapkError::MissingSection,
            PapkError::FrameworkVersionMissing,
            PapkError::FrameworkVersionMismatch,
        ] {
            assert!(!e.to_string().is_empty());
        }
    }
}
