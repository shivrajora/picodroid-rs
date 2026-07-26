// SPDX-License-Identifier: GPL-3.0-only
//! Hand-rolled deterministic generative tests (no proptest — the workspace
//! stays dependency-free). A tiny xorshift64 PRNG with a constant seed drives:
//!
//! 1. pack -> parse round-trips: field-for-field equality plus the layout
//!    invariants (asset data 4-byte alignment, `section_count` 2/3,
//!    `assets_offset == 0` iff no assets);
//! 2. no-panic robustness: every strict prefix of a generated file, and
//!    single-byte corruptions across the first 64 bytes, run through the
//!    parser, `validate_structure`, and both manifest scanners.
//!
//! Failures print the generation index; reproduce by keeping SEED fixed.
#![cfg(feature = "write")]

use papk_format::{
    find_manifest_value, find_manifest_value_in_prefix, keys, validate_structure, AssetSpec,
    EntryPoint, ManifestSpec, Papk, PapkBuilder,
};

const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// xorshift64 — deterministic, dependency-free.
struct XorShift(u64);

impl XorShift {
    fn new(seed: u64) -> Self {
        assert_ne!(seed, 0);
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform-ish value in `0..n` (n > 0).
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    fn ascii_string(&mut self, len: usize) -> String {
        const CHARSET: &[u8] =
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789/_-.";
        (0..len)
            .map(|_| CHARSET[self.below(CHARSET.len())] as char)
            .collect()
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u64() as u8).collect()
    }
}

struct GenAsset {
    name: String,
    width: u16,
    height: u16,
    cf: u8,
    stride: u16,
    data: Vec<u8>,
}

/// Owned storage for one generated papk spec (the builder borrows from it).
struct GenSpec {
    entry_kind: usize, // 0 = main-class, 1 = activity, 2 = application
    entry: String,
    package: String,
    version: String,
    fmv: String,
    extras: Vec<(String, String)>,
    classes: Vec<(String, Vec<u8>)>,
    assets: Vec<GenAsset>,
}

fn gen_spec(rng: &mut XorShift, max_class_blob: usize, max_asset_data: usize) -> GenSpec {
    let mut extras = Vec::new();
    for i in 0..rng.below(8) {
        // "x{i}-" prefix: unique, never collides with the fixed keys.
        let key_len = 1 + rng.below(12);
        let key = format!("x{i}-{}", rng.ascii_string(key_len));
        let value_len = rng.below(48);
        let value = rng.ascii_string(value_len);
        extras.push((key, value));
    }
    let mut classes = Vec::new();
    for _ in 0..rng.below(16) {
        let name_len = 1 + rng.below(64);
        let name = rng.ascii_string(name_len);
        let blob_len = rng.below(max_class_blob + 1);
        let blob = rng.bytes(blob_len);
        classes.push((name, blob));
    }
    let mut assets = Vec::new();
    for _ in 0..rng.below(4) {
        let name_len = 1 + rng.below(24);
        let name = rng.ascii_string(name_len);
        // Arbitrary (incl. odd) lengths to force padding paths.
        let data_len = rng.below(max_asset_data + 1);
        let data = rng.bytes(data_len);
        assets.push(GenAsset {
            name,
            width: 1 + rng.below(64) as u16,
            height: 1 + rng.below(64) as u16,
            cf: rng.next_u64() as u8,
            stride: rng.next_u64() as u16,
            data,
        });
    }
    let entry_kind = rng.below(3);
    let entry_len = 1 + rng.below(40);
    let entry = rng.ascii_string(entry_len);
    let package_len = 1 + rng.below(24);
    let package = rng.ascii_string(package_len);
    let version_len = 1 + rng.below(12);
    let version = rng.ascii_string(version_len);
    let fmv_len = 1 + rng.below(12);
    let fmv = rng.ascii_string(fmv_len);
    GenSpec {
        entry_kind,
        entry,
        package,
        version,
        fmv,
        extras,
        classes,
        assets,
    }
}

fn build(spec: &GenSpec) -> Vec<u8> {
    let entry = match spec.entry_kind {
        0 => EntryPoint::MainClass(&spec.entry),
        1 => EntryPoint::Activity(&spec.entry),
        _ => EntryPoint::Application(&spec.entry),
    };
    let mut b = PapkBuilder::new(ManifestSpec {
        entry,
        package_name: &spec.package,
        version: &spec.version,
        framework_map_version: &spec.fmv,
    });
    for (k, v) in &spec.extras {
        b.manifest_entry(k, v);
    }
    for (name, blob) in &spec.classes {
        b.class(name, blob);
    }
    for a in &spec.assets {
        b.asset(AssetSpec {
            name: &a.name,
            width: a.width,
            height: a.height,
            cf: a.cf,
            stride: a.stride,
            data: &a.data,
        });
    }
    b.build().expect("generated spec must build")
}

#[test]
fn round_trip_200_random_papks() {
    let mut rng = XorShift::new(SEED);
    for case in 0..200 {
        let spec = gen_spec(&mut rng, 4096, 1024);
        let file = build(&spec);
        let p = Papk::parse(&file).unwrap_or_else(|e| panic!("case {case}: parse failed: {e}"));

        // ── Manifest: exact entry order and content. ─────────────────────
        let entry_key: &[u8] = match spec.entry_kind {
            0 => keys::MAIN_CLASS,
            1 => keys::ACTIVITY,
            _ => keys::APPLICATION,
        };
        let mut expected: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (entry_key.to_vec(), spec.entry.as_bytes().to_vec()),
            (
                keys::PACKAGE_NAME.to_vec(),
                spec.package.as_bytes().to_vec(),
            ),
            (keys::VERSION.to_vec(), spec.version.as_bytes().to_vec()),
            (
                keys::FRAMEWORK_MAP_VERSION.to_vec(),
                spec.fmv.as_bytes().to_vec(),
            ),
        ];
        for (k, v) in &spec.extras {
            expected.push((k.as_bytes().to_vec(), v.as_bytes().to_vec()));
        }
        let got: Vec<(Vec<u8>, Vec<u8>)> = p
            .manifest()
            .unwrap()
            .map(|e| (e.key.to_vec(), e.value.to_vec()))
            .collect();
        assert_eq!(got, expected, "case {case}: manifest mismatch");

        // Lookups agree with iteration.
        assert_eq!(p.manifest_value(entry_key), Some(spec.entry.as_str()));
        assert_eq!(p.framework_map_version(), Some(spec.fmv.as_str()));
        for (k, v) in &spec.extras {
            assert_eq!(p.manifest_value(k.as_bytes()), Some(v.as_str()));
        }
        // Both scanners agree on a complete, well-formed file.
        assert_eq!(
            find_manifest_value(&file, keys::FRAMEWORK_MAP_VERSION),
            Some(spec.fmv.as_str()),
            "case {case}"
        );
        assert_eq!(
            find_manifest_value_in_prefix(&file, keys::FRAMEWORK_MAP_VERSION),
            Some(spec.fmv.as_str()),
            "case {case}"
        );
        validate_structure(&file).unwrap_or_else(|e| panic!("case {case}: {e}"));

        // ── Classes: declared count and field-for-field equality. ────────
        assert_eq!(p.class_count(), Ok(spec.classes.len() as u32));
        let classes: Vec<_> = p.classes().unwrap().collect();
        assert_eq!(classes.len(), spec.classes.len(), "case {case}");
        for (got, (name, blob)) in classes.iter().zip(&spec.classes) {
            assert_eq!(got.name, name.as_bytes(), "case {case}");
            assert_eq!(got.data, blob.as_slice(), "case {case}");
        }

        // ── Assets + layout invariants. ──────────────────────────────────
        let hdr = p.file_header();
        if spec.assets.is_empty() {
            assert_eq!(hdr.assets_offset, 0, "case {case}");
            assert_eq!(hdr.section_count, 2, "case {case}");
            assert!(p.assets().unwrap().is_none(), "case {case}");
            assert_eq!(p.asset_count(), Ok(None), "case {case}");
        } else {
            assert_ne!(hdr.assets_offset, 0, "case {case}");
            assert_eq!(hdr.section_count, 3, "case {case}");
            assert_eq!(p.asset_count(), Ok(Some(spec.assets.len() as u32)));
            let (_, section) = p.assets_section().unwrap().unwrap();
            let assets: Vec<_> = p.assets().unwrap().unwrap().collect();
            assert_eq!(assets.len(), spec.assets.len(), "case {case}");
            for (got, want) in assets.iter().zip(&spec.assets) {
                assert_eq!(got.name, want.name.as_bytes(), "case {case}");
                assert_eq!(got.width, want.width, "case {case}");
                assert_eq!(got.height, want.height, "case {case}");
                assert_eq!(got.cf, want.cf, "case {case}");
                assert_eq!(got.stride, want.stride, "case {case}");
                assert_eq!(got.data, want.data.as_slice(), "case {case}");
                // Pixel data must sit on a 4-byte boundary within the section.
                let off = (got.data.as_ptr() as usize).wrapping_sub(section.as_ptr() as usize);
                assert_eq!(off % 4, 0, "case {case}: asset data misaligned");
            }
        }

        // Header basics the writer must always emit.
        assert_eq!(hdr.version_major, 1, "case {case}");
        assert_eq!(hdr.version_minor, 1, "case {case}");
        assert_eq!(hdr.manifest_offset, 24, "case {case}");
    }
}

/// Exercise the full read surface on a possibly-garbage buffer; the only
/// failure mode under test is a panic.
fn exercise(bytes: &[u8]) {
    if let Ok(p) = Papk::parse(bytes) {
        let _ = p.file_header();
        if let Ok(mi) = p.manifest() {
            for e in mi {
                let _ = (e.key, e.value);
            }
        }
        if let Ok(ci) = p.classes() {
            for e in ci {
                let _ = (e.name, e.data);
            }
        }
        if let Ok(Some(ai)) = p.assets() {
            for e in ai {
                let _ = (e.name, e.data);
            }
        }
        let _ = p.main_class();
        let _ = p.activity();
        let _ = p.application();
        let _ = p.framework_map_version();
        let _ = p.verify_compat("0.1.0");
        let _ = p.class_count();
        let _ = p.asset_count();
        let _ = p.manifest_section();
        let _ = p.classes_section();
        let _ = p.assets_section();
    }
    let _ = validate_structure(bytes);
    let _ = find_manifest_value(bytes, keys::FRAMEWORK_MAP_VERSION);
    let _ = find_manifest_value_in_prefix(bytes, keys::FRAMEWORK_MAP_VERSION);
}

#[test]
fn no_panic_on_any_strict_prefix() {
    let mut rng = XorShift::new(SEED ^ 0xDEAD_BEEF);
    for _ in 0..50 {
        // Smaller blobs keep the prefix sweep fast; overflow-checks are on in
        // the test profile, so the offset arithmetic is exercised hard.
        let spec = gen_spec(&mut rng, 256, 128);
        let file = build(&spec);
        for n in 0..=file.len() {
            exercise(&file[..n]);
        }
    }
}

#[test]
fn no_panic_on_single_byte_corruption() {
    let mut rng = XorShift::new(SEED ^ 0x0BAD_CAFE);
    for _ in 0..50 {
        let spec = gen_spec(&mut rng, 256, 128);
        let file = build(&spec);
        // Corrupt each of the first 64 bytes (file header + first section
        // header + start of the manifest) — the offset/length fields live
        // there — with several interesting values.
        for pos in 0..file.len().min(64) {
            for val in [0x00, 0x01, 0x7F, 0x80, 0xFF, rng.next_u64() as u8] {
                let mut corrupted = file.clone();
                corrupted[pos] = val;
                exercise(&corrupted);
            }
        }
    }
}
