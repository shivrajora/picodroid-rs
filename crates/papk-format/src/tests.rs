// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use alloc::boxed::Box;
use alloc::vec::Vec;
use class_link::build::{build_section_named, minimal_class};

/// A v2 PAPK built by hand (no `PapkBuilder`, so the reader is tested
/// against the layout and not against the writer): `entries` in the
/// manifest, `classes` linked into a class section, `assets` as
/// `(name, w, h, cf, stride, data)` records. Every section starts 4-aligned.
#[allow(clippy::type_complexity)]
fn build_papk(
    entries: &[(&str, &str)],
    classes: &[(&str, &[u8])],
    assets: &[(&str, u16, u16, u8, u16, &[u8])],
) -> Vec<u8> {
    let mut manifest_data: Vec<u8> = Vec::new();
    for (k, v) in entries {
        manifest_data.extend_from_slice(&(k.len() as u16).to_le_bytes());
        manifest_data.extend_from_slice(k.as_bytes());
        manifest_data.extend_from_slice(&(v.len() as u16).to_le_bytes());
        manifest_data.extend_from_slice(v.as_bytes());
    }
    let named: Vec<(&[u8], &[u8])> = classes.iter().map(|(n, b)| (n.as_bytes(), *b)).collect();
    let classes_data = build_section_named(&named).expect("test classes link");
    let mut assets_data: Vec<u8> = Vec::new();
    if !assets.is_empty() {
        assets_data.extend_from_slice(&(assets.len() as u32).to_le_bytes());
        for (name, w, h, cf, stride, data) in assets {
            assets_data.extend_from_slice(&(name.len() as u16).to_le_bytes());
            assets_data.extend_from_slice(name.as_bytes());
            assets_data.extend_from_slice(&w.to_le_bytes());
            assets_data.extend_from_slice(&h.to_le_bytes());
            assets_data.push(*cf);
            assets_data.push(0);
            assets_data.extend_from_slice(&stride.to_le_bytes());
            assets_data.extend_from_slice(&(data.len() as u32).to_le_bytes());
            assets_data.resize(assets_data.len().next_multiple_of(4), 0);
            assets_data.extend_from_slice(data);
            assets_data.resize(assets_data.len().next_multiple_of(4), 0);
        }
    }

    let manifest_offset = FILE_HEADER_LEN;
    let classes_offset =
        (manifest_offset + SECTION_HEADER_LEN + manifest_data.len()).next_multiple_of(4);
    let assets_offset = if assets.is_empty() {
        0
    } else {
        (classes_offset + SECTION_HEADER_LEN + classes_data.len()).next_multiple_of(4)
    };

    let mut file: Vec<u8> = Vec::new();
    file.extend_from_slice(b"PAPK");
    file.extend_from_slice(&VERSION_MAJOR.to_le_bytes());
    file.extend_from_slice(&VERSION_MINOR.to_le_bytes());
    file.extend_from_slice(&(2 + u32::from(!assets.is_empty())).to_le_bytes());
    file.extend_from_slice(&(manifest_offset as u32).to_le_bytes());
    file.extend_from_slice(&(classes_offset as u32).to_le_bytes());
    file.extend_from_slice(&(assets_offset as u32).to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes()); // resources_offset
    assert_eq!(file.len(), FILE_HEADER_LEN);
    let section = |file: &mut Vec<u8>, tag: u32, data: &[u8]| {
        file.extend_from_slice(&tag.to_le_bytes());
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes()); // crc32
        file.extend_from_slice(&0u32.to_le_bytes()); // reserved
        file.extend_from_slice(data);
    };
    section(&mut file, TAG_MANIFEST, &manifest_data);
    file.resize(classes_offset, 0);
    section(&mut file, TAG_CLASSES, &classes_data);
    if !assets.is_empty() {
        file.resize(assets_offset, 0);
        section(&mut file, TAG_ASSETS, &assets_data);
    }
    file
}

/// A main-class PAPK with the three fixed manifest keys.
fn build_test_papk(main_class: &str, classes: &[(&str, &[u8])]) -> Vec<u8> {
    build_papk(
        &[
            ("main-class", main_class),
            ("package-name", "testpkg"),
            ("version", "1.0"),
        ],
        classes,
        &[],
    )
}

/// The file at an aligned address, leaked: what a flash image or a
/// `.rodata` static gives the reader.
fn parse_leaked(papk: Vec<u8>) -> Papk<'static> {
    let buf: &'static AlignedBuf = Box::leak(Box::new(AlignedBuf::new(&papk)));
    Papk::parse(buf).unwrap()
}

#[test]
fn test_parse_header() {
    let p = parse_leaked(build_test_papk("test/Main", &[]));
    assert_eq!(p.manifest_offset, FILE_HEADER_LEN);
    assert_eq!(p.classes_offset % 4, 0);
}

#[test]
fn test_main_class() {
    let p = parse_leaked(build_test_papk("hello/World", &[]));
    assert_eq!(p.main_class(), Some("hello/World"));
}

#[test]
fn test_manifest_iter() {
    let p = parse_leaked(build_test_papk("foo/Bar", &[]));
    let entries: Vec<_> = p.manifest().unwrap().collect();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].key, b"main-class");
    assert_eq!(entries[0].value, b"foo/Bar");
    assert_eq!(entries[1].key, b"package-name");
    assert_eq!(entries[2].key, b"version");
}

#[test]
fn test_classes_iter() {
    let bar = minimal_class(b"foo/Bar");
    let util = minimal_class(b"lib/Util");
    let p = parse_leaked(build_test_papk(
        "foo/Bar",
        &[("foo/Bar", &bar), ("lib/Util", &util)],
    ));
    let classes: Vec<_> = p.classes().unwrap().collect();
    assert_eq!(classes.len(), 2);
    assert_eq!(classes[0].name, b"foo/Bar");
    assert_eq!(classes[0].data, &bar[..]);
    assert_eq!(classes[0].link.methods_len(), 0);
    assert_eq!(
        classes[0].linked().super_name(),
        Some(&b"java/lang/Object"[..])
    );
    assert_eq!(classes[1].name, b"lib/Util");
    assert_eq!(classes[1].data, &util[..]);
    // The section's index finds them by name.
    let section = p.class_section().unwrap();
    assert_eq!(section.find_class(b"lib/Util"), Some(1));
    assert_eq!(section.find_class(b"nope/Nope"), None);
    section.validate().unwrap();
}

#[test]
fn test_application() {
    let p = parse_leaked(build_papk(
        &[
            ("application", "demo/MyApp"),
            ("package-name", "testpkg"),
            ("version", "1.0"),
        ],
        &[],
        &[],
    ));
    assert_eq!(p.application(), Some("demo/MyApp"));
    assert_eq!(p.main_class(), None);
    assert_eq!(p.activity(), None);
}

#[test]
fn test_bad_magic() {
    let mut papk = build_test_papk("foo/Bar", &[]);
    papk[0] = 0xFF;
    assert!(matches!(Papk::parse(&papk), Err(PapkError::BadMagic)));
}

#[test]
fn test_truncated() {
    let papk = build_test_papk("foo/Bar", &[]);
    assert!(matches!(
        Papk::parse(&papk[..10]),
        Err(PapkError::Truncated)
    ));
    assert!(matches!(
        Papk::parse(&papk[..FILE_HEADER_LEN - 1]),
        Err(PapkError::Truncated)
    ));
}

#[test]
fn a_v1_file_is_refused() {
    let mut papk = build_test_papk("foo/Bar", &[]);
    papk[4..6].copy_from_slice(&1u16.to_le_bytes());
    assert!(matches!(
        Papk::parse(&papk),
        Err(PapkError::UnsupportedVersion)
    ));
}

/// Build a PAPK with a custom set of manifest key/value pairs (no classes).
fn build_papk_with_manifest(entries: &[(&str, &str)]) -> Vec<u8> {
    build_papk(entries, &[], &[])
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
    // A PAPK without the key is compatible with firmware that hasn't cut
    // any shrink-map release yet.
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

// ── ASSETS section tests ─────────────────────────────────────────────

/// A PAPK with an empty manifest, no classes and a populated ASSETS
/// section. Each asset is `(name, w, h, cf, stride, data)`.
#[allow(clippy::type_complexity)]
fn build_papk_with_assets(assets: &[(&str, u16, u16, u8, u16, &[u8])]) -> Vec<u8> {
    build_papk(&[], &[], assets)
}

#[test]
fn assets_section_absent_without_assets() {
    // A 0 in the [20..24] header slot is a papk with no ASSETS section.
    let p = parse_leaked(build_test_papk("foo/Bar", &[]));
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
    // Corrupt assets_offset to point past EOF.
    let mut papk = build_papk_with_assets(&[("x.png", 1, 1, 18, 0, &[0, 0])]);
    let len = papk.len() as u32;
    let bad = (len + 100).to_le_bytes();
    papk[20..24].copy_from_slice(&bad);
    assert!(matches!(Papk::parse(&papk), Err(PapkError::Truncated)));
}

// ── New API surface (papk-format additions over jvm/src/apk.rs) ─────

#[test]
fn file_header_parse_reads_all_fields() {
    let papk = build_papk_with_assets(&[("x.png", 1, 1, 18, 0, &[0, 0, 0, 0])]);
    let hdr = FileHeader::parse(&papk).unwrap();
    assert_eq!(hdr.version_major, VERSION_MAJOR);
    assert_eq!(hdr.version_minor, VERSION_MINOR);
    assert_eq!(hdr.section_count, 3);
    assert_eq!(hdr.manifest_offset as usize, FILE_HEADER_LEN);
    // An empty manifest: CLSS right after the MANI header.
    assert_eq!(
        hdr.classes_offset as usize,
        FILE_HEADER_LEN + SECTION_HEADER_LEN
    );
    assert_ne!(hdr.assets_offset, 0);
    assert_eq!(hdr.resources_offset, 0);
}

#[test]
fn file_header_parse_does_not_enforce_version_major() {
    // A future-major file: FileHeader::parse still dumps the header,
    // while the full parser refuses it.
    let mut papk = build_test_papk("foo/Bar", &[]);
    papk[4..6].copy_from_slice(&3u16.to_le_bytes());
    let hdr = FileHeader::parse(&papk).unwrap();
    assert_eq!(hdr.version_major, 3);
    assert!(matches!(
        Papk::parse(&papk),
        Err(PapkError::UnsupportedVersion)
    ));
}

#[test]
fn file_header_parse_checks_magic_and_length_only() {
    assert_eq!(FileHeader::parse(&[]), Err(PapkError::Truncated));
    assert_eq!(
        FileHeader::parse(&[0u8; FILE_HEADER_LEN - 1]),
        Err(PapkError::Truncated)
    );
    let mut papk = build_test_papk("foo/Bar", &[]);
    papk[0] = b'X';
    assert_eq!(FileHeader::parse(&papk), Err(PapkError::BadMagic));
}

#[test]
fn file_header_accessor_matches_standalone_parse() {
    let bar = minimal_class(b"foo/Bar");
    let papk = build_test_papk("foo/Bar", &[("foo/Bar", &bar)]);
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
    // An empty class section: no classes, the index right after its
    // header, no literals.
    assert_eq!(cdata, [0, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0]);

    let (ah, adata) = p.assets_section().unwrap().expect("ASST present");
    assert_eq!(ah.tag, TAG_ASSETS);
    assert_eq!(ah.length as usize, adata.len());
}

#[test]
fn assets_section_accessor_none_without_assets() {
    let p = parse_leaked(build_test_papk("foo/Bar", &[]));
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
    let bar = minimal_class(b"foo/Bar");
    let util = minimal_class(b"lib/Util");
    let p = parse_leaked(build_test_papk(
        "foo/Bar",
        &[("foo/Bar", &bar), ("lib/Util", &util)],
    ));
    assert_eq!(p.class_count(), Ok(2));
    assert_eq!(
        p.classes().unwrap().count() as u32,
        p.class_count().unwrap()
    );
}

#[test]
fn a_class_count_the_section_cannot_hold_is_an_error() {
    // Corrupt the declared count upward: the directory would run past the
    // section, so the section refuses to parse — no short iterator, an
    // error a dump tool prints (`class_count` still shows what was declared).
    let bar = minimal_class(b"foo/Bar");
    let mut papk = build_test_papk("foo/Bar", &[("foo/Bar", &bar)]);
    let cdata_start = {
        let p = Papk::parse(&papk).unwrap();
        p.classes_offset + SECTION_HEADER_LEN
    };
    papk[cdata_start..cdata_start + 4].copy_from_slice(&5u32.to_le_bytes());
    let p = parse_leaked(papk);
    assert_eq!(p.class_count(), Ok(5));
    assert!(matches!(p.classes(), Err(PapkError::Classes(_))));
    assert!(matches!(p.class_section(), Err(PapkError::Classes(_))));
}

#[test]
fn asset_count_reports_declared_or_none() {
    let plain = parse_leaked(build_test_papk("foo/Bar", &[]));
    assert_eq!(plain.asset_count(), Ok(None));

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
    for field in [12usize, 16, 20, 24] {
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
        PapkError::Classes(class_link::LinkError::Truncated),
    ] {
        assert!(!e.to_string().is_empty());
    }
}
