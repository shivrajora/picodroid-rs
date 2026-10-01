// SPDX-License-Identifier: GPL-3.0-only
//! Golden-fixture tests: the checked-in `.papk` files were produced by
//! `papk-pack` (see tests/fixtures/README.md for the exact invocations) and
//! pin the on-disk layout: the parser must extract the known contents, and
//! (with the `write` feature) `PapkBuilder` must reproduce each file byte
//! for byte.

use papk_format::{keys, FileHeader, Papk, FILE_HEADER_LEN, VERSION_MAJOR, VERSION_MINOR};

/// `include_bytes!` data is only byte-aligned; the class section is read
/// in place as `u16` words and 8-byte index entries.
#[repr(C, align(8))]
struct Aligned<const N: usize>([u8; N]);

const MINIMAL_LEN: usize = include_bytes!("fixtures/minimal.papk").len();
const WITH_ASSETS_LEN: usize = include_bytes!("fixtures/with-assets.papk").len();
static MINIMAL_A: Aligned<MINIMAL_LEN> = Aligned(*include_bytes!("fixtures/minimal.papk"));
static WITH_ASSETS_A: Aligned<WITH_ASSETS_LEN> =
    Aligned(*include_bytes!("fixtures/with-assets.papk"));
static MINIMAL: &[u8] = &MINIMAL_A.0;
static WITH_ASSETS: &[u8] = &WITH_ASSETS_A.0;
static MAIN_CLASS: &[u8] = include_bytes!("fixtures/Main.class");

/// The RGB565 payload papk-pack decoded from fixtures/gradient.png, computed
/// independently from the PNG's generation formula (pixel (x, y) has
/// r = x*32, g = y*32, b = (x^y)*32 — see fixtures/README.md).
fn expected_gradient_rgb565() -> Vec<u8> {
    let mut out = Vec::with_capacity(8 * 8 * 2);
    for y in 0..8u16 {
        for x in 0..8u16 {
            let r = (x * 32) as u8;
            let g = (y * 32) as u8;
            let b = ((x ^ y) * 32) as u8;
            let v: u16 = (((r >> 3) as u16) << 11) | (((g >> 2) as u16) << 5) | ((b >> 3) as u16);
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

fn assert_common_content(p: &Papk<'_>) {
    // Manifest: exact key order as papk-pack emits it.
    let entries: Vec<(Vec<u8>, Vec<u8>)> = p
        .manifest()
        .unwrap()
        .map(|e| (e.key.to_vec(), e.value.to_vec()))
        .collect();
    assert_eq!(
        entries,
        [
            (b"main-class".to_vec(), b"fixture/Main".to_vec()),
            (b"package-name".to_vec(), b"fixture".to_vec()),
            (b"version".to_vec(), b"1.0".to_vec()),
            (b"framework-map-version".to_vec(), b"0.0.0".to_vec()),
        ]
    );
    assert_eq!(p.main_class(), Some("fixture/Main"));
    assert_eq!(p.activity(), None);
    assert_eq!(p.application(), None);
    assert_eq!(p.manifest_value(keys::PACKAGE_NAME), Some("fixture"));
    assert_eq!(p.manifest_value(keys::VERSION), Some("1.0"));
    assert_eq!(p.framework_map_version(), Some("0.0.0"));
    // Typed accessors over the same four keys; the identity keys are absent
    // on these files.
    assert_eq!(p.package_name(), Some("fixture"));
    assert_eq!(p.version(), Some("1.0"));
    assert_eq!(p.version_code(), None);
    assert_eq!(p.label(), None);
    assert_eq!(p.icon(), None);

    // Classes: exactly the checked-in Main.class, with its link table.
    assert_eq!(p.class_count(), Ok(1));
    let classes: Vec<_> = p.classes().unwrap().collect();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].name, b"fixture/Main");
    assert_eq!(classes[0].data, MAIN_CLASS);
    assert_eq!(classes[0].link.methods_len(), 2, "<init> and main");
    classes[0].link.validate(classes[0].data).unwrap();
    let section = p.class_section().unwrap();
    section.validate().unwrap();
    assert_eq!(section.find_class(b"fixture/Main"), Some(0));

    let hdr = p.file_header();
    assert_eq!(hdr.version_major, VERSION_MAJOR);
    assert_eq!(hdr.version_minor, VERSION_MINOR);
    assert_eq!(hdr.manifest_offset as usize, FILE_HEADER_LEN);
    assert_eq!(hdr.classes_offset % 4, 0);
    assert_eq!(hdr.resources_offset, 0);
}

#[test]
fn minimal_fixture_parses_to_known_content() {
    let p = Papk::parse(MINIMAL).unwrap();
    assert_common_content(&p);
    assert!(p.assets().unwrap().is_none());
    assert_eq!(p.asset_count(), Ok(None));

    let hdr = p.file_header();
    assert_eq!(hdr.section_count, 2);
    assert_eq!(hdr.assets_offset, 0);
    assert_eq!(hdr, FileHeader::parse(MINIMAL).unwrap());
}

#[test]
fn with_assets_fixture_parses_to_known_content() {
    let p = Papk::parse(WITH_ASSETS).unwrap();
    assert_common_content(&p);

    let hdr = p.file_header();
    assert_eq!(hdr.section_count, 3);
    assert_ne!(hdr.assets_offset, 0);
    assert_eq!(hdr.assets_offset % 4, 0);

    assert_eq!(p.asset_count(), Ok(Some(1)));
    let assets: Vec<_> = p.assets().unwrap().expect("ASST present").collect();
    assert_eq!(assets.len(), 1);
    let a = &assets[0];
    assert_eq!(a.name, b"gradient.png");
    assert_eq!(a.width, 8);
    assert_eq!(a.height, 8);
    assert_eq!(a.cf, 0x12); // LV_COLOR_FORMAT_RGB565
    assert_eq!(a.stride, 0);
    assert_eq!(a.data, expected_gradient_rgb565());
}

#[test]
fn scanners_agree_on_fixtures() {
    for fixture in [MINIMAL, WITH_ASSETS] {
        papk_format::validate_structure(fixture).unwrap();
        assert_eq!(
            papk_format::find_manifest_value(fixture, keys::FRAMEWORK_MAP_VERSION),
            Some("0.0.0")
        );
        assert_eq!(
            papk_format::find_manifest_value_in_prefix(fixture, keys::FRAMEWORK_MAP_VERSION),
            Some("0.0.0")
        );
    }
}

/// The writer reproduces each fixture byte for byte: the fixtures are what
/// `papk-pack` (over this writer) emitted, so a change in layout here shows
/// up as a diff against a checked-in file.
#[cfg(feature = "write")]
mod rebuild {
    use super::*;
    use papk_format::{AssetSpec, EntryPoint, ManifestSpec, PapkBuilder};

    fn builder<'a>() -> PapkBuilder<'a> {
        let mut b = PapkBuilder::new(ManifestSpec {
            entry: EntryPoint::MainClass("fixture/Main"),
            package_name: "fixture",
            version: "1.0",
            framework_map_version: "0.0.0",
            version_code: None,
            label: None,
            icon: None,
        });
        b.class("fixture/Main", MAIN_CLASS);
        b
    }

    #[test]
    fn builder_reproduces_minimal_fixture() {
        let bytes = builder().build().unwrap();
        assert_eq!(
            bytes, MINIMAL,
            "minimal.papk differs from the writer's output"
        );
        assert_common_content(&Papk::parse(&papk_format::AlignedBuf::new(&bytes)).unwrap());
    }

    #[test]
    fn builder_reproduces_with_assets_fixture() {
        let pixels = expected_gradient_rgb565();
        let mut b = builder();
        b.asset(AssetSpec {
            name: "gradient.png",
            width: 8,
            height: 8,
            cf: 0x12,
            stride: 0,
            data: &pixels,
        });
        let bytes = b.build().unwrap();
        assert_eq!(
            bytes, WITH_ASSETS,
            "with-assets.papk differs from the writer's output"
        );
    }
}

/// `validate_embedded` is the structural check without the per-class deep
/// check: it still refuses a file that is not a PAPK or whose class section
/// does not fit, but a link table that disagrees with its class is only
/// the deep check's to find.
#[test]
fn validate_embedded_skips_only_the_deep_class_check() {
    use papk_format::{validate_embedded, validate_structure};
    assert_eq!(validate_structure(MINIMAL), Ok(()));
    assert_eq!(validate_embedded(MINIMAL), Ok(()));

    // Corrupt the class's name hash in its link table (word 16).
    let section = Papk::parse(MINIMAL).unwrap().class_section().unwrap();
    let table = section.class(0).unwrap().link.as_ptr() as usize - MINIMAL.as_ptr() as usize;
    let mut bad = Aligned(MINIMAL_A.0);
    bad.0[table + 2 * 16] ^= 0xFF;
    assert!(validate_structure(&bad.0).is_err());
    assert_eq!(validate_embedded(&bad.0), Ok(()));

    // Bounds and magic are still checked.
    let mut bad = Aligned(MINIMAL_A.0);
    bad.0[0] = b'X';
    assert!(validate_embedded(&bad.0).is_err());
    assert!(validate_embedded(&MINIMAL[..MINIMAL.len() - 8]).is_err());
}
