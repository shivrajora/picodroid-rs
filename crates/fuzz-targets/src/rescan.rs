// SPDX-License-Identifier: GPL-3.0-only
//! The boot-time walk over the app region: flash as a torn install, a
//! duplicated run, a scribbled header or a reflash may have left it.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::OnceLock;

use class_link::build::simple_class;
use class_link::name_hash;
use papk_format::flash_image::{
    build_commit_page, build_header_page, build_verified_page, is_committed, parse_meta,
    COMMIT_OFFSET, META_SIZE, PAGE_LEN, VERIFIED_OFFSET,
};
use papk_format::{
    validate_embedded, validate_structure, AssetSpec, EntryPoint, ManifestSpec, Papk, PapkBuilder,
};
use pd_install::mem_region::{MemRegion, MemTransport, NoCoordinator};
use pd_install::{run_sectors, scan_region, Found};

use crate::stubs::{FirstFit, FRAMEWORK_MAP_VERSION};
use crate::Bytes;

const SECTORS: usize = 16;
const REGION_LEN: usize = SECTORS * META_SIZE;

thread_local! {
    static REGION: RefCell<MemRegion> = RefCell::new(MemRegion::new(SECTORS, 8));
}

/// The class whose name the stand-in framework owns.
const FRAMEWORK_CLASS: &[u8] = b"picodroid/app/Activity";

fn papk(package: &str, class: &[u8], pad: usize) -> Vec<u8> {
    let class_name = core::str::from_utf8(class).unwrap();
    let main = simple_class(class, b"java/lang/Object", Some(&[0x03, 0xAC]));
    let mut b = PapkBuilder::new(ManifestSpec {
        entry: EntryPoint::MainClass(class_name),
        package_name: package,
        version: "1.0",
        framework_map_version: FRAMEWORK_MAP_VERSION,
        version_code: Some(1),
        label: Some("Seed"),
        icon: (pad > 0).then_some("pad.png"),
        design_size: None,
        requires_features: None,
    });
    b.class(class_name, &main);
    let data = vec![0x5Au8; pad];
    if pad > 0 {
        b.asset(AssetSpec {
            name: "pad.png",
            width: 1,
            height: (pad / 4) as u16,
            cf: 0x12,
            stride: 0,
            data: &data,
        });
    }
    b.build().unwrap()
}

/// Three images, built once: a one-sector app, a two-sector app, and one
/// whose class is named like a framework class.
fn images() -> &'static [Vec<u8>; 3] {
    static IMAGES: OnceLock<[Vec<u8>; 3]> = OnceLock::new();
    IMAGES.get_or_init(|| {
        [
            papk("com.a", b"a/Main", 0),
            papk("com.b", b"b/Main", META_SIZE + 512),
            papk("com.c", FRAMEWORK_CLASS, 0),
        ]
    })
}

/// The firmware's class-hash check, over a one-class framework.
fn classes_ok(image: &[u8]) -> bool {
    let forbidden = name_hash(FRAMEWORK_CLASS);
    match Papk::parse(image).and_then(|p| p.class_section()) {
        Ok(section) => section.index().iter().all(|e| e.hash != forbidden),
        Err(_) => false,
    }
}

/// The boot-meta pages of a baked run, as the input chooses them.
struct Meta {
    len: u32,
    flags: u32,
    seq: u32,
    commit: bool,
    verified: bool,
}

/// Write a run at `sector` into `model`: header page, optional commit and
/// verified pages, and as much of the image as fits.
fn bake(model: &mut [u8], sector: usize, image: &[u8], meta: Meta) {
    let at = sector * META_SIZE;
    if at + META_SIZE > model.len() {
        return;
    }
    model[at..at + PAGE_LEN].copy_from_slice(&build_header_page(meta.len, meta.flags, meta.seq));
    if meta.commit {
        model[at + COMMIT_OFFSET..at + COMMIT_OFFSET + PAGE_LEN]
            .copy_from_slice(&build_commit_page());
    }
    if meta.verified {
        model[at + VERIFIED_OFFSET..at + VERIFIED_OFFSET + PAGE_LEN]
            .copy_from_slice(&build_verified_page());
    }
    let start = at + META_SIZE;
    let n = image.len().min(model.len().saturating_sub(start));
    model[start..start + n].copy_from_slice(&image[..n]);
}

/// Build the region's contents from the input: raw bytes over the start,
/// or a script of bakes, erases, scribbles and copies.
fn build_model(b: &mut Bytes<'_>) -> Vec<u8> {
    let mut model = vec![0xFFu8; REGION_LEN];
    if b.u8() & 1 == 0 {
        let raw = b.rest();
        let n = raw.len().min(REGION_LEN);
        model[..n].copy_from_slice(&raw[..n]);
        return model;
    }
    let imgs = images();
    let mut ops = 0;
    while !b.is_empty() && ops < 64 {
        ops += 1;
        match b.u8() % 6 {
            0 => {
                let sector = b.below(SECTORS);
                let image = &imgs[b.u8() as usize % 3];
                let len = match b.u8() % 4 {
                    0 => image.len() as u32,
                    1 => image.len() as u32 + 1,
                    2 => 0,
                    _ => b.u32(),
                };
                let flags = b.u8() as u32 & 1;
                let seq = b.u32();
                let bits = b.u8();
                bake(
                    &mut model,
                    sector,
                    image,
                    Meta {
                        len,
                        flags,
                        seq,
                        commit: bits & 1 != 0,
                        verified: bits & 2 != 0,
                    },
                );
            }
            1 => {
                let s = b.below(SECTORS);
                let n = b.below(SECTORS) + 1;
                let end = ((s + n) * META_SIZE).min(REGION_LEN);
                model[s * META_SIZE..end].fill(0xFF);
            }
            2 => {
                let off = b.u32() as usize % REGION_LEN;
                let bytes = b.take(64);
                let n = bytes.len().min(REGION_LEN - off);
                model[off..off + n].copy_from_slice(&bytes[..n]);
            }
            3 => {
                let sector = b.below(SECTORS);
                let (len, flags, seq) = (b.u32(), b.u32(), b.u32());
                let at = sector * META_SIZE;
                model[at..at + PAGE_LEN].copy_from_slice(&build_header_page(len, flags, seq));
                if b.u8() & 1 != 0 {
                    model[at + COMMIT_OFFSET..at + COMMIT_OFFSET + PAGE_LEN]
                        .copy_from_slice(&build_commit_page());
                }
            }
            4 => {
                let (from, to) = (b.below(SECTORS), b.below(SECTORS));
                let page: [u8; META_SIZE] =
                    model[from * META_SIZE..][..META_SIZE].try_into().unwrap();
                model[to * META_SIZE..][..META_SIZE].copy_from_slice(&page);
            }
            _ => {
                let off = b.u32() as usize % REGION_LEN;
                model[off] ^= 1 << (b.u8() % 8);
            }
        }
    }
    model
}

/// Everything the walk must say about `region`, checked.
fn check_walk(region: &[u8]) -> Vec<Found<'_>> {
    let found: Vec<Found<'_>> = scan_region(region, classes_ok).collect();
    let again: Vec<Found<'_>> = scan_region(region, classes_ok).collect();
    assert_eq!(found, again, "the walk is not deterministic");

    let sectors = (region.len() / META_SIZE) as u32;
    let mut covered = BTreeSet::new();
    let mut last_end = 0u32;
    for f in &found {
        let (first, span) = f.span();
        assert!(span >= 1 && first + span <= sectors, "{f:?} out of bounds");
        assert!(first >= last_end, "{f:?} overlaps the finding before it");
        last_end = first + span;
        for s in first..first + span {
            covered.insert(s);
        }
        let header = &region[first as usize * META_SIZE..][..META_SIZE];
        let max_len = (sectors - first - 1) as usize * META_SIZE;
        match *f {
            Found::App {
                meta,
                image,
                verified,
                ..
            } => {
                assert_eq!(span, run_sectors(meta.len as usize));
                assert_eq!(image.len(), meta.len as usize);
                let at = (first + 1) as usize * META_SIZE;
                assert!(core::ptr::eq(image, &region[at..at + image.len()]));
                assert!(is_committed(header));
                if verified {
                    assert!(validate_embedded(image).is_ok());
                } else {
                    assert!(validate_structure(image).is_ok());
                }
                assert!(Papk::parse(image).unwrap().package_name().is_some());
                assert!(classes_ok(image));
            }
            Found::Collides { image, .. } => {
                assert_eq!(span, run_sectors(image.len()));
                assert!(is_committed(header));
                assert!(!classes_ok(image));
            }
            Found::Stale { .. } => {
                let meta = parse_meta(header, max_len);
                assert!(meta.is_none(), "stale but committed");
                assert!(papk_format::flash_image::parse_header(header, max_len).is_some());
            }
            Found::Garbage { .. } => {
                assert!(papk_format::flash_image::parse_header(header, max_len).is_some());
            }
        }
    }
    // Completeness: a sector the walk stepped over holds no committed,
    // valid, named run.
    for s in 0..sectors {
        if covered.contains(&s) {
            continue;
        }
        let header = &region[s as usize * META_SIZE..][..META_SIZE];
        let max_len = (sectors - s - 1) as usize * META_SIZE;
        if let Some(meta) = parse_meta(header, max_len) {
            let at = (s + 1) as usize * META_SIZE;
            let image = &region[at..at + meta.len as usize];
            let named = validate_structure(image).is_ok()
                && Papk::parse(image).unwrap().package_name().is_some();
            assert!(!named, "a run at sector {s} was missed");
        }
    }
    found
}

/// Input: a mode byte, then raw region bytes or a script of edits; then,
/// for the install tie-in, an image choice and a sector to uninstall.
///
/// The walk is deterministic, stays in bounds, never overlaps itself, and
/// classifies every run as the firmware does (committed + valid + named is
/// an app; valid but colliding occupies its span; a bare header is stale;
/// anything else steps one sector). It misses no committed valid run. An
/// install onto the region adds exactly one app to what the walk reports,
/// and an uninstall removes exactly that run.
pub fn rescan(data: &[u8]) {
    let mut b = Bytes::new(data);
    let tail_choice = b.u8();
    let model = build_model(&mut b);
    REGION.with_borrow_mut(|region| {
        region.erase_all();
        region.restore(&model);
        let before: Vec<(u32, u32)> = check_walk(region.bytes()).iter().map(Found::span).collect();

        // Install the chosen seed after everything the walk found.
        let image = &images()[tail_choice as usize % 2];
        let mut dir = FirstFit::<true>::new(SECTORS as u32);
        for (first, sectors) in &before {
            dir.installed.push(crate::stubs::Run {
                package: String::new(),
                first: *first,
                sectors: *sectors,
            });
        }
        let next = before.iter().map(|(f, s)| f + s).max().unwrap_or(0);
        let fits = next + run_sectors(image.len()) <= SECTORS as u32 && before.len() < 8;
        let mut t = MemTransport::for_papk(image);
        let ok = pd_install::install(
            &mut t,
            &mut NoCoordinator,
            region,
            &mut dir,
            &mut [0u8; 256],
            FRAMEWORK_MAP_VERSION,
            image.len() as u32,
        );
        assert_eq!(
            ok,
            fits,
            "install outcome vs room after the walk ({:?}; {} runs before)",
            t.error,
            before.len()
        );
        let after = check_walk(region.bytes());
        let spans: Vec<(u32, u32)> = after.iter().map(Found::span).collect();
        if ok {
            assert_eq!(
                spans.len(),
                before.len() + 1,
                "install changed the walk elsewhere"
            );
            assert_eq!(&spans[..before.len()], &before[..]);
            match after.last().unwrap() {
                Found::App {
                    first_sector,
                    image: found,
                    ..
                } => {
                    assert_eq!(*first_sector, next);
                    assert_eq!(*found, &image[..]);
                }
                other => panic!("the installed run reads as {other:?}"),
            }
        } else {
            assert_eq!(spans, before, "a refused install changed the region");
            return;
        }

        // Uninstall one app the walk reports; the rest stay as they were.
        let apps: Vec<(u32, u32)> = after
            .iter()
            .filter(|f| matches!(f, Found::App { .. }))
            .map(Found::span)
            .collect();
        let (first, sectors) = apps[(tail_choice >> 2) as usize % apps.len()];
        let mut t = MemTransport::for_papk(&[]);
        assert!(pd_install::uninstall(
            &mut t,
            &mut NoCoordinator,
            region,
            &mut dir,
            first,
            sectors
        ));
        let gone: Vec<(u32, u32)> = check_walk(region.bytes()).iter().map(Found::span).collect();
        let expected: Vec<(u32, u32)> =
            spans.iter().copied().filter(|&(f, _)| f != first).collect();
        assert_eq!(gone, expected, "uninstall of {first}+{sectors}");
        assert!(region.sector(first).iter().all(|&x| x == 0xFF));
    });
}
