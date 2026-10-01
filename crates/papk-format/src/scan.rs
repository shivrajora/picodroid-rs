// SPDX-License-Identifier: GPL-3.0-only
//! Streaming / pre-flight manifest scanners.
//!
//! These deliberately do NOT go through [`crate::Papk`]: they answer "is this
//! byte buffer PAPK-shaped, and what does one manifest key say" for callers
//! that may not hold a complete or valid file — the `pdb install` host
//! pre-flight ([`validate_structure`] + [`find_manifest_value`]) and the
//! device-side install peek over a partially received wire buffer
//! ([`find_manifest_value_in_prefix`]).
//!
//! The two `find_manifest_value*` scanners have deliberately different
//! semantics and MUST remain separate functions:
//!
//! - **strict** ([`find_manifest_value`]): the buffer is the whole file; a
//!   manifest section that walks past the buffer end returns `None`. Merging
//!   toward tolerant re-opens the pdb "100-byte stub accepted" install
//!   regression.
//! - **prefix-tolerant** ([`find_manifest_value_in_prefix`]): the buffer may
//!   be a prefix of the file; a manifest section extending past the buffer is
//!   walked only as far as buffered. Merging toward strict breaks device
//!   installs whose manifest straddles the peek boundary.

use crate::{FILE_HEADER_LEN, SECTION_HEADER_LEN, SUPPORTED_VERSION_MAJOR};

/// Reasons a buffer fails PAPK structural validation. Distinct from "manifest
/// key absent" (see `find_manifest_value`'s `None`) — these all mean the file
/// isn't a PAPK and the install should refuse regardless of compat mode.
#[derive(Debug, PartialEq, Eq)]
pub enum StructuralError {
    /// File is shorter than the fixed PAPK header.
    TooShort,
    /// First 4 bytes aren't `PAPK`.
    BadMagic,
    /// Declared manifest offset + section header doesn't fit in the buffer.
    ManifestOutOfBounds,
    /// Manifest section header isn't `MANI`.
    ManifestBadMagic,
    /// A manifest TLV entry's length fields walk past the section end.
    ManifestMalformed,
    /// `version_major` is not the one this build reads: a v1 file, from
    /// before the classes carried link tables. Re-pack it.
    UnsupportedVersion,
    /// Declared classes offset + section header doesn't fit in the buffer.
    ClassesOutOfBounds,
    /// Classes section header isn't `CLSS`.
    ClassesBadMagic,
    /// The classes section is not a valid class section: a class or its
    /// link table out of bounds, a table that does not derive from its
    /// class bytes, or a bad index.
    Classes(class_link::LinkError),
}

impl core::fmt::Display for StructuralError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::TooShort => "file is smaller than a PAPK header",
            Self::BadMagic => "magic bytes are not 'PAPK'",
            Self::ManifestOutOfBounds => "manifest offset points past end of file",
            Self::ManifestBadMagic => "manifest section header is not 'MANI'",
            Self::ManifestMalformed => "manifest TLV entries overflow the section",
            Self::UnsupportedVersion => {
                "PAPK format version is not the one this build reads (a v1 file: re-pack it with the current toolchain)"
            }
            Self::ClassesOutOfBounds => "classes offset points past end of file",
            Self::ClassesBadMagic => "classes section header is not 'CLSS'",
            Self::Classes(e) => return write!(f, "classes section: {e}"),
        };
        f.write_str(s)
    }
}

/// Deep-check a class section: parse it and validate every class's link
/// table against its bytes. A buffer that is not 4-byte aligned — a host
/// `Vec<u8>` or `include_bytes!` data, never a flash image — is checked
/// through an aligned copy when the crate can allocate (`write`).
fn classes_section_valid(data: &[u8]) -> Result<(), class_link::LinkError> {
    match class_link::ClassSection::parse(data) {
        Ok(s) => s.validate(),
        #[cfg(feature = "write")]
        Err(class_link::LinkError::Misaligned) => {
            let aligned = crate::aligned::AlignedBuf::new(data);
            class_link::ClassSection::parse(&aligned)?.validate()
        }
        Err(e) => Err(e),
    }
}

/// Validate that `bytes` is structurally a PAPK: header present, magic and
/// version match, the manifest section parses to completion, and the
/// classes section is a valid class section — every class's link table
/// derives from its class bytes ([`class_link::Link::validate`]). That deep
/// check runs here, once per install, embed or boot scan, and the JVM then
/// trusts the tables. A Ok result does NOT imply any particular manifest key
/// is present — callers still need [`find_manifest_value`] for that.
///
/// Errors here are unconditional refusals at the host level: they exist
/// independently of compat mode, and in particular are not gated on
/// `--skip-host-check` (that flag only bypasses *compat* checks, not
/// structural validity — streaming a truncated PAPK to flash bricks the
/// device regardless of version arithmetic).
pub fn validate_structure(bytes: &[u8]) -> Result<(), StructuralError> {
    validate(bytes, true)
}

/// [`validate_structure`] without the deep check of the classes section:
/// the header, the manifest walk and the class section's own bounds
/// (`ClassSection::parse`), nothing per class. For an image that already
/// passed the deep check and cannot have changed since — a system app
/// linked into the firmware, which the build script validated before
/// embedding it. The deep check costs ~0.3 ms per KB of PAPK from XIP flash
/// (RP2350), on every boot.
pub fn validate_embedded(bytes: &[u8]) -> Result<(), StructuralError> {
    validate(bytes, false)
}

fn validate(bytes: &[u8], deep: bool) -> Result<(), StructuralError> {
    if bytes.len() < FILE_HEADER_LEN {
        return Err(StructuralError::TooShort);
    }
    if &bytes[0..4] != b"PAPK" {
        return Err(StructuralError::BadMagic);
    }
    if u16::from_le_bytes([bytes[4], bytes[5]]) != SUPPORTED_VERSION_MAJOR {
        return Err(StructuralError::UnsupportedVersion);
    }
    let mani_off = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if mani_off
        .checked_add(SECTION_HEADER_LEN)
        .is_none_or(|e| e > bytes.len())
    {
        return Err(StructuralError::ManifestOutOfBounds);
    }
    if &bytes[mani_off..mani_off + 4] != b"MANI" {
        return Err(StructuralError::ManifestBadMagic);
    }
    let mani_len = u32::from_le_bytes([
        bytes[mani_off + 4],
        bytes[mani_off + 5],
        bytes[mani_off + 6],
        bytes[mani_off + 7],
    ]) as usize;
    let mani_data_start = mani_off + SECTION_HEADER_LEN;
    let mani_data_end = mani_data_start
        .checked_add(mani_len)
        .ok_or(StructuralError::ManifestMalformed)?;
    if mani_data_end > bytes.len() {
        return Err(StructuralError::ManifestMalformed);
    }

    // Walk the manifest TLVs to confirm every key/value length stays in range.
    let mut p = mani_data_start;
    while p < mani_data_end {
        if p + 2 > mani_data_end {
            return Err(StructuralError::ManifestMalformed);
        }
        let klen = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + klen > mani_data_end {
            return Err(StructuralError::ManifestMalformed);
        }
        p += klen;
        if p + 2 > mani_data_end {
            return Err(StructuralError::ManifestMalformed);
        }
        let vlen = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + vlen > mani_data_end {
            return Err(StructuralError::ManifestMalformed);
        }
        p += vlen;
    }

    // The classes section.
    let clss_off = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as usize;
    if clss_off
        .checked_add(SECTION_HEADER_LEN)
        .is_none_or(|e| e > bytes.len())
    {
        return Err(StructuralError::ClassesOutOfBounds);
    }
    if &bytes[clss_off..clss_off + 4] != b"CLSS" {
        return Err(StructuralError::ClassesBadMagic);
    }
    let clss_len = u32::from_le_bytes([
        bytes[clss_off + 4],
        bytes[clss_off + 5],
        bytes[clss_off + 6],
        bytes[clss_off + 7],
    ]) as usize;
    let clss_start = clss_off + SECTION_HEADER_LEN;
    let clss_end = clss_start
        .checked_add(clss_len)
        .ok_or(StructuralError::ClassesOutOfBounds)?;
    if clss_end > bytes.len() {
        return Err(StructuralError::ClassesOutOfBounds);
    }
    let classes = &bytes[clss_start..clss_end];
    if deep {
        classes_section_valid(classes).map_err(StructuralError::Classes)
    } else {
        class_link::ClassSection::parse(classes)
            .map(|_| ())
            .map_err(StructuralError::Classes)
    }
}

/// STRICT whole-file manifest scan (`pdb install` pre-flight semantics):
/// extract the value of manifest key `key` from a complete PAPK byte buffer.
/// Returns `None` if the magic is wrong, the manifest section is
/// malformed/truncated — any TLV walking past the buffer end — or the key
/// is absent.
///
/// Generalization of pdb's `read_framework_map_version` (use
/// [`crate::keys::FRAMEWORK_MAP_VERSION`] for that behavior).
/// `bytes` should be the full PAPK file (small — typical PAPKs are < 10 KB).
pub fn find_manifest_value<'a>(bytes: &'a [u8], key: &[u8]) -> Option<&'a str> {
    if bytes.len() < FILE_HEADER_LEN || &bytes[0..4] != b"PAPK" {
        return None;
    }
    let mani_off = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    let mani_data_start = mani_off.checked_add(SECTION_HEADER_LEN)?;
    if mani_data_start > bytes.len() {
        return None;
    }
    if &bytes[mani_off..mani_off + 4] != b"MANI" {
        return None;
    }
    let mani_len = u32::from_le_bytes([
        bytes[mani_off + 4],
        bytes[mani_off + 5],
        bytes[mani_off + 6],
        bytes[mani_off + 7],
    ]) as usize;
    let mani_data_end = mani_data_start.checked_add(mani_len)?;
    if mani_data_end > bytes.len() {
        return None;
    }

    let mut p = mani_data_start;
    while p + 2 <= mani_data_end {
        let klen = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + klen > mani_data_end {
            return None;
        }
        let entry_key = &bytes[p..p + klen];
        p += klen;
        if p + 2 > mani_data_end {
            return None;
        }
        let vlen = u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
        p += 2;
        if p + vlen > mani_data_end {
            return None;
        }
        let val = &bytes[p..p + vlen];
        p += vlen;
        if entry_key == key {
            return core::str::from_utf8(val).ok();
        }
    }
    None
}

/// PREFIX-TOLERANT manifest scan (device install-peek semantics): extract the
/// value of manifest key `key` from a buffered PAPK *prelude*. The buffer may
/// be a prefix of the file — a manifest section whose declared end lies past
/// the buffer is walked only as far as buffered (the key may simply be in the
/// unscanned tail; that yields `None`, not an error). Returns `None` if the
/// header is malformed, the manifest section header is truncated within the
/// buffer, or the key is not found in the buffered part.
///
/// Moved from `platforms/rp`'s `packagemanager::install::
/// extract_framework_map_version` (use
/// [`crate::keys::FRAMEWORK_MAP_VERSION`] for that behavior).
pub fn find_manifest_value_in_prefix<'a>(prefix: &'a [u8], key: &[u8]) -> Option<&'a str> {
    if prefix.len() < FILE_HEADER_LEN || &prefix[0..4] != b"PAPK" {
        return None;
    }
    let mani_off = u32::from_le_bytes([prefix[12], prefix[13], prefix[14], prefix[15]]) as usize;
    let mani_data_start = mani_off.checked_add(SECTION_HEADER_LEN)?;
    if mani_data_start > prefix.len() || mani_off + 4 > prefix.len() {
        return None;
    }
    if &prefix[mani_off..mani_off + 4] != b"MANI" {
        return None;
    }
    let mani_len = u32::from_le_bytes([
        prefix[mani_off + 4],
        prefix[mani_off + 5],
        prefix[mani_off + 6],
        prefix[mani_off + 7],
    ]) as usize;
    let mani_data_end = mani_data_start.checked_add(mani_len)?;
    // It's OK if mani_data_end > prefix.len() — the key may simply be in the
    // unscanned tail. Walk only what we've buffered.
    let scan_end = mani_data_end.min(prefix.len());

    let mut p = mani_data_start;
    while p + 2 <= scan_end {
        let klen = u16::from_le_bytes([prefix[p], prefix[p + 1]]) as usize;
        p += 2;
        if p + klen > scan_end {
            return None;
        }
        let entry_key = &prefix[p..p + klen];
        p += klen;
        if p + 2 > scan_end {
            return None;
        }
        let vlen = u16::from_le_bytes([prefix[p], prefix[p + 1]]) as usize;
        p += 2;
        if p + vlen > scan_end {
            return None;
        }
        let val = &prefix[p..p + vlen];
        p += vlen;
        if entry_key == key {
            return core::str::from_utf8(val).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        find_manifest_value, find_manifest_value_in_prefix, validate_structure, StructuralError,
    };
    use crate::keys;
    use alloc::vec;
    use alloc::vec::Vec;

    fn build_papk(manifest_entries: &[(&str, &str)]) -> Vec<u8> {
        let mut manifest = Vec::new();
        for (k, v) in manifest_entries {
            manifest.extend_from_slice(&(k.len() as u16).to_le_bytes());
            manifest.extend_from_slice(k.as_bytes());
            manifest.extend_from_slice(&(v.len() as u16).to_le_bytes());
            manifest.extend_from_slice(v.as_bytes());
        }
        // MANI right after the 28-byte header; CLSS at the next 4-byte
        // boundary, holding an empty class section (no classes, the index
        // right after the section's own 8-byte header, then an empty
        // literal pool).
        let clss_off = (28 + 16 + manifest.len()).next_multiple_of(4);
        let mut out = Vec::new();
        out.extend_from_slice(b"PAPK");
        out.extend_from_slice(&super::SUPPORTED_VERSION_MAJOR.to_le_bytes()); // major
        out.extend_from_slice(&0u16.to_le_bytes()); // minor
        out.extend_from_slice(&2u32.to_le_bytes()); // sec count
        out.extend_from_slice(&28u32.to_le_bytes()); // manifest_offset
        out.extend_from_slice(&(clss_off as u32).to_le_bytes()); // classes_offset
        out.extend_from_slice(&0u32.to_le_bytes()); // assets_offset
        out.extend_from_slice(&0u32.to_le_bytes()); // resources_offset
        out.extend_from_slice(b"MANI");
        out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // crc
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        out.extend_from_slice(&manifest);
        out.resize(clss_off, 0);
        out.extend_from_slice(b"CLSS");
        out.extend_from_slice(&12u32.to_le_bytes()); // length
        out.extend_from_slice(&0u32.to_le_bytes()); // crc
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        out.extend_from_slice(&0u32.to_le_bytes()); // class_count
        out.extend_from_slice(&8u32.to_le_bytes()); // index_off
        out.extend_from_slice(&0u32.to_le_bytes()); // literal count
        out
    }

    // ── Moved from tools/pdb/src/papk_meta.rs (strict scanner +
    //    validate_structure) — assertion content unchanged. ────────────────
    mod strict {
        use super::*;

        #[test]
        fn returns_value_when_present() {
            let buf = build_papk(&[
                ("main-class", "x/Y"),
                ("framework-map-version", "0.1.0"),
                ("package-name", "test"),
            ]);
            assert_eq!(
                find_manifest_value(&buf, keys::FRAMEWORK_MAP_VERSION),
                Some("0.1.0")
            );
        }

        #[test]
        fn returns_none_when_absent() {
            let buf = build_papk(&[("main-class", "x/Y")]);
            assert_eq!(find_manifest_value(&buf, keys::FRAMEWORK_MAP_VERSION), None);
        }

        #[test]
        fn returns_none_for_bad_magic() {
            let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
            buf[0] = b'X';
            assert_eq!(find_manifest_value(&buf, keys::FRAMEWORK_MAP_VERSION), None);
        }

        #[test]
        fn returns_none_for_truncated_input() {
            let buf = build_papk(&[("framework-map-version", "0.1.0")]);
            assert_eq!(
                find_manifest_value(&buf[..30], keys::FRAMEWORK_MAP_VERSION),
                None
            );
        }

        #[test]
        fn validate_accepts_well_formed_papk() {
            let buf = build_papk(&[("framework-map-version", "0.1.0")]);
            assert_eq!(validate_structure(&buf), Ok(()));
        }

        #[test]
        fn validate_accepts_papk_without_fmv_key() {
            // Legacy no-key case — structural validity is independent of which
            // keys are present. compat::check is responsible for catching "no
            // framework-map-version key", not validate_structure.
            let buf = build_papk(&[("main-class", "x/Y")]);
            assert_eq!(validate_structure(&buf), Ok(()));
        }

        #[test]
        fn validate_rejects_short_buffer() {
            // The regression: a 100-byte stub must be refused.  Previously
            // read_framework_map_version returned None and compat::check treated
            // "None vs 0.0.0 firmware" as symmetric-versionless → accept.
            let buf = vec![b'P', b'A', b'P', b'K'];
            assert_eq!(validate_structure(&buf), Err(StructuralError::TooShort));
            let stub = vec![0u8; 100];
            assert!(validate_structure(&stub).is_err());
        }

        #[test]
        fn validate_rejects_bad_magic() {
            let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
            buf[0] = b'X';
            assert_eq!(validate_structure(&buf), Err(StructuralError::BadMagic));
        }

        #[test]
        fn validate_rejects_manifest_offset_out_of_bounds() {
            let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
            // Corrupt manifest offset (bytes 12..16) to point past end of file.
            let bad_off = (buf.len() as u32 + 4096).to_le_bytes();
            buf[12..16].copy_from_slice(&bad_off);
            assert_eq!(
                validate_structure(&buf),
                Err(StructuralError::ManifestOutOfBounds)
            );
        }

        #[test]
        fn validate_rejects_manifest_bad_section_magic() {
            let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
            // Manifest section header starts at offset 28 by construction.
            buf[28] = b'X';
            assert_eq!(
                validate_structure(&buf),
                Err(StructuralError::ManifestBadMagic)
            );
        }

        #[test]
        fn validate_rejects_a_v1_file() {
            let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
            buf[4..6].copy_from_slice(&1u16.to_le_bytes());
            assert_eq!(
                validate_structure(&buf),
                Err(StructuralError::UnsupportedVersion)
            );
        }

        #[test]
        fn validate_rejects_a_bad_classes_section() {
            let ok = build_papk(&[("framework-map-version", "0.1.0")]);
            assert_eq!(validate_structure(&ok), Ok(()));
            let clss = u32::from_le_bytes(ok[16..20].try_into().unwrap()) as usize;
            let mut buf = ok.clone();
            buf[clss] = b'X';
            assert_eq!(
                validate_structure(&buf),
                Err(StructuralError::ClassesBadMagic)
            );
            let mut buf = ok.clone();
            buf[16..20].copy_from_slice(&(ok.len() as u32).to_le_bytes());
            assert_eq!(
                validate_structure(&buf),
                Err(StructuralError::ClassesOutOfBounds)
            );
            // Five classes declared, none present.
            let mut buf = ok.clone();
            buf[clss + 16..clss + 20].copy_from_slice(&5u32.to_le_bytes());
            assert!(matches!(
                validate_structure(&buf),
                Err(StructuralError::Classes(_))
            ));
        }

        #[test]
        fn validate_rejects_manifest_overrunning_tlv() {
            let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
            // TLV key length lives at offset 28+16 = 44 (first TLV's klen u16).
            // Inflate it past the section end.
            buf[44..46].copy_from_slice(&0xffffu16.to_le_bytes());
            assert_eq!(
                validate_structure(&buf),
                Err(StructuralError::ManifestMalformed)
            );
        }
    }

    // ── Moved from platforms/rp/src/packagemanager/install.rs (prefix-
    //    tolerant scanner). These 4 tests existed there but NEVER compiled:
    //    packagemanager/mod.rs cfg-gates `mod install` out of both test and
    //    sim builds. This is their first-ever execution. ────────────────────
    mod prefix_tolerant {
        use super::*;

        #[test]
        fn extracts_present_key() {
            let buf = build_papk(&[("main-class", "x/Y"), ("framework-map-version", "0.1.0")]);
            assert_eq!(
                find_manifest_value_in_prefix(&buf, keys::FRAMEWORK_MAP_VERSION),
                Some("0.1.0")
            );
        }

        #[test]
        fn returns_none_when_absent() {
            let buf = build_papk(&[("main-class", "x/Y")]);
            assert_eq!(
                find_manifest_value_in_prefix(&buf, keys::FRAMEWORK_MAP_VERSION),
                None
            );
        }

        #[test]
        fn returns_none_for_bad_magic() {
            let mut buf = build_papk(&[("framework-map-version", "0.1.0")]);
            buf[0] = 0xFF;
            assert_eq!(
                find_manifest_value_in_prefix(&buf, keys::FRAMEWORK_MAP_VERSION),
                None
            );
        }

        #[test]
        fn returns_none_for_truncated_manifest() {
            // Truncate so the manifest section header itself isn't fully present.
            let buf = build_papk(&[("framework-map-version", "0.1.0")]);
            assert_eq!(
                find_manifest_value_in_prefix(&buf[..28], keys::FRAMEWORK_MAP_VERSION),
                None
            );
        }
    }

    // ── NEW: differential test pinning the two scanners apart forever. ───
    #[test]
    fn strict_and_tolerant_differ_on_manifest_straddling_the_cut() {
        // Manifest: the fmv entry first, then a long filler entry so the
        // declared manifest end lies well past our cut point.
        let filler_value = "z".repeat(200);
        let buf = build_papk(&[
            ("framework-map-version", "0.1.0"),
            ("zz-filler", &filler_value),
        ]);
        // Cut right after the fmv entry: 28 (header) + 16 (MANI header) +
        // [2+21 key][2+5 value] = 74 bytes. The manifest section's declared
        // end is far beyond, so this prefix ends mid-section.
        let fmv_entry_len = 2 + "framework-map-version".len() + 2 + "0.1.0".len();
        let cut = 28 + 16 + fmv_entry_len;
        assert!(cut < buf.len());
        let prefix = &buf[..cut];

        // Tolerant: walks the buffered part, finds the key.
        assert_eq!(
            find_manifest_value_in_prefix(prefix, keys::FRAMEWORK_MAP_VERSION),
            Some("0.1.0")
        );
        // Strict: the manifest section extends past the buffer => None.
        assert_eq!(
            find_manifest_value(prefix, keys::FRAMEWORK_MAP_VERSION),
            None
        );
        // And on the complete file both agree.
        assert_eq!(
            find_manifest_value(&buf, keys::FRAMEWORK_MAP_VERSION),
            Some("0.1.0")
        );
        assert_eq!(
            find_manifest_value_in_prefix(&buf, keys::FRAMEWORK_MAP_VERSION),
            Some("0.1.0")
        );
    }

    #[test]
    fn scanners_find_arbitrary_keys() {
        // The generalization over pdb's fixed framework-map-version literal.
        let buf = build_papk(&[("package-name", "demo"), ("custom", "v")]);
        assert_eq!(find_manifest_value(&buf, keys::PACKAGE_NAME), Some("demo"));
        assert_eq!(find_manifest_value(&buf, b"custom"), Some("v"));
        assert_eq!(
            find_manifest_value_in_prefix(&buf, keys::PACKAGE_NAME),
            Some("demo")
        );
        assert_eq!(find_manifest_value_in_prefix(&buf, b"custom"), Some("v"));
    }

    #[test]
    fn structural_error_has_display_impl() {
        use alloc::string::ToString;
        for e in [
            StructuralError::TooShort,
            StructuralError::BadMagic,
            StructuralError::ManifestOutOfBounds,
            StructuralError::ManifestBadMagic,
            StructuralError::ManifestMalformed,
        ] {
            assert!(!e.to_string().is_empty());
        }
    }
}
