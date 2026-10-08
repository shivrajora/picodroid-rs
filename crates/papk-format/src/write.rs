// SPDX-License-Identifier: GPL-3.0-only
//! PAPK writer (`write` feature, alloc-only).
//!
//! [`PapkBuilder`] owns "the manifest shape":
//!
//! - file header (28 bytes) with `version_major = 2`, `version_minor = 0`
//!   always;
//! - the CLASSES section is a `class-link` class section: every class is
//!   linked here (its link table built and its name checked against the
//!   name it was added under), and the sorted class index is written;
//! - section order MANI, CLSS, then ASST only when at least one asset is
//!   present (otherwise `assets_offset = 0` and `section_count = 2`);
//! - every section starts on a 4-byte boundary, zero-padded from the end of
//!   the previous one ([`section_after`]) — the reader takes each section's
//!   offset from the file header, so the padding is invisible to it;
//! - manifest key order: the entry-point key (`main-class` / `activity` /
//!   `application`), `package-name`, `version`, `framework-map-version`,
//!   then `version-code`, `label` and `icon` — each only when the spec sets
//!   it, so a spec that sets none is byte-identical to the historical output —
//!   then any extra entries in insertion order;
//! - asset records padded with zero bytes to 4-byte boundaries before the
//!   pixel data and before the next record;
//! - `crc32` and `reserved` words written as 0.
//!
//! Where the old writer silently truncated oversized lengths (`as u16` /
//! `as u32`), the builder returns a [`BuildError`] instead — byte-identical
//! output for every input the old writer handled correctly, an error for
//! inputs that would have produced a corrupt papk.

use alloc::vec::Vec;

use crate::{
    keys, FILE_HEADER_LEN, MAGIC, SECTION_HEADER_LEN, TAG_ASSETS, TAG_CLASSES, TAG_MANIFEST,
    TAG_RESOURCES, VERSION_MAJOR, VERSION_MINOR,
};

/// The app's entry point — exactly one of the three manifest entry keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryPoint<'a> {
    /// `main-class`: a class with `public static void main(String[])`.
    MainClass(&'a str),
    /// `activity`: an `Activity` subclass.
    Activity(&'a str),
    /// `application`: an `Application` subclass.
    Application(&'a str),
}

impl<'a> EntryPoint<'a> {
    fn key_and_value(self) -> (&'static [u8], &'a str) {
        match self {
            Self::MainClass(v) => (keys::MAIN_CLASS, v),
            Self::Activity(v) => (keys::ACTIVITY, v),
            Self::Application(v) => (keys::APPLICATION, v),
        }
    }
}

/// The fixed part of every PAPK manifest, plus the optional identity keys.
#[derive(Debug, Clone, Copy)]
pub struct ManifestSpec<'a> {
    pub entry: EntryPoint<'a>,
    pub package_name: &'a str,
    pub version: &'a str,
    pub framework_map_version: &'a str,
    /// `version-code`, written as decimal text when set.
    pub version_code: Option<u32>,
    /// `label`, the display name; absent means "use the package name".
    pub label: Option<&'a str>,
    /// `icon`, the name of an ASSETS entry; absent means no icon.
    pub icon: Option<&'a str>,
}

/// One asset for the ASSETS section.
#[derive(Debug, Clone, Copy)]
pub struct AssetSpec<'a> {
    pub name: &'a str,
    pub width: u16,
    pub height: u16,
    /// Opaque LVGL color-format byte (`lv_color_format_t`); papk-format does
    /// NOT know LVGL — `LV_COLOR_FORMAT_RGB565` stays in papk-pack, `cf_label`
    /// stays in papk-info (accepted duplication that can drift from vendored
    /// LVGL; both keep their own drift guards).
    pub cf: u8,
    /// Bytes per row; `0` = derive from `width` + `cf` (papk-pack always
    /// writes 0).
    pub stride: u16,
    pub data: &'a [u8],
}

/// Errors from [`PapkBuilder::build`]. These replace the old writer's silent
/// `as u16` / `as u32` length truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    /// A class/asset name or manifest key exceeds the u16 length prefix.
    NameTooLong,
    /// A manifest value or class/asset data blob exceeds its length prefix
    /// (u16 for manifest values, u32 for data blobs).
    ValueTooLong,
    /// More classes or assets than the u32 count field can hold.
    TooManyEntries,
    /// A section or offset exceeds the u32 range of the file header fields.
    TooLarge,
    /// A class could not be linked (malformed, over 64 KB, a name that is
    /// not UTF-8, two classes with one name or one hash), or spells a name
    /// other than the one it was added under.
    Link(class_link::LinkError),
}

impl core::fmt::Display for BuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::NameTooLong => "name or manifest key exceeds the u16 length prefix",
            Self::ValueTooLong => "value or data blob exceeds its length prefix",
            Self::TooManyEntries => "too many classes or assets for the u32 count field",
            Self::TooLarge => "section offset or length exceeds the u32 header fields",
            Self::Link(e) => return write!(f, "class section: {e}"),
        };
        f.write_str(s)
    }
}

/// Builder for a PAPK file. Borrows all inputs (`'a`); allocates only the
/// output buffer and the entry bookkeeping vectors.
pub struct PapkBuilder<'a> {
    manifest: ManifestSpec<'a>,
    extras: Vec<(&'a str, &'a str)>,
    classes: Vec<(&'a str, &'a [u8])>,
    assets: Vec<AssetSpec<'a>>,
    resources: Option<&'a [u8]>,
}

impl<'a> PapkBuilder<'a> {
    pub fn new(manifest: ManifestSpec<'a>) -> Self {
        Self {
            manifest,
            extras: Vec::new(),
            classes: Vec::new(),
            assets: Vec::new(),
            resources: None,
        }
    }

    /// Set the RESOURCES section data (built by
    /// [`crate::res::ResTableBuilder`]). An empty slice means no section.
    pub fn resources(&mut self, data: &'a [u8]) -> &mut Self {
        self.resources = (!data.is_empty()).then_some(data);
        self
    }

    /// Append an extra manifest entry (future keys). Emitted after the fixed
    /// four, in insertion order.
    pub fn manifest_entry(&mut self, key: &'a str, value: &'a str) -> &mut Self {
        self.extras.push((key, value));
        self
    }

    /// Append a class. `jvm_name` is the JVM internal name (forward slashes,
    /// no `.class` suffix, e.g. `"helloworld/HelloWorld"`); it must be the
    /// name the class file itself spells, or [`build`](Self::build) refuses
    /// the class. Emission order is insertion order — sort beforehand for
    /// deterministic output (papk-pack sorts by name).
    pub fn class(&mut self, jvm_name: &'a str, bytes: &'a [u8]) -> &mut Self {
        self.classes.push((jvm_name, bytes));
        self
    }

    /// Append an asset. Emission order is insertion order (papk-pack sorts by
    /// name).
    pub fn asset(&mut self, asset: AssetSpec<'a>) -> &mut Self {
        self.assets.push(asset);
        self
    }

    /// Serialize the PAPK file (see the module docs for the layout contract).
    pub fn build(&self) -> Result<Vec<u8>, BuildError> {
        let manifest_data = self.build_manifest_data()?;
        let classes_data = self.build_classes_data()?;
        let assets_data = if self.assets.is_empty() {
            Vec::new()
        } else {
            self.build_assets_data()?
        };

        let manifest_len = u32::try_from(manifest_data.len()).map_err(|_| BuildError::TooLarge)?;
        let classes_len = u32::try_from(classes_data.len()).map_err(|_| BuildError::TooLarge)?;
        let assets_len = u32::try_from(assets_data.len()).map_err(|_| BuildError::TooLarge)?;

        // MANIFEST starts right after the file header.
        let manifest_offset = FILE_HEADER_LEN as u32;
        let classes_offset = section_after(manifest_offset, manifest_len)?;
        // 0 means "no ASSETS section". Legacy parsers see zero in the slot
        // they formerly read as `reserved` and behave unchanged.
        let assets_offset = if self.assets.is_empty() {
            0u32
        } else {
            section_after(classes_offset, classes_len)?
        };
        let resources_offset = match self.resources {
            None => 0u32,
            Some(_) if self.assets.is_empty() => section_after(classes_offset, classes_len)?,
            Some(_) => section_after(assets_offset, assets_len)?,
        };
        let section_count =
            2 + u32::from(!self.assets.is_empty()) + u32::from(self.resources.is_some());

        let mut file = Vec::new();

        // File header (28 bytes)
        file.extend_from_slice(MAGIC);
        file.extend_from_slice(&VERSION_MAJOR.to_le_bytes());
        file.extend_from_slice(&VERSION_MINOR.to_le_bytes());
        file.extend_from_slice(&section_count.to_le_bytes());
        file.extend_from_slice(&manifest_offset.to_le_bytes());
        file.extend_from_slice(&classes_offset.to_le_bytes());
        file.extend_from_slice(&assets_offset.to_le_bytes());
        file.extend_from_slice(&resources_offset.to_le_bytes());
        debug_assert_eq!(file.len(), FILE_HEADER_LEN);

        // MANIFEST section
        push_section_header(&mut file, TAG_MANIFEST, manifest_len);
        file.extend_from_slice(&manifest_data);

        // CLASSES section
        pad_to(&mut file, classes_offset);
        push_section_header(&mut file, TAG_CLASSES, classes_len);
        file.extend_from_slice(&classes_data);

        // ASSETS section (optional)
        if !self.assets.is_empty() {
            pad_to(&mut file, assets_offset);
            push_section_header(&mut file, TAG_ASSETS, assets_len);
            file.extend_from_slice(&assets_data);
        }

        // RESOURCES section (optional)
        if let Some(resources) = self.resources {
            let resources_len = u32::try_from(resources.len()).map_err(|_| BuildError::TooLarge)?;
            pad_to(&mut file, resources_offset);
            push_section_header(&mut file, TAG_RESOURCES, resources_len);
            file.extend_from_slice(resources);
        }

        Ok(file)
    }

    /// Build the MANIFEST section data (key/value pairs).
    fn build_manifest_data(&self) -> Result<Vec<u8>, BuildError> {
        let mut data = Vec::new();
        let (entry_key, entry_value) = self.manifest.entry.key_and_value();
        push_bytes_u16(&mut data, entry_key, BuildError::NameTooLong)?;
        push_bytes_u16(&mut data, entry_value.as_bytes(), BuildError::ValueTooLong)?;
        push_bytes_u16(&mut data, keys::PACKAGE_NAME, BuildError::NameTooLong)?;
        push_bytes_u16(
            &mut data,
            self.manifest.package_name.as_bytes(),
            BuildError::ValueTooLong,
        )?;
        push_bytes_u16(&mut data, keys::VERSION, BuildError::NameTooLong)?;
        push_bytes_u16(
            &mut data,
            self.manifest.version.as_bytes(),
            BuildError::ValueTooLong,
        )?;
        push_bytes_u16(
            &mut data,
            keys::FRAMEWORK_MAP_VERSION,
            BuildError::NameTooLong,
        )?;
        push_bytes_u16(
            &mut data,
            self.manifest.framework_map_version.as_bytes(),
            BuildError::ValueTooLong,
        )?;
        if let Some(code) = self.manifest.version_code {
            let text = alloc::string::ToString::to_string(&code);
            push_bytes_u16(&mut data, keys::VERSION_CODE, BuildError::NameTooLong)?;
            push_bytes_u16(&mut data, text.as_bytes(), BuildError::ValueTooLong)?;
        }
        if let Some(label) = self.manifest.label {
            push_bytes_u16(&mut data, keys::LABEL, BuildError::NameTooLong)?;
            push_bytes_u16(&mut data, label.as_bytes(), BuildError::ValueTooLong)?;
        }
        if let Some(icon) = self.manifest.icon {
            push_bytes_u16(&mut data, keys::ICON, BuildError::NameTooLong)?;
            push_bytes_u16(&mut data, icon.as_bytes(), BuildError::ValueTooLong)?;
        }
        for (k, v) in &self.extras {
            push_bytes_u16(&mut data, k.as_bytes(), BuildError::NameTooLong)?;
            push_bytes_u16(&mut data, v.as_bytes(), BuildError::ValueTooLong)?;
        }
        Ok(data)
    }

    /// Build the CLASSES section data: the classes linked, indexed and laid
    /// out by `class-link`, each checked to spell the name it was added
    /// under.
    fn build_classes_data(&self) -> Result<Vec<u8>, BuildError> {
        let named: Vec<(&[u8], &[u8])> = self
            .classes
            .iter()
            .map(|(name, bytes)| (name.as_bytes(), *bytes))
            .collect();
        class_link::build::build_section_named(&named).map_err(BuildError::Link)
    }

    /// Build the ASSETS section data. The data of each asset starts on a
    /// 4-byte boundary within the section and each record is followed by 0..3
    /// pad bytes so the next record also starts on a 4-byte boundary.
    fn build_assets_data(&self) -> Result<Vec<u8>, BuildError> {
        let mut data = Vec::new();
        let count = u32::try_from(self.assets.len()).map_err(|_| BuildError::TooManyEntries)?;
        data.extend_from_slice(&count.to_le_bytes());
        for a in &self.assets {
            push_bytes_u16(&mut data, a.name.as_bytes(), BuildError::NameTooLong)?;
            data.extend_from_slice(&a.width.to_le_bytes());
            data.extend_from_slice(&a.height.to_le_bytes());
            data.push(a.cf);
            data.push(0); // reserved0
            data.extend_from_slice(&a.stride.to_le_bytes());
            let data_size = u32::try_from(a.data.len()).map_err(|_| BuildError::ValueTooLong)?;
            data.extend_from_slice(&data_size.to_le_bytes());
            // Pad to 4-byte boundary before the data.
            while !data.len().is_multiple_of(4) {
                data.push(0);
            }
            data.extend_from_slice(a.data);
            // Pad to 4-byte boundary before the next record.
            while !data.len().is_multiple_of(4) {
                data.push(0);
            }
        }
        Ok(data)
    }
}

/// Append `[u16 len][bytes]`, or `err` if `bytes` exceeds the u16 prefix.
fn push_bytes_u16(out: &mut Vec<u8>, bytes: &[u8], err: BuildError) -> Result<(), BuildError> {
    let len = u16::try_from(bytes.len()).map_err(|_| err)?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// Offset of the section that follows the one starting at `offset` with
/// `data_len` bytes of payload — rounded up to the next 4-byte boundary.
///
/// Every section starts 4-byte aligned so that the asset payloads inside
/// ASSETS, which are padded to 4 bytes *relative to their section*, are
/// 4-byte aligned in the file as well — and so at their mapped address,
/// since a papk is placed on a flash sector boundary. LVGL reads pixels
/// straight out of XIP flash through a `const uint16_t *`; on Cortex-M0+ an
/// unaligned halfword load is a HardFault, not a slow path, so an ASSETS
/// section landing on an odd offset crashed the RP2040 board the moment a
/// scaled image was drawn (`docs/bugs-rp2040-imagedemo-2026-09-15.md`).
fn section_after(offset: u32, data_len: u32) -> Result<u32, BuildError> {
    offset
        .checked_add(SECTION_HEADER_LEN as u32)
        .and_then(|v| v.checked_add(data_len))
        .and_then(|v| v.checked_add(3))
        .map(|v| v & !3)
        .ok_or(BuildError::TooLarge)
}

/// Zero-fill `out` up to `offset` (the alignment padding between sections).
fn pad_to(out: &mut Vec<u8>, offset: u32) {
    out.resize(offset as usize, 0);
}

/// Append a 16-byte section header (`crc32`/`reserved` = 0).
fn push_section_header(out: &mut Vec<u8>, tag: u32, data_len: u32) {
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // crc32: unchecked in v1
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Papk;
    use alloc::string::String;

    /// The golden fixture's class: a real, linkable class file (a fake blob
    /// no longer packs — every class is linked on the way in).
    static MAIN: &[u8] = include_bytes!("../tests/fixtures/Main.class");
    const MAIN_NAME: &str = "fixture/Main";

    fn spec() -> ManifestSpec<'static> {
        ManifestSpec {
            entry: EntryPoint::MainClass(MAIN_NAME),
            package_name: "t",
            version: "1.0",
            framework_map_version: "0.0.0",
            version_code: None,
            label: None,
            icon: None,
        }
    }

    /// The header is 28 bytes with or without a RESOURCES section; the
    /// section's presence shows only in `resources_offset` and the count.
    #[test]
    fn resources_section_round_trips() {
        let mut table = crate::res::ResTableBuilder::new();
        let hello = table.push_string("Hello").unwrap();
        let main = table.push_layout(alloc::vec![7, 8, 9]).unwrap();
        let table = table.build().unwrap();

        for with_asset in [false, true] {
            let mut plain = PapkBuilder::new(spec());
            plain.class(MAIN_NAME, MAIN);
            let mut b = PapkBuilder::new(spec());
            b.class(MAIN_NAME, MAIN);
            if with_asset {
                for b in [&mut plain, &mut b] {
                    b.asset(AssetSpec {
                        name: "a.png",
                        width: 1,
                        height: 1,
                        cf: 0x12,
                        stride: 0,
                        data: &[1, 2],
                    });
                }
            }
            b.resources(&table);
            // An empty table is no section at all.
            plain.resources(&[]);

            let plain = plain.build().unwrap();
            assert_eq!(&plain[4..6], &VERSION_MAJOR.to_le_bytes());
            assert_eq!(&plain[6..8], &VERSION_MINOR.to_le_bytes());
            assert_eq!(&plain[12..16], &(FILE_HEADER_LEN as u32).to_le_bytes());
            let p = Papk::parse(&plain).unwrap();
            assert!(p.resources().unwrap().is_none());
            assert_eq!(p.file_header().resources_offset, 0);
            assert_eq!(p.file_header().section_count, 2 + u32::from(with_asset));
            assert!(crate::validate_structure(&plain).is_ok());

            let file = b.build().unwrap();
            let p = Papk::parse(&file).unwrap();
            let h = p.file_header();
            assert_eq!(h.version_minor, VERSION_MINOR);
            assert_eq!(h.manifest_offset as usize, FILE_HEADER_LEN);
            assert_eq!(h.section_count, 3 + u32::from(with_asset));
            assert_eq!(h.resources_offset % 4, 0);
            assert_eq!(p.classes().unwrap().count(), 1);
            assert_eq!(p.assets().unwrap().is_some(), with_asset);
            let (sh, data) = p.resources_section().unwrap().unwrap();
            assert_eq!(sh.tag, TAG_RESOURCES);
            assert_eq!(data, &table[..]);
            let t = p.resources().unwrap().unwrap();
            assert_eq!(t.string(hello), Some(&b"Hello"[..]));
            assert_eq!(t.layout(main).unwrap().word(2), Some(9));
            assert!(crate::validate_structure(&file).is_ok());
        }
    }

    /// The bug this guards: asset payloads are padded to 4 bytes *within*
    /// their section, and `lib.rs` asserted exactly that — on a fixture whose
    /// sections happened to start aligned anyway. A section is any length, so
    /// ASSETS landed wherever CLASSES ended: `imagedemo`'s pixels sat at an
    /// odd flash address and the RP2040 HardFaulted on LVGL's first
    /// `uint16_t` read of them. Manifest values of every length here move
    /// the sections that follow; absolute file offsets asserted.
    #[test]
    fn sections_and_asset_data_are_4_byte_aligned_in_the_file() {
        for value_len in 1..=8usize {
            let version = alloc::string::String::from_utf8(alloc::vec![b'7'; value_len]).unwrap();
            let mut spec = spec();
            spec.version = &version;
            let pixels: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF];
            let mut b = PapkBuilder::new(spec);
            b.class(MAIN_NAME, MAIN);
            // An odd-length name moves the payload within the section too.
            b.asset(AssetSpec {
                name: "logo.png",
                width: 1,
                height: 1,
                cf: 0x12,
                stride: 0,
                data: pixels,
            });
            let file = b.build().unwrap();

            let hdr = Papk::parse(&file).unwrap().file_header();
            assert_eq!(
                hdr.classes_offset % 4,
                0,
                "classes_offset misaligned for value_len {value_len}"
            );
            assert_eq!(
                hdr.assets_offset % 4,
                0,
                "assets_offset misaligned for value_len {value_len}"
            );

            let p = Papk::parse(&file).unwrap();
            let entry = p.assets().unwrap().unwrap().next().unwrap();
            let file_offset = (entry.data.as_ptr() as usize).wrapping_sub(file.as_ptr() as usize);
            assert_eq!(
                file_offset % 4,
                0,
                "asset data at file offset {file_offset} (value_len {value_len}) is not 4-byte \
                 aligned — an unaligned uint16_t read on Cortex-M0+ is a HardFault"
            );
            assert_eq!(entry.data, pixels);
        }
    }

    #[test]
    fn identity_keys_are_emitted_in_order_after_the_fixed_four() {
        let mut spec = spec();
        spec.version_code = Some(7);
        spec.label = Some("Test App");
        spec.icon = Some("icon.png");
        let mut b = PapkBuilder::new(spec);
        b.manifest_entry("x-extra", "1");
        b.class(MAIN_NAME, MAIN);
        let bytes = b.build().unwrap();
        let p = Papk::parse(&bytes).unwrap();
        let keys: alloc::vec::Vec<&[u8]> = p.manifest().unwrap().map(|e| e.key).collect();
        assert_eq!(
            keys,
            [
                &b"main-class"[..],
                b"package-name",
                b"version",
                b"framework-map-version",
                b"version-code",
                b"label",
                b"icon",
                b"x-extra",
            ]
        );
        assert_eq!(p.version_code(), Some(7));
        assert_eq!(p.label(), Some("Test App"));
        assert_eq!(p.icon(), Some("icon.png"));
        assert_eq!(p.package_name(), Some("t"));
        assert_eq!(p.version(), Some("1.0"));
    }

    #[test]
    fn unset_identity_keys_are_absent_not_empty() {
        let mut b = PapkBuilder::new(spec());
        b.class(MAIN_NAME, MAIN);
        let bytes = b.build().unwrap();
        let p = Papk::parse(&bytes).unwrap();
        assert_eq!(p.manifest().unwrap().count(), 4);
        assert_eq!(p.version_code(), None);
        assert_eq!(p.label(), None);
        assert_eq!(p.icon(), None);
    }

    #[test]
    fn a_garbled_version_code_reads_as_none() {
        let mut b = PapkBuilder::new(spec());
        b.manifest_entry("version-code", "seven");
        b.class(MAIN_NAME, MAIN);
        let bytes = b.build().unwrap();
        assert_eq!(Papk::parse(&bytes).unwrap().version_code(), None);
    }

    #[test]
    fn minimal_build_parses_back() {
        let mut b = PapkBuilder::new(spec());
        b.class(MAIN_NAME, MAIN);
        let bytes = b.build().unwrap();
        let p = Papk::parse(&bytes).unwrap();
        assert_eq!(p.main_class(), Some(MAIN_NAME));
        assert_eq!(p.manifest_value(crate::keys::PACKAGE_NAME), Some("t"));
        assert_eq!(p.framework_map_version(), Some("0.0.0"));
        assert_eq!(p.class_count(), Ok(1));
        assert!(p.assets().unwrap().is_none());
        let hdr = p.file_header();
        assert_eq!(hdr.section_count, 2);
        assert_eq!(hdr.assets_offset, 0);
        assert_eq!(hdr.version_minor, VERSION_MINOR);
        // The class comes back with its table, and its name from the table.
        let entry = p.classes().unwrap().next().unwrap();
        assert_eq!(entry.name, MAIN_NAME.as_bytes());
        assert_eq!(entry.data, MAIN);
        assert_eq!(entry.link.methods_len(), 2, "<init> and main");
        let section = p.class_section().unwrap();
        assert_eq!(section.find_class(MAIN_NAME.as_bytes()), Some(0));
        section.validate().unwrap();
    }

    #[test]
    fn a_class_added_under_another_name_is_refused() {
        let mut b = PapkBuilder::new(spec());
        b.class("t/Main", MAIN);
        assert_eq!(
            b.build(),
            Err(BuildError::Link(class_link::LinkError::NameMismatch {
                idx: 0
            }))
        );
        // As is a blob that is not a class file at all.
        let mut b = PapkBuilder::new(spec());
        b.class(MAIN_NAME, b"CAFE");
        assert!(matches!(b.build(), Err(BuildError::Link(_))));
    }

    #[test]
    fn oversized_name_errors_instead_of_truncating() {
        let long = String::from_utf8(alloc::vec![b'a'; u16::MAX as usize + 1]).unwrap();
        let mut b = PapkBuilder::new(spec());
        b.manifest_entry(&long, "v");
        b.class(MAIN_NAME, MAIN);
        assert_eq!(b.build(), Err(BuildError::NameTooLong));
    }

    #[test]
    fn oversized_manifest_value_errors() {
        let long = String::from_utf8(alloc::vec![b'v'; u16::MAX as usize + 1]).unwrap();
        let mut b = PapkBuilder::new(spec());
        b.manifest_entry("k", &long);
        assert_eq!(b.build(), Err(BuildError::ValueTooLong));
    }

    #[test]
    fn u16_max_manifest_key_still_builds() {
        // The boundary the writer handles must keep working.
        let key = String::from_utf8(alloc::vec![b'n'; u16::MAX as usize]).unwrap();
        let mut b = PapkBuilder::new(spec());
        b.manifest_entry(&key, "v");
        b.class(MAIN_NAME, MAIN);
        let bytes = b.build().unwrap();
        let p = Papk::parse(&bytes).unwrap();
        assert_eq!(p.manifest_value(key.as_bytes()), Some("v"));
    }

    #[test]
    fn build_error_display_non_empty() {
        use alloc::string::ToString;
        for e in [
            BuildError::NameTooLong,
            BuildError::ValueTooLong,
            BuildError::TooManyEntries,
            BuildError::TooLarge,
            BuildError::Link(class_link::LinkError::Truncated),
        ] {
            assert!(!e.to_string().is_empty());
        }
    }
}
