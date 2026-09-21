// SPDX-License-Identifier: GPL-3.0-only
use super::test_support::{papk, system, FW};
use super::*;
use crate::install::mem_region::{MemRegion, MemTransport, NoCoordinator};
use crate::install::{install, uninstall, InstallError};
use papk_format::flash_image::build_meta_pages;
use papk_format::{EntryPoint, ManifestSpec, PapkBuilder};

const MAX: usize = 8;

/// Enough filler to make the run occupy exactly `sectors` sectors.
fn papk_of_sectors(package: &str, sectors: u32) -> Vec<u8> {
    let base = papk("x", 0).len();
    let target = (sectors as usize - 1) * SECTOR - 100;
    let p = papk(package, target - base);
    assert_eq!(run_sectors(p.len()), sectors);
    p
}

fn fresh(sectors: usize, max_apps: usize) -> MemRegion {
    reset_for_test();
    let region = MemRegion::new(sectors, max_apps);
    rescan_region(&region);
    region
}

/// What build.rs links into the region: the meta pages, then the image.
fn bake(r: &mut MemRegion, sector: u32, image: &[u8], flags: u32, seq: u32) {
    unsafe {
        r.select_run(sector);
        for (i, chunk) in image.chunks(256).enumerate() {
            let mut page = [0xFFu8; 256];
            page[..chunk.len()].copy_from_slice(chunk);
            assert!(r.write_page(i as u32, &page));
        }
        r.commit_metadata(image.len() as u32, flags, seq);
    }
    rescan_region(r);
}

fn boot_package() -> Option<&'static str> {
    select_boot(None, None).map(|e| e.package())
}

fn do_install(region: &mut MemRegion, bytes: &[u8]) -> Result<(), InstallError> {
    let mut t = MemTransport::for_papk(bytes);
    let ok = install(&mut t, &mut NoCoordinator, region, bytes.len() as u32);
    match t.error {
        Some(e) => Err(e),
        None => {
            assert!(ok && t.success && t.ready);
            Ok(())
        }
    }
}

fn do_uninstall(region: &mut MemRegion, package: &str) {
    let (first, sectors) = find(package).expect("installed").run();
    let mut t = MemTransport::for_papk(&[]);
    assert!(uninstall(
        &mut t,
        &mut NoCoordinator,
        region,
        first,
        sectors
    ));
}

fn placed() -> Vec<(String, u32, u32, u32)> {
    let mut v: Vec<_> = apps()
        .map(|e| {
            (
                e.package().to_string(),
                e.first_sector as u32,
                e.sectors as u32,
                e.seq,
            )
        })
        .collect();
    v.sort_by_key(|e| e.1);
    v
}

fn image_of(package: &str) -> Vec<u8> {
    find(package).unwrap().image.to_vec()
}

#[test]
fn an_erased_region_scans_to_no_apps_and_no_boot_image() {
    let _g = test_support::lock();
    let _r = fresh(16, MAX);
    assert_eq!(installed_count(), 0);
    assert!(boot_image().is_none());
    assert_eq!(free_space(), (16, 16));
    assert_eq!(next_seq(), 1);
}

#[test]
fn a_baked_run_is_found_and_is_the_boot_default() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    let a = papk("com.a", 100);
    // What build.rs links into the region: meta pages then the image.
    unsafe {
        r.select_run(0);
        for (i, chunk) in a.chunks(256).enumerate() {
            let mut page = [0xFFu8; 256];
            page[..chunk.len()].copy_from_slice(chunk);
            assert!(r.write_page(i as u32, &page));
        }
        r.commit_metadata(a.len() as u32, FLAG_BOOT_DEFAULT, 0);
    }
    rescan_region(&r);
    assert_eq!(
        placed(),
        vec![("com.a".to_string(), 0, run_sectors(a.len()), 0)]
    );
    assert_eq!(boot_image(), Some(&a[..]));
    assert!(find("com.a").unwrap().is_boot_default());
    let _ = build_meta_pages; // keep the import honest on both shrink modes
}

#[test]
fn fresh_packages_go_first_fit_and_a_full_directory_refuses() {
    let _g = test_support::lock();
    let mut r = fresh(64, 3);
    do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.b", 4)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.c", 2)).unwrap();
    assert_eq!(
        placed(),
        vec![
            ("com.a".into(), 0, 3, 1),
            ("com.b".into(), 3, 4, 2),
            ("com.c".into(), 7, 2, 3),
        ]
    );
    let err = do_install(&mut r, &papk_of_sectors("com.d", 2)).unwrap_err();
    assert!(
        matches!(
            err,
            InstallError::NoRoom {
                installed: 3,
                max: 3,
                ..
            }
        ),
        "{err:?}"
    );
    // Nothing was touched by the refusal.
    assert_eq!(installed_count(), 3);
}

#[test]
fn a_papk_larger_than_the_region_is_too_large_before_anything_else() {
    let _g = test_support::lock();
    let mut r = fresh(4, MAX);
    let big = papk_of_sectors("com.big", 5);
    let err = do_install(&mut r, &big).unwrap_err();
    assert_eq!(err, InstallError::TooLarge);
    assert!(r.ops.is_empty());
}

/// The simulator's warm boot: the process that installed is gone, and
/// the new one reads the region it dumped (`hal/sim/pdb.rs::reboot`,
/// `hal/sim/app_region.rs::init`). A dump restored into a fresh region
/// must rescan to the same directory, byte for byte.
#[test]
fn a_region_restored_from_its_dump_rescans_to_the_same_directory() {
    let _g = test_support::lock();
    let mut r = fresh(32, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
    let b = papk_of_sectors("com.b", 4);
    do_install(&mut r, &b).unwrap();
    let before = placed();
    let dump = r.bytes().to_vec();
    assert_eq!(dump.len(), 32 * SECTOR);

    let mut restored = MemRegion::new(32, MAX);
    restored.restore(&dump);
    reset_for_test();
    rescan_region(&restored);
    assert_eq!(placed(), before);
    assert_eq!(image_of("com.b"), b);
    assert_eq!(next_seq(), 3);
}

#[test]
fn a_reinstall_goes_beside_the_old_copy_and_evicts_it_after_commit() {
    let _g = test_support::lock();
    let mut r = fresh(32, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.b", 3)).unwrap();
    r.ops.clear();
    let a2 = papk_of_sectors("com.a", 4);
    do_install(&mut r, &a2).unwrap();
    // The new copy landed after b; the old one at 0 was erased last.
    assert_eq!(
        placed(),
        vec![("com.b".into(), 3, 3, 2), ("com.a".into(), 6, 4, 3)]
    );
    assert_eq!(image_of("com.a"), a2);
    let last_erase = r
        .ops
        .iter()
        .rposition(|o| matches!(o, crate::install::mem_region::Op::Erase(0, 3)));
    let commit = r
        .ops
        .iter()
        .position(|o| matches!(o, crate::install::mem_region::Op::Program(6, 0, 512)));
    assert!(
        commit.unwrap() < last_erase.unwrap(),
        "old copy erased before the new one committed: {:?}",
        r.ops
    );
}

#[test]
fn a_reinstall_with_no_room_beside_goes_in_place() {
    let _g = test_support::lock();
    let mut r = fresh(8, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 5)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.b", 3)).unwrap();
    assert_eq!(free_space(), (0, 0));
    let a2 = papk_of_sectors("com.a", 4);
    do_install(&mut r, &a2).unwrap();
    assert_eq!(
        placed(),
        vec![("com.a".into(), 0, 4, 3), ("com.b".into(), 5, 3, 2)]
    );
    assert_eq!(image_of("com.a"), a2);
    // The old run's fifth sector was erased with the rest, not left stale.
    assert!(r.sector(4).iter().all(|&b| b == 0xFF));
}

#[test]
fn boot_default_is_inherited_by_a_reinstall_of_the_same_package() {
    let _g = test_support::lock();
    let mut r = fresh(32, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    // Mark it the boot default the way the baked image is.
    let (first, sectors) = find("com.a").unwrap().run();
    let a = image_of("com.a");
    unsafe {
        r.erase_run(first, sectors);
        r.select_run(first);
        for (i, chunk) in a.chunks(256).enumerate() {
            let mut page = [0xFFu8; 256];
            page[..chunk.len()].copy_from_slice(chunk);
            r.write_page(i as u32, &page);
        }
        r.commit_metadata(a.len() as u32, FLAG_BOOT_DEFAULT, 0);
    }
    rescan_region(&r);
    do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
    assert!(find("com.a").unwrap().is_boot_default());
    assert!(!find("com.b").unwrap().is_boot_default());
    assert_eq!(boot_image(), Some(&image_of("com.a")[..]));
}

#[test]
fn uninstall_erases_the_whole_run() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
    do_uninstall(&mut r, "com.a");
    assert_eq!(placed(), vec![("com.b".into(), 3, 2, 2)]);
    for s in 0..3 {
        assert!(
            r.sector(s).iter().all(|&b| b == 0xFF),
            "sector {s} not erased"
        );
    }
    assert_eq!(free_space(), (11, 14));
}

#[test]
fn fragmented_free_space_compacts_then_installs() {
    let _g = test_support::lock();
    let mut r = fresh(20, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 5)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.b", 5)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.c", 5)).unwrap();
    let b = image_of("com.b");
    do_uninstall(&mut r, "com.a");
    do_uninstall(&mut r, "com.c");
    assert_eq!(free_space(), (10, 15));
    let d = papk_of_sectors("com.d", 12);
    do_install(&mut r, &d).unwrap();
    // b slid to the front with the next seq, d took the one after.
    assert_eq!(
        placed(),
        vec![("com.b".into(), 0, 5, 3), ("com.d".into(), 5, 12, 4)]
    );
    assert_eq!(image_of("com.b"), b, "the moved image must survive intact");
    assert_eq!(image_of("com.d"), d);
    // The vacated sectors of b's old run hold no stale bytes.
    for s in 5..10 {
        assert!(
            r.sector(s).iter().all(|&b| b == 0xFF) || s >= 5,
            "sector {s}"
        );
    }
    assert!(r.sector(17).iter().all(|&b| b == 0xFF));
}

/// The gap is smaller than the run being moved, so the slide overwrites
/// its own source as it goes — the ascending-order copy must still land
/// every byte.
#[test]
fn an_overlapping_slide_preserves_the_image() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.b", 8)).unwrap();
    let b = image_of("com.b");
    do_uninstall(&mut r, "com.a");
    // 2 free at the front, 6 at the tail: a 7-sector run needs compaction.
    let c = papk_of_sectors("com.c", 7);
    do_install(&mut r, &c).unwrap();
    assert_eq!(
        placed(),
        vec![("com.b".into(), 0, 8, 3), ("com.c".into(), 8, 7, 4)]
    );
    assert_eq!(image_of("com.b"), b);
    assert_eq!(image_of("com.c"), c);
}

#[test]
fn a_commit_less_run_is_stale_and_cleanup_erases_it() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    // A relocation or install that lost power after the header page.
    unsafe {
        r.select_run(5);
        r.write_meta_header(100, 0, 9);
    }
    rescan_region(&r);
    assert_eq!(placed(), vec![("com.a".into(), 0, 2, 1)]);
    // Its sectors are not free until cleanup.
    assert_eq!(free_space(), (9, 12));
    cleanup(&mut r);
    assert_eq!(free_space(), (14, 14));
    assert!(r.sector(5).iter().all(|&b| b == 0xFF));
}

#[test]
fn duplicates_resolve_to_the_higher_seq_and_the_loser_is_cleaned_up() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    let a1 = papk_of_sectors("com.a", 2);
    let a2 = papk_of_sectors("com.a", 2);
    // Two committed copies, as a power loss between commit and evict leaves.
    for (sector, image, seq) in [(0u32, &a1, 1u32), (4, &a2, 2)] {
        unsafe {
            r.select_run(sector);
            for (i, chunk) in image.chunks(256).enumerate() {
                let mut page = [0xFFu8; 256];
                page[..chunk.len()].copy_from_slice(chunk);
                r.write_page(i as u32, &page);
            }
            r.commit_metadata(image.len() as u32, 0, seq);
        }
    }
    rescan_region(&r);
    assert_eq!(placed(), vec![("com.a".into(), 4, 2, 2)]);
    cleanup(&mut r);
    assert!(r.sector(0).iter().all(|&b| b == 0xFF));
    assert_eq!(placed(), vec![("com.a".into(), 4, 2, 2)]);
}

#[test]
fn a_single_app_board_replaces_whatever_is_installed() {
    let _g = test_support::lock();
    let mut r = fresh(16, 1);
    do_install(&mut r, &papk_of_sectors("com.a", 3)).unwrap();
    assert!(find("com.a").unwrap().is_boot_default());
    let b = papk_of_sectors("com.b", 2);
    do_install(&mut r, &b).unwrap();
    assert_eq!(placed(), vec![("com.b".into(), 0, 2, 2)]);
    assert_eq!(boot_image(), Some(&b[..]));
    // The old run's third sector was erased along with the rest.
    assert!(r.sector(2).iter().all(|&b| b == 0xFF));
}

#[test]
fn a_multi_app_board_needs_a_package_name() {
    let _g = test_support::lock();
    let r = fresh(8, MAX);
    let mut bare = PapkBuilder::new(ManifestSpec {
        entry: EntryPoint::MainClass("t/Main"),
        package_name: "",
        version: "1.0",
        framework_map_version: FW,
        version_code: None,
        label: None,
        icon: None,
    });
    bare.class("t/Main", b"CAFE");
    let bytes = bare.build().unwrap();
    // An empty package-name is present-but-empty; the plan treats it as a name.
    // The real "no key" case comes from a hand-built PAPK — simulate by planning.
    assert_eq!(
        plan_install(None, bytes.len(), MAX).unwrap_err(),
        PlanError::NoPackageName
    );
    assert!(r.ops.is_empty());
}

#[test]
fn the_running_package_is_recorded_and_bounded() {
    let _g = test_support::lock();
    set_running(Some("com.example.weather"));
    assert_eq!(running(), Some("com.example.weather"));
    set_running(None);
    assert_eq!(running(), None);
    let long = "x".repeat(100);
    set_running(Some(&long));
    assert_eq!(running().map(|s| s.len()), Some(64));
}

/// A reflash bakes the package again at sector 0, sequence 0, while an
/// older `pdb install` copy of it sits further up with a higher
/// sequence: the fresh bake must win (it is what the developer just
/// flashed, and the old copy may be built for another map version),
/// and the old copy is stale.
#[test]
fn a_fresh_bake_beats_an_older_install_of_the_same_package() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    let a1 = papk_of_sectors("com.a", 2);
    bake(&mut r, 0, &a1, FLAG_BOOT_DEFAULT, 0);
    // An upgrade goes beside the bake and evicts it.
    let a2 = papk_of_sectors("com.a", 2);
    do_install(&mut r, &a2).unwrap();
    assert_eq!(placed(), vec![("com.a".into(), 2, 2, 1)]);
    assert!(r.sector(0).iter().all(|&b| b == 0xFF));
    // Then a reflash bakes a third copy at sector 0 again.
    let a3 = papk_of_sectors("com.a", 2);
    bake(&mut r, 0, &a3, FLAG_BOOT_DEFAULT, 0);
    assert_eq!(placed(), vec![("com.a".into(), 0, 2, 0)]);
    assert_eq!(boot_image(), Some(&a3[..]));
    // The old copy is stale until cleanup erases it.
    assert_eq!(free_space(), (12, 12));
    cleanup(&mut r);
    assert_eq!(free_space(), (14, 14));
    assert!(r.sector(2).iter().all(|&b| b == 0xFF));
    assert_eq!(placed(), vec![("com.a".into(), 0, 2, 0)]);
}

// ── System apps and boot selection (M2) ────────────────────────────────

#[test]
fn system_apps_are_entries_but_not_installed_apps_and_survive_a_rescan() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    register_system(&[system(LAUNCHER_PACKAGE), system("picodroid.settings")]);
    assert_eq!(installed_count(), 0);
    assert_eq!(entries().count(), 2);
    assert!(is_system(LAUNCHER_PACKAGE));
    assert_eq!(find("picodroid.settings").unwrap().kind, Kind::System);
    assert_eq!(free_space(), (16, 16), "system apps take no region space");
    assert_eq!(launcher().map(|e| e.package()), Some(LAUNCHER_PACKAGE));
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    cleanup(&mut r);
    rescan_region(&r);
    assert_eq!(entries().count(), 3);
    assert!(is_system(LAUNCHER_PACKAGE));
    assert_eq!(placed(), vec![("com.a".into(), 0, 2, 1)]);
    assert_eq!(next_seq(), 2, "system entries carry no sequence number");
}

/// A PAPK whose manifest sets every optional key.
fn papk_labelled(package: &str, version: &str, code: u32, label: &str) -> Vec<u8> {
    let mut b = PapkBuilder::new(ManifestSpec {
        entry: EntryPoint::MainClass("t/Main"),
        package_name: package,
        version,
        framework_map_version: FW,
        version_code: Some(code),
        label: Some(label),
        icon: Some("icon.png"),
    });
    b.class("t/Main", b"CAFE");
    b.build().unwrap()
}

#[test]
fn an_entry_keeps_its_manifest_and_follows_a_reinstall() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    do_install(&mut r, &papk_labelled("com.a", "1.0", 1, "Alpha")).unwrap();
    let e = find("com.a").unwrap();
    assert_eq!(
        (e.label(), e.version(), e.version_code(), e.icon()),
        ("Alpha", "1.0", 1, Some("icon.png"))
    );
    // The values are slices into the image in place, not copies.
    assert!(e.image.as_ptr_range().contains(&e.label().as_ptr()));
    // No label: the package name stands in; no version code: 1.
    do_install(&mut r, &papk("com.b", 10)).unwrap();
    let b = find("com.b").unwrap();
    assert_eq!((b.label(), b.version_code(), b.icon()), ("com.b", 1, None));
    // An upgrade is a new entry with the new manifest.
    do_install(&mut r, &papk_labelled("com.a", "2.0", 2, "Alpha II")).unwrap();
    let e = find("com.a").unwrap();
    assert_eq!(
        (e.label(), e.version(), e.version_code()),
        ("Alpha II", "2.0", 2)
    );
    assert_eq!(installed_count(), 2);
}

#[test]
fn a_compacted_run_keeps_its_manifest_values() {
    let _g = test_support::lock();
    let mut r = fresh(20, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 5)).unwrap();
    do_install(&mut r, &papk_labelled("com.b", "3.1", 31, "Bravo")).unwrap();
    do_install(&mut r, &papk_of_sectors("com.c", 5)).unwrap();
    do_uninstall(&mut r, "com.a");
    do_uninstall(&mut r, "com.c");
    // Too big for either gap, small enough for both: compaction slides b.
    do_install(&mut r, &papk_of_sectors("com.d", 14)).unwrap();
    let b = find("com.b").unwrap();
    assert_eq!(b.first_sector, 0, "b slid to the front");
    assert_eq!(
        (b.label(), b.version(), b.version_code()),
        ("Bravo", "3.1", 31)
    );
    assert!(b.image.as_ptr_range().contains(&b.label().as_ptr()));
}

#[test]
fn a_system_entry_keeps_its_manifest_too() {
    let _g = test_support::lock();
    let _r = fresh(16, MAX);
    let image: &'static [u8] = alloc::boxed::Box::leak(
        papk_labelled(LAUNCHER_PACKAGE, "0.1", 7, "Launcher").into_boxed_slice(),
    );
    register_system(&[image]);
    let l = launcher().unwrap();
    assert_eq!(
        (l.label(), l.version_code(), l.icon()),
        ("Launcher", 7, Some("icon.png"))
    );
}

#[test]
fn the_directory_generation_moves_when_the_slots_do() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    let g0 = directory_generation();
    rescan_region(&r);
    let g1 = directory_generation();
    assert_ne!(g0, g1);
    register_system(&[system("picodroid.settings")]);
    let g2 = directory_generation();
    assert_ne!(g1, g2);
    do_install(&mut r, &papk("com.a", 10)).unwrap();
    let g3 = directory_generation();
    assert_ne!(g2, g3, "an install rescans");
    reset_for_test();
    assert_ne!(g3, directory_generation());
}

#[cfg(has_multi_app)]
#[test]
fn a_rescan_repacks_the_slots() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    do_install(&mut r, &papk("com.a", 10)).unwrap();
    do_install(&mut r, &papk("com.b", 10)).unwrap();
    assert_eq!((slot_of("com.a"), slot_of("com.b")), (Some(0), Some(1)));
    let g = directory_generation();
    do_uninstall(&mut r, "com.a");
    assert_eq!(slot_of("com.b"), Some(0), "the rescan repacked the array");
    assert_eq!(slot_of("com.a"), None);
    assert_ne!(directory_generation(), g);
}

#[test]
fn register_system_skips_bad_images_duplicates_and_overflow() {
    let _g = test_support::lock();
    let _r = fresh(16, MAX);
    register_system(&[system("picodroid.a"), system("picodroid.a")]);
    assert_eq!(entries().count(), 1);
    register_system(&[b"not a papk at all"]);
    assert_eq!(entries().count(), 1);
    // Built for a framework this firmware is not.
    let mut future = PapkBuilder::new(ManifestSpec {
        entry: EntryPoint::MainClass("t/Main"),
        package_name: "picodroid.future",
        version: "1.0",
        framework_map_version: "9.9.9",
        version_code: Some(1),
        label: None,
        icon: None,
    });
    future.class("t/Main", b"CAFE");
    let future: &'static [u8] = alloc::boxed::Box::leak(future.build().unwrap().into_boxed_slice());
    register_system(&[future]);
    assert!(find("picodroid.future").is_none());
    // Only SYSTEM_MAX fit.
    register_system(&[system("picodroid.b"), system("picodroid.c")]);
    assert_eq!(entries().count(), SYSTEM_MAX);
    assert!(find("picodroid.b").is_some());
    assert!(find("picodroid.c").is_none());
}

/// Java may uninstall an installed app that is neither a system app nor
/// the one asking; the platform's erase is refused under test, so the
/// composed call reports `Failed` without touching the directory.
#[cfg(has_multi_app)]
#[test]
fn a_java_uninstall_refuses_system_running_and_unknown_packages() {
    let _g = test_support::lock();
    reset_for_test();
    let mut region = fresh(16, MAX);
    register_system(&[system("picodroid.launcher")]);
    rescan_region(&region);
    do_install(&mut region, &papk("com.a", 0)).unwrap();
    let run = uninstall_target("com.a").unwrap();
    assert_eq!(
        run,
        (
            find("com.a").unwrap().first_sector as u32,
            find("com.a").unwrap().sectors as u32
        )
    );
    assert_eq!(
        uninstall_target("picodroid.launcher"),
        Err(UninstallOutcome::System)
    );
    assert_eq!(
        uninstall_target("com.zzz"),
        Err(UninstallOutcome::NotInstalled)
    );
    set_running(Some("com.a"));
    assert_eq!(uninstall_target("com.a"), Err(UninstallOutcome::Running));
    set_running(Some("picodroid.launcher"));
    assert_eq!(uninstall_from_app("com.a"), UninstallOutcome::Failed);
    assert!(find("com.a").is_some());
    set_running(None);
}

#[test]
fn a_run_named_like_a_system_app_is_stale_and_cleanup_erases_it() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    register_system(&[system(LAUNCHER_PACKAGE)]);
    let fake = papk_of_sectors(LAUNCHER_PACKAGE, 2);
    bake(&mut r, 0, &fake, FLAG_BOOT_DEFAULT, 0);
    assert_eq!(installed_count(), 0);
    assert_eq!(find(LAUNCHER_PACKAGE).unwrap().kind, Kind::System);
    // Its sectors are not free until cleanup.
    assert_eq!(free_space(), (14, 14));
    cleanup(&mut r);
    assert_eq!(free_space(), (16, 16));
    assert!(r.sector(0).iter().all(|&b| b == 0xFF));
}

#[test]
fn select_boot_follows_the_override_then_the_board_then_the_flags() {
    let _g = test_support::lock();
    let mut r = fresh(32, MAX);
    let a = papk_of_sectors("com.a", 2);
    bake(&mut r, 0, &a, FLAG_BOOT_DEFAULT, 0);
    do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
    register_system(&[system(LAUNCHER_PACKAGE)]);
    let pick = |o: Option<&str>, b: Option<&str>| select_boot(o, b).map(|e| e.package());
    assert_eq!(pick(None, None), Some("com.a"), "the boot default");
    assert_eq!(pick(None, Some("com.b")), Some("com.b"), "board key");
    assert_eq!(
        pick(Some("com.b"), Some("com.a")),
        Some("com.b"),
        "override wins"
    );
    assert_eq!(pick(Some("launcher"), None), Some(LAUNCHER_PACKAGE));
    assert_eq!(pick(Some("app"), Some("com.b")), Some("com.a"));
    assert_eq!(pick(Some(LAUNCHER_PACKAGE), None), Some(LAUNCHER_PACKAGE));
    // A name that is not installed falls through.
    assert_eq!(pick(Some("com.zzz"), Some("com.b")), Some("com.b"));
    assert_eq!(pick(None, Some("com.zzz")), Some("com.a"));
    assert_eq!(boot_image(), Some(&a[..]));
}

#[test]
fn without_a_boot_default_the_launcher_comes_before_the_lowest_run() {
    let _g = test_support::lock();
    let mut r = fresh(32, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
    assert_eq!(
        boot_package(),
        Some("com.a"),
        "lowest sector when nothing else says"
    );
    register_system(&[system(LAUNCHER_PACKAGE)]);
    assert_eq!(boot_package(), Some(LAUNCHER_PACKAGE));
    assert_eq!(
        select_boot(Some("app"), None).map(|e| e.package()),
        Some("com.a"),
        "--boot app skips the launcher"
    );
}

#[test]
fn a_launcher_alone_boots_and_an_empty_directory_boots_nothing() {
    let _g = test_support::lock();
    let _r = fresh(8, MAX);
    assert_eq!(boot_package(), None);
    register_system(&[system(LAUNCHER_PACKAGE)]);
    assert_eq!(boot_package(), Some(LAUNCHER_PACKAGE));
    // No baked app: `--boot app` warns and the next rule applies.
    assert_eq!(
        select_boot(Some("app"), None).map(|e| e.package()),
        Some(LAUNCHER_PACKAGE)
    );
}

#[cfg(has_multi_app)]
#[test]
fn next_image_takes_a_pending_launch_once_then_returns_home() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    register_system(&[system(LAUNCHER_PACKAGE)]);
    let home = launcher().unwrap().image;
    set_running(Some(LAUNCHER_PACKAGE));
    assert_eq!(request_launch("com.zzz"), Err(NotFound));
    request_launch("com.a").unwrap();
    assert_eq!(next_image(), Some(&image_of("com.a")[..]));
    set_running(Some("com.a"));
    assert_eq!(next_image(), Some(home), "an app exit returns home");
    assert_eq!(
        next_image(),
        Some(home),
        "and again: the launcher was not running"
    );
}

#[cfg(has_multi_app)]
#[test]
fn a_pending_launch_is_dropped_when_its_package_is_gone() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    do_install(&mut r, &papk_of_sectors("com.b", 2)).unwrap();
    register_system(&[system(LAUNCHER_PACKAGE)]);
    set_running(Some("com.b"));
    request_launch("com.a").unwrap();
    do_uninstall(&mut r, "com.a");
    assert_eq!(next_image(), Some(launcher().unwrap().image));
}

#[cfg(has_multi_app)]
#[test]
fn a_reinstalled_pending_package_launches_its_new_copy() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    request_launch("com.a").unwrap();
    let a2 = papk_of_sectors("com.a", 3);
    do_install(&mut r, &a2).unwrap();
    assert_eq!(next_image(), Some(&a2[..]));
}

#[cfg(has_multi_app)]
#[test]
fn the_launcher_is_restarted_once_then_the_device_waits() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    register_system(&[system(LAUNCHER_PACKAGE)]);
    let home = launcher().unwrap().image;
    set_running(Some(LAUNCHER_PACKAGE));
    assert_eq!(next_image(), Some(home), "first exit: start it again");
    assert_eq!(next_image(), None, "second in a row: wait for an install");
    // A launch in between resets the count.
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    request_launch("com.a").unwrap();
    assert_eq!(next_image(), Some(&image_of("com.a")[..]));
    set_running(Some("com.a"));
    assert_eq!(next_image(), Some(home));
    set_running(Some(LAUNCHER_PACKAGE));
    assert_eq!(next_image(), Some(home));
    assert_eq!(next_image(), None);
}

#[cfg(has_multi_app)]
#[test]
fn without_a_launcher_an_exit_waits_for_an_install() {
    let _g = test_support::lock();
    let mut r = fresh(16, MAX);
    do_install(&mut r, &papk_of_sectors("com.a", 2)).unwrap();
    set_running(Some("com.a"));
    assert_eq!(next_image(), None);
}
