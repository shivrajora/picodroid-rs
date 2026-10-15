// SPDX-License-Identifier: GPL-3.0-only
//! The PAPK container: the reader, the install-time structural check, the
//! 512-byte install peek and the writer.

use papk_format::res::{self, config, ResTable};
use papk_format::{
    asset_bits_per_pixel, find_manifest_value, find_manifest_value_in_prefix, flash_image, keys,
    validate_embedded, validate_structure, AlignedBuf, AssetSpec, EntryPoint, ManifestSpec, Papk,
    PapkBuilder,
};

use crate::Bytes;

/// Any bytes as a PAPK. `validate_structure` is what an install, an embed
/// and a boot scan run before the JVM reads the image through accessors
/// that trust it; so whatever it accepts, every accessor must then read
/// without an error, and the class section must pass the checks
/// [`crate::class::read_validated`] holds it to.
pub fn papk(data: &[u8]) {
    // Installed images are read in place from 8-aligned flash.
    let buf = AlignedBuf::new(data);
    let bytes: &[u8] = &buf;

    let deep = validate_structure(bytes);
    let shallow = validate_embedded(bytes);
    if deep.is_ok() {
        assert!(
            shallow.is_ok(),
            "the deep check passed and the shallow one refused: {shallow:?}"
        );
    }
    // The boot-meta sector reader takes whatever flash holds, too.
    let _ = flash_image::parse_meta(bytes, bytes.len());
    let _ = (
        flash_image::is_committed(bytes),
        flash_image::is_verified(bytes),
    );

    let Ok(p) = Papk::parse(bytes) else {
        assert!(
            deep.is_err(),
            "validate_structure passed what Papk::parse refuses"
        );
        return;
    };
    read_everything(&p, deep.is_ok());
}

/// Every accessor of `p`. `trusted`: the image passed the deep check, so no
/// accessor may fail.
fn read_everything(p: &Papk<'_>, trusted: bool) {
    let _ = p.file_header();
    let _ = (
        p.main_class(),
        p.activity(),
        p.application(),
        p.package_name(),
    );
    let _ = (p.version(), p.label(), p.icon(), p.version_code());
    let _ = (p.design_size(), p.requires_features(), p.activities());
    let _ = (p.services(), p.theme(), p.framework_map_version());
    let _ = p.verify_compat("1.0.0");
    let _ = p.manifest_value(keys::PACKAGE_NAME);

    match p.manifest() {
        Ok(m) => m.for_each(|e| {
            let _ = (e.key, e.value);
        }),
        Err(e) => assert!(!trusted, "manifest() of a validated image: {e:?}"),
    }
    match p.classes() {
        Ok(c) => {
            let n = c.count();
            if trusted {
                assert_eq!(Ok(n as u32), p.class_count(), "classes() vs class_count()");
            }
        }
        Err(e) => assert!(!trusted, "classes() of a validated image: {e:?}"),
    }
    match p.class_section() {
        Ok(s) => {
            let valid = s.validate();
            if trusted {
                assert!(
                    valid.is_ok(),
                    "class section of a validated image: {valid:?}"
                );
                crate::class::read_validated(&s);
            } else {
                crate::class::read_unvalidated(&s);
            }
        }
        Err(e) => assert!(!trusted, "class_section() of a validated image: {e:?}"),
    }
    match p.assets() {
        Ok(Some(assets)) => {
            let mut n = 0u32;
            for a in assets {
                n += 1;
                // Recomputed here rather than trusted from the reader: LVGL
                // reads `row * height` bytes from `data` with no bound of
                // its own.
                let bpp = asset_bits_per_pixel(a.cf).expect("an asset of an unknown format");
                let min_row = (u32::from(a.width) * bpp).div_ceil(8) as usize;
                let row = if a.stride == 0 {
                    min_row
                } else {
                    a.stride as usize
                };
                assert!(
                    row >= min_row,
                    "stride {} under a {min_row}-byte row",
                    a.stride
                );
                assert!(
                    row * a.height as usize <= a.data.len(),
                    "{}x{} cf {:#x} stride {} in {} bytes",
                    a.width,
                    a.height,
                    a.cf,
                    a.stride,
                    a.data.len()
                );
            }
            if trusted {
                assert_eq!(p.asset_count(), Ok(Some(n)), "asset count vs records");
            }
        }
        Ok(None) => {}
        Err(e) => assert!(!trusted, "assets() of a validated image: {e:?}"),
    }
    match p.resources() {
        Ok(Some(t)) => read_resources(&t),
        Ok(None) => {}
        Err(e) => assert!(!trusted, "resources() of a validated image: {e:?}"),
    }
}

/// Every entry of every type, through the base table and through the
/// override blocks a range of screens selects.
fn read_resources(t: &ResTable<'_>) {
    // A hostile directory can declare 65535 entries per type; reading the
    // first few hundred covers every code path without making each input
    // slow.
    const MAX_ENTRIES: u16 = 256;
    let types: Vec<(u8, u16)> = t.types().collect();
    let ids = || {
        types
            .iter()
            .flat_map(|&(ty, n)| (0..n.min(MAX_ENTRIES)).map(move |e| res::res_id(ty, e)))
    };
    for (ty, _) in &types {
        let _ = (t.entry_count(*ty), res::type_name(*ty));
    }
    for id in ids() {
        let _ = (t.value(id), t.string(id), t.drawable_name(id));
        for l in [t.layout(id), t.style(id)].into_iter().flatten() {
            for i in 0..l.len().min(4096) {
                l.word(i).expect("a word inside the layout's own length");
            }
        }
    }
    let blocks: Vec<_> = t.overrides().take(64).collect();
    for b in &blocks {
        let _ = (b.len(), b.pairs().take(256).count());
    }
    for (w_dp, h_dp, touch) in [(320, 240, true), (240, 320, false), (800, 480, true)] {
        let c = config::Config { w_dp, h_dp, touch };
        let mut selected = [0u8; config::MAX_RANKED];
        let n = config::best_first(blocks.iter().map(|b| b.qualifiers), &c, &mut selected);
        let r = t.with(&selected[..n]);
        for id in ids().take(512) {
            let ty = res::res_type(id).unwrap_or(0);
            let _ = (r.value_of(ty, id), r.string(id), r.drawable_name(id));
            let _ = (r.layout(id), r.style(id));
        }
    }
}

/// The install peek: the first [`pdb_protocol::INSTALL_PEEK_BYTES`] of the
/// stream, before anything is erased, decide the compat and feature checks
/// and where the app goes. Input: a `u16` cut point, then the file.
///
/// Whatever the tolerant prefix scan finds must be what the strict
/// whole-file scan finds, whenever the latter finds anything — and always,
/// for an image that passes the deep check.
pub fn papk_prefix(data: &[u8]) {
    let mut b = Bytes::new(data);
    let cut = b.u16() as usize;
    let full = b.rest();
    let prefix = &full[..cut.min(full.len())];
    let valid = validate_structure(&AlignedBuf::new(full)).is_ok();
    for key in [
        keys::FRAMEWORK_MAP_VERSION,
        keys::PACKAGE_NAME,
        keys::REQUIRES_FEATURES,
    ] {
        let peeked = find_manifest_value_in_prefix(prefix, key);
        let strict = find_manifest_value(full, key);
        if let (Some(p), Some(s)) = (peeked, strict) {
            assert_eq!(p, s, "the peek and the file disagree on {key:?}");
        }
        if valid {
            if let Some(p) = peeked {
                assert_eq!(
                    strict,
                    Some(p),
                    "a validated file hides {key:?} from the strict scan"
                );
            }
        }
        // The whole file is its own longest prefix: where the strict scan
        // has an answer, the tolerant one gives the same.
        if strict.is_some() || valid {
            assert_eq!(
                find_manifest_value_in_prefix(full, key),
                strict,
                "the prefix scan of the whole file disagrees on {key:?}"
            );
        }
    }
}

/// The writer and the reader against each other: a PAPK built from a
/// structured input must parse back field for field and pass the deep
/// check, or the builder must refuse it. The classes come from
/// `class_link::build::simple_class` with fuzz-chosen names and bytecode.
pub fn papk_roundtrip(data: &[u8]) {
    let mut b = Bytes::new(data);
    let text = |b: &mut Bytes<'_>, max: usize| -> String {
        String::from_utf8_lossy(b.take(max)).into_owned()
    };
    let entry = text(&mut b, 40);
    let package = text(&mut b, 40);
    let version = text(&mut b, 16);
    let fmv = text(&mut b, 16);
    let label = (b.u8() & 1 == 1).then(|| text(&mut b, 24));
    let version_code = (b.u8() & 1 == 1).then(|| b.u32());
    let design = (b.u8() & 1 == 1).then(|| (b.u16(), b.u16()));

    let class_count = b.below(4);
    let mut classes: Vec<(String, Vec<u8>)> = Vec::new();
    for _ in 0..class_count {
        let name = text(&mut b, 24);
        let code = b.take(32).to_vec();
        let with_code = b.u8() & 1 == 1;
        let blob = class_link::build::simple_class(
            name.as_bytes(),
            b"java/lang/Object",
            with_code.then_some(&code[..]),
        );
        classes.push((name, blob));
    }
    let asset_count = b.below(3);
    let mut assets: Vec<(String, u16, u16, u8, u16, Vec<u8>)> = Vec::new();
    for _ in 0..asset_count {
        let name = text(&mut b, 16);
        let (w, h) = (b.u8() as u16 % 17, b.u8() as u16 % 17);
        let cf = [0x0F, 0x10, 0x12, b.u8()][b.below(4)];
        let stride = if b.u8() & 1 == 1 { b.u8() as u16 } else { 0 };
        let len = b.below(1200);
        let pixels = b.take(len).to_vec();
        assets.push((name, w, h, cf, stride, pixels));
    }

    let entry_point = match b.below(3) {
        0 => EntryPoint::MainClass(&entry),
        1 => EntryPoint::Activity(&entry),
        _ => EntryPoint::Application(&entry),
    };
    let mut builder = PapkBuilder::new(ManifestSpec {
        entry: entry_point,
        package_name: &package,
        version: &version,
        framework_map_version: &fmv,
        version_code,
        label: label.as_deref(),
        icon: None,
        design_size: design,
        requires_features: None,
    });
    for (name, blob) in &classes {
        builder.class(name, blob);
    }
    for (name, w, h, cf, stride, pixels) in &assets {
        builder.asset(AssetSpec {
            name,
            width: *w,
            height: *h,
            cf: *cf,
            stride: *stride,
            data: pixels,
        });
    }
    let Ok(file) = builder.build() else {
        return;
    };

    let buf = AlignedBuf::new(&file);
    if let Err(e) = validate_structure(&buf) {
        panic!("the builder wrote an image the deep check refuses: {e}");
    }
    let p = Papk::parse(&buf).expect("a validated image parses");
    assert_eq!(p.package_name(), Some(package.as_str()));
    assert_eq!(p.version(), Some(version.as_str()));
    assert_eq!(p.framework_map_version(), Some(fmv.as_str()));
    assert_eq!(p.label(), label.as_deref());
    assert_eq!(p.class_count(), Ok(classes.len() as u32));
    let section = p.class_section().expect("a validated class section");
    for (name, _) in &classes {
        assert!(
            section.find_class(name.as_bytes()).is_some(),
            "class {name:?} was packed and cannot be found"
        );
    }
    let got: Vec<_> = p.assets().unwrap().into_iter().flatten().collect();
    assert_eq!(got.len(), assets.len());
    for (g, (name, w, h, cf, stride, pixels)) in got.iter().zip(&assets) {
        assert_eq!(g.name, name.as_bytes());
        assert_eq!((g.width, g.height, g.cf, g.stride), (*w, *h, *cf, *stride));
        assert_eq!(g.data, &pixels[..]);
    }
    read_everything(&p, true);
}
