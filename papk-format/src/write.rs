// SPDX-License-Identifier: GPL-3.0-only
//! PAPK writer (`write` feature, alloc-only).
//!
//! [`PapkBuilder`] owns "the manifest shape" and emits files **byte-identical**
//! to the historical `papk-pack` `build_papk()`:
//!
//! - file header (24 bytes) with `version_major = 1`, `version_minor = 1`
//!   always;
//! - section order MANI, CLSS, then ASST only when at least one asset is
//!   present (otherwise `assets_offset = 0` and `section_count = 2`);
//! - manifest key order: the entry-point key (`main-class` / `activity` /
//!   `application`), `package-name`, `version`, `framework-map-version`,
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
    VERSION_MAJOR, VERSION_MINOR,
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

/// The fixed part of every PAPK manifest.
#[derive(Debug, Clone, Copy)]
pub struct ManifestSpec<'a> {
    pub entry: EntryPoint<'a>,
    pub package_name: &'a str,
    pub version: &'a str,
    pub framework_map_version: &'a str,
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
}

impl core::fmt::Display for BuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::NameTooLong => "name or manifest key exceeds the u16 length prefix",
            Self::ValueTooLong => "value or data blob exceeds its length prefix",
            Self::TooManyEntries => "too many classes or assets for the u32 count field",
            Self::TooLarge => "section offset or length exceeds the u32 header fields",
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
}

impl<'a> PapkBuilder<'a> {
    pub fn new(manifest: ManifestSpec<'a>) -> Self {
        Self {
            manifest,
            extras: Vec::new(),
            classes: Vec::new(),
            assets: Vec::new(),
        }
    }

    /// Append an extra manifest entry (future keys). Emitted after the fixed
    /// four, in insertion order.
    pub fn manifest_entry(&mut self, key: &'a str, value: &'a str) -> &mut Self {
        self.extras.push((key, value));
        self
    }

    /// Append a class. `jvm_name` is the JVM internal name (forward slashes,
    /// no `.class` suffix, e.g. `"helloworld/HelloWorld"`). Emission order is
    /// insertion order — sort beforehand for deterministic output (papk-pack
    /// sorts by name).
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

    /// Serialize the PAPK file. Emission is byte-identical to the historical
    /// papk-pack `build_papk()` (see the module docs for the layout contract).
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

        // File header is 24 bytes. MANIFEST section starts immediately after.
        let manifest_offset = FILE_HEADER_LEN as u32;
        let classes_offset = manifest_offset
            .checked_add(SECTION_HEADER_LEN as u32)
            .and_then(|v| v.checked_add(manifest_len))
            .ok_or(BuildError::TooLarge)?;
        // 0 means "no ASSETS section". Legacy parsers see zero in the slot
        // they formerly read as `reserved` and behave unchanged.
        let assets_offset = if self.assets.is_empty() {
            0u32
        } else {
            classes_offset
                .checked_add(SECTION_HEADER_LEN as u32)
                .and_then(|v| v.checked_add(classes_len))
                .ok_or(BuildError::TooLarge)?
        };
        let section_count: u32 = if self.assets.is_empty() { 2 } else { 3 };

        let mut file = Vec::new();

        // File header (24 bytes)
        file.extend_from_slice(MAGIC);
        file.extend_from_slice(&VERSION_MAJOR.to_le_bytes());
        file.extend_from_slice(&VERSION_MINOR.to_le_bytes());
        file.extend_from_slice(&section_count.to_le_bytes());
        file.extend_from_slice(&manifest_offset.to_le_bytes());
        file.extend_from_slice(&classes_offset.to_le_bytes());
        file.extend_from_slice(&assets_offset.to_le_bytes());

        // MANIFEST section
        push_section_header(&mut file, TAG_MANIFEST, manifest_len);
        file.extend_from_slice(&manifest_data);

        // CLASSES section
        push_section_header(&mut file, TAG_CLASSES, classes_len);
        file.extend_from_slice(&classes_data);

        // ASSETS section (optional)
        if !self.assets.is_empty() {
            push_section_header(&mut file, TAG_ASSETS, assets_len);
            file.extend_from_slice(&assets_data);
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
        for (k, v) in &self.extras {
            push_bytes_u16(&mut data, k.as_bytes(), BuildError::NameTooLong)?;
            push_bytes_u16(&mut data, v.as_bytes(), BuildError::ValueTooLong)?;
        }
        Ok(data)
    }

    /// Build the CLASSES section data.
    fn build_classes_data(&self) -> Result<Vec<u8>, BuildError> {
        let mut data = Vec::new();
        let count = u32::try_from(self.classes.len()).map_err(|_| BuildError::TooManyEntries)?;
        data.extend_from_slice(&count.to_le_bytes());
        for (name, bytes) in &self.classes {
            push_bytes_u16(&mut data, name.as_bytes(), BuildError::NameTooLong)?;
            let data_len = u32::try_from(bytes.len()).map_err(|_| BuildError::ValueTooLong)?;
            data.extend_from_slice(&data_len.to_le_bytes());
            data.extend_from_slice(bytes);
        }
        Ok(data)
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

    fn spec() -> ManifestSpec<'static> {
        ManifestSpec {
            entry: EntryPoint::MainClass("t/Main"),
            package_name: "t",
            version: "1.0",
            framework_map_version: "0.0.0",
        }
    }

    #[test]
    fn minimal_build_parses_back() {
        let mut b = PapkBuilder::new(spec());
        b.class("t/Main", b"\xCA\xFE\xBA\xBE");
        let bytes = b.build().unwrap();
        let p = Papk::parse(&bytes).unwrap();
        assert_eq!(p.main_class(), Some("t/Main"));
        assert_eq!(p.manifest_value(crate::keys::PACKAGE_NAME), Some("t"));
        assert_eq!(p.framework_map_version(), Some("0.0.0"));
        assert_eq!(p.class_count(), Ok(1));
        assert!(p.assets().unwrap().is_none());
        let hdr = p.file_header();
        assert_eq!(hdr.section_count, 2);
        assert_eq!(hdr.assets_offset, 0);
        assert_eq!(hdr.version_minor, 1);
    }

    #[test]
    fn oversized_name_errors_instead_of_truncating() {
        let long = String::from_utf8(alloc::vec![b'a'; u16::MAX as usize + 1]).unwrap();
        let mut b = PapkBuilder::new(spec());
        b.class(&long, b"x");
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
    fn u16_max_name_still_builds() {
        // The boundary the old writer handled correctly must keep working.
        let name = String::from_utf8(alloc::vec![b'n'; u16::MAX as usize]).unwrap();
        let mut b = PapkBuilder::new(spec());
        b.class(&name, b"x");
        let bytes = b.build().unwrap();
        let p = Papk::parse(&bytes).unwrap();
        let entry = p.classes().unwrap().next().unwrap();
        assert_eq!(entry.name.len(), u16::MAX as usize);
    }

    #[test]
    fn build_error_display_non_empty() {
        use alloc::string::ToString;
        for e in [
            BuildError::NameTooLong,
            BuildError::ValueTooLong,
            BuildError::TooManyEntries,
            BuildError::TooLarge,
        ] {
            assert!(!e.to_string().is_empty());
        }
    }
}
