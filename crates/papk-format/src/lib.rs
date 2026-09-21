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
//! by a MANIFEST section, a CLASSES section, (optionally, in v1.1+) an
//! ASSETS section and (optionally, in v1.2+) a RESOURCES section.  All
//! integers are little-endian.
//!
//! **Every section starts on a 4-byte boundary**, zero-padded from the end of
//! the previous one. The reader takes each section's offset from the file
//! header and so never depends on this, but a *writer* must honour it: asset
//! pixel data is padded to 4 bytes relative to its section, which only makes
//! it 4-byte aligned in the file — and hence at its mapped flash address —
//! if the section itself is. Alignment is not cosmetic here. LVGL reads
//! bundled pixels in place out of XIP flash through a `const uint16_t *`, and
//! a Cortex-M0+ answers an unaligned halfword load with a HardFault rather
//! than a slow path, so a misplaced ASSETS section takes an RP2040 down the
//! first time it draws a scaled image (`docs/bugs-rp2040-imagedemo-2026-09-15.md`).
//! Sections packed back to back, as they were before 2026-09-15, put ASSETS
//! wherever the sum of the class files happened to land.
//!
//! ```text
//! File header (24 bytes):
//!   [0..4]   magic:           b"PAPK"
//!   [4..2]   version_major:   u16 LE  (currently 1)
//!   [6..2]   version_minor:   u16 LE  (0 = no assets, 1 = ASSETS section may exist,
//!                                      2 = the header has the v1.2 extension)
//!   [8..4]   section_count:   u32 LE
//!   [12..4]  manifest_offset: u32 LE  (offset to MANIFEST section header)
//!   [16..4]  classes_offset:  u32 LE  (offset to CLASSES section header)
//!   [20..4]  assets_offset:   u32 LE  (offset to ASSETS section header, 0 = absent)
//!
//! File header extension (4 more bytes, present iff version_minor >= 2):
//!   [24..4]  resources_offset: u32 LE (offset to RESOURCES section header, 0 = absent)
//!
//! The writer emits the extension — and minor 2 — only for a PAPK that has a
//! RESOURCES section, so every other PAPK stays byte-identical to v1.1. A
//! v1.1 reader handed a v1.2 file takes its three offsets from the same slots
//! as ever and simply never sees the resources. A reader must gate on
//! `version_minor` before it looks at [24..28]: in a v1.1 file those bytes
//! are the MANIFEST section's tag.
//!
//! Section header (16 bytes):
//!   [0..4]   tag:      u32 LE  ("MANI", "CLSS", "ASST" or "RESR")
//!   [4..4]   length:   u32 LE  (byte count of section data, NOT including header)
//!   [8..4]   crc32:    u32 LE  (0 = unchecked in v1)
//!   [12..4]  reserved: u32 LE  = 0
//!
//! MANIFEST section data:
//!   Sequence of [u16 key_len][key][u16 val_len][val] entries (UTF-8, no NUL).
//!   Walk until `length` bytes consumed. The well-known keys are in [`keys`];
//!   the writer emits the entry-point key, `package-name`, `version`,
//!   `framework-map-version`, then — only when set — `version-code`, `label`
//!   and `icon`, then any extra entries. Readers must tolerate absent keys:
//!   a PAPK packed before a key existed simply lacks it.
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
//!
//! RESOURCES section data (v1.2+):
//!   see the [`res`] module.
//!
//! Between sections:
//!   [pad 0..3 zero bytes so the next section header starts 4-byte aligned]
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

pub mod flash_image;
pub mod res;

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
/// manifest key / ASSETS section were introduced) for a PAPK without a
/// RESOURCES section.
pub const VERSION_MINOR: u16 = 1;
/// `version_minor` of a PAPK whose file header carries the v1.2 extension
/// (`resources_offset`). Emitted only when there is a RESOURCES section.
pub const VERSION_MINOR_RESOURCES: u16 = 2;
/// Byte length of the fixed file header.
pub const FILE_HEADER_LEN: usize = 24;
/// Byte length of the file header with the v1.2 extension.
pub const FILE_HEADER_LEN_V1_2: usize = 28;
/// Byte length of each section header.
pub const SECTION_HEADER_LEN: usize = 16;
/// Section tag for the MANIFEST section (`b"MANI"` as a LE u32).
pub const TAG_MANIFEST: u32 = u32::from_le_bytes(*b"MANI");
/// Section tag for the CLASSES section (`b"CLSS"` as a LE u32).
pub const TAG_CLASSES: u32 = u32::from_le_bytes(*b"CLSS");
/// Section tag for the ASSETS section (`b"ASST"` as a LE u32).
pub const TAG_ASSETS: u32 = u32::from_le_bytes(*b"ASST");
/// Section tag for the RESOURCES section (`b"RESR"` as a LE u32).
pub const TAG_RESOURCES: u32 = u32::from_le_bytes(*b"RESR");

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
    /// Monotonic integer version, decimal text (`"3"`); a newer build of the
    /// same package carries a greater code. Absent on old PAPKs (read as 1).
    pub const VERSION_CODE: &[u8] = b"version-code";
    /// Human-readable app name a launcher shows. Absent means "use the
    /// package name".
    pub const LABEL: &[u8] = b"label";
    /// Name of the ASSETS entry holding the app icon (e.g. `"icon.png"`).
    pub const ICON: &[u8] = b"icon";
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
    /// Offset of the RESOURCES section header; `0` = absent, and always `0`
    /// below v1.2, whose header has no such field.
    pub resources_offset: u32,
}

/// `resources_offset` of a header whose magic and 24-byte length are already
/// checked: `0` unless the file declares the v1.2 extension and is long
/// enough to hold it.
fn read_resources_offset(data: &[u8]) -> u32 {
    if read_u16_le(data, 6) >= VERSION_MINOR_RESOURCES && data.len() >= FILE_HEADER_LEN_V1_2 {
        read_u32_le(data, 24)
    } else {
        0
    }
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
            resources_offset: read_resources_offset(data),
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
    /// Offset to the RESOURCES section header, or 0 if absent (any papk
    /// below v1.2, and every app without a `res/` tree).
    resources_offset: usize,
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
        let resources_offset = read_resources_offset(data) as usize;

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
        if resources_offset != 0
            && resources_offset
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
            resources_offset,
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
            resources_offset: self.resources_offset as u32,
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

    /// Returns the RESOURCES section header and its data slice, or `None` if
    /// the papk has no RESOURCES section.
    pub fn resources_section(&self) -> Result<Option<(SectionHeader, &'a [u8])>, PapkError> {
        if self.resources_offset == 0 {
            return Ok(None);
        }
        section_at(self.data, self.resources_offset, TAG_RESOURCES).map(Some)
    }

    /// The app's compiled resource table, or `None` if it has no `res/` tree.
    pub fn resources(&self) -> Result<Option<res::ResTable<'a>>, PapkError> {
        match self.resources_section()? {
            Some((_, data)) => res::ResTable::parse(data).map(Some),
            None => Ok(None),
        }
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

    /// Returns the `package-name` value — the app's identity, taken from the
    /// manifest's `package=` attribute at pack time.
    pub fn package_name(&self) -> Option<&'a str> {
        self.manifest_value(keys::PACKAGE_NAME)
    }

    /// Returns the `version` value, the human-readable version name.
    pub fn version(&self) -> Option<&'a str> {
        self.manifest_value(keys::VERSION)
    }

    /// Returns the `label` value — the display name a launcher shows — or
    /// `None` on a PAPK packed without one (show the package name instead).
    pub fn label(&self) -> Option<&'a str> {
        self.manifest_value(keys::LABEL)
    }

    /// Returns the `icon` value, the name of the ASSETS entry holding the
    /// app icon, or `None` when the app declares none.
    pub fn icon(&self) -> Option<&'a str> {
        self.manifest_value(keys::ICON)
    }

    /// Returns the `version-code` value parsed as a decimal integer, or
    /// `None` when the key is absent or unparseable. Consumers treat a
    /// missing code as 1, the default the packer writes for a manifest that
    /// does not set one.
    pub fn version_code(&self) -> Option<u32> {
        self.manifest_value(keys::VERSION_CODE)?.parse().ok()
    }

    /// Verify this PAPK's shrink-map version is compatible with the firmware.
    ///
    /// Delegates to [`compat::check`] so the host-side `pdb install`
    /// pre-flight and this device-side load-time check share one rule
    /// implementation. See `compat` crate docs for the table.
    pub fn verify_compat(&self, firmware_version: &str) -> Result<(), PapkError> {
        compat::check(self.framework_map_version(), firmware_version).map_err(|e| match e {
            compat::CompatError::Missing => PapkError::FrameworkVersionMissing,
            compat::CompatError::Mismatch
            | compat::CompatError::BadVersion
            | compat::CompatError::PredatesMemberShrink => PapkError::FrameworkVersionMismatch,
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
mod tests;
