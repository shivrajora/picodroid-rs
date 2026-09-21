// SPDX-License-Identifier: GPL-3.0-only
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
fn verify_compat_rejects_papk_before_member_floor() {
    let papk =
        build_papk_with_manifest(&[("main-class", "x/Y"), ("framework-map-version", "0.15.0")]);
    assert_eq!(
        parse_leaked(papk).verify_compat(compat::MEMBER_SHRINK_FLOOR),
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
