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
    // Typed accessors over the same four keys; the identity keys that came
    // later are absent on these pre-refactor files.
    assert_eq!(p.package_name(), Some("fixture"));
    assert_eq!(p.version(), Some("1.0"));
    assert_eq!(p.version_code(), None);
    assert_eq!(p.label(), None);
    assert_eq!(p.icon(), None);

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

/// Writer equality with the pre-refactor papk-pack output, section by section.
///
/// These were byte-for-byte comparisons until the writer started aligning
/// every section to 4 bytes (2026-09-15): the old layout put `imagedemo`'s
/// pixels on an odd flash address, which is a HardFault the first time LVGL
/// reads them through a `uint16_t *` on Cortex-M0+. The fixtures stay exactly
/// as papk-pack produced them — they are also the proof that the *reader*
/// still handles the unaligned files already installed on devices — so the
/// comparison is now "every section identical, only its offset moved".
#[cfg(feature = "write")]
mod rebuild {
    use super::*;
    use papk_format::{AssetSpec, EntryPoint, ManifestSpec, PapkBuilder, SECTION_HEADER_LEN};

    /// Assert `built` carries the same sections as `fixture`, allowing only
    /// the file-header offset words and the padding between sections to
    /// differ — and that the new offsets are 4-byte aligned.
    fn assert_same_sections(built: &[u8], fixture: &[u8]) {
        let b = FileHeader::parse(built).unwrap();
        let f = FileHeader::parse(fixture).unwrap();
        assert_eq!(b.version_major, f.version_major);
        assert_eq!(b.version_minor, f.version_minor);
        assert_eq!(b.section_count, f.section_count);
        assert_eq!(b.manifest_offset, f.manifest_offset);

        let pairs = [
            ("MANIFEST", b.manifest_offset, f.manifest_offset),
            ("CLASSES", b.classes_offset, f.classes_offset),
            ("ASSETS", b.assets_offset, f.assets_offset),
        ];
        for (name, boff, foff) in pairs {
            if foff == 0 {
                assert_eq!(
                    boff, 0,
                    "{name}: absent in the fixture, present in the build"
                );
                continue;
            }
            assert_eq!(
                boff % 4,
                0,
                "{name} starts at {boff}, which is not 4-aligned"
            );
            // Length lives at +4 of the section header; compare header+data.
            let len = u32::from_le_bytes(
                fixture[foff as usize + 4..foff as usize + 8]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let span = SECTION_HEADER_LEN + len;
            assert_eq!(
                &built[boff as usize..boff as usize + span],
                &fixture[foff as usize..foff as usize + span],
                "{name} section bytes differ"
            );
        }
        // Everything the offsets skipped over is zero padding, nothing else.
        assert!(
            built.len() - fixture.len() < 4 * pairs.len(),
            "build grew by more than section alignment padding"
        );
    }

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
    fn builder_reproduces_minimal_fixture_section_for_section() {
        let bytes = builder().build().unwrap();
        assert_same_sections(&bytes, MINIMAL);
        assert_common_content(&Papk::parse(&bytes).unwrap());
    }

    #[test]
    fn builder_reproduces_with_assets_fixture_section_for_section() {
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
        assert_same_sections(&bytes, WITH_ASSETS);

        // This fixture's CLASSES section happens to end on a 4-byte boundary,
        // so it never showed the misalignment — part of why the bug survived.
        // `write.rs`'s `sections_and_asset_data_are_4_byte_aligned_in_the_file`
        // sweeps the class lengths that do expose it; here we only confirm the
        // result is aligned.
        let built = Papk::parse(&bytes).unwrap();
        let entry = built.assets().unwrap().unwrap().next().unwrap();
        let off = (entry.data.as_ptr() as usize).wrapping_sub(bytes.as_ptr() as usize);
        assert_eq!(off % 4, 0, "rebuilt asset data must be 4-byte aligned");
        assert_eq!(entry.data, &pixels[..]);
    }
}
