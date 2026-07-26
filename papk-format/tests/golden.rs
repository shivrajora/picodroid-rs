// SPDX-License-Identifier: GPL-3.0-only
//! Golden-fixture tests: the checked-in `.papk` files were produced by the
//! PRE-refactor `papk-pack` CLI (see tests/fixtures/README.md for the exact
//! invocations). They are the "behavior-preserving, format unchanged" proof:
//! the parser must extract the known contents, and (with the `write` feature)
//! `PapkBuilder` must reproduce each file byte-for-byte.

use papk_format::{keys, FileHeader, Papk};

static MINIMAL: &[u8] = include_bytes!("fixtures/minimal.papk");
static WITH_ASSETS: &[u8] = include_bytes!("fixtures/with-assets.papk");
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

    // Classes: exactly the checked-in Main.class.
    assert_eq!(p.class_count(), Ok(1));
    let classes: Vec<_> = p.classes().unwrap().collect();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].name, b"fixture/Main");
    assert_eq!(classes[0].data, MAIN_CLASS);
}

#[test]
fn minimal_fixture_parses_to_known_content() {
    let p = Papk::parse(MINIMAL).unwrap();
    assert_common_content(&p);
    assert!(p.assets().unwrap().is_none());
    assert_eq!(p.asset_count(), Ok(None));

    let hdr = p.file_header();
    assert_eq!(hdr.version_major, 1);
    assert_eq!(hdr.version_minor, 1);
    assert_eq!(hdr.section_count, 2);
    assert_eq!(hdr.manifest_offset, 24);
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

/// Byte-for-byte writer equality with the pre-refactor papk-pack output.
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
        });
        b.class("fixture/Main", MAIN_CLASS);
        b
    }

    #[test]
    fn builder_reproduces_minimal_fixture_byte_for_byte() {
        let bytes = builder().build().unwrap();
        assert_eq!(bytes, MINIMAL);
    }

    #[test]
    fn builder_reproduces_with_assets_fixture_byte_for_byte() {
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
        assert_eq!(bytes, WITH_ASSETS);
    }
}
