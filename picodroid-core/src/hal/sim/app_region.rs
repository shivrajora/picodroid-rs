// SPDX-License-Identifier: GPL-3.0-only
//! The simulator's app region: an in-memory [`MemRegion`] of the board's
//! `PAPK_REGION_LEN`, seeded from the environment, with the control-channel
//! verbs that install into it (docs/designs/multi-app-2026-09.md M1f).
//!
//! Seeding: the app under test (`PICODROID_APK_PATH`) is baked at sector 0
//! exactly as `flash.sh` bakes it on a device — `FLAG_BOOT_DEFAULT`,
//! `seq` 0 — and every path in `PICODROID_SIM_APPS` (colon-separated) is
//! installed through the real installer, so placement, compaction and the
//! directory run the same code they run on hardware.
//!
//! The verbs (`./scripts/sim-ctrl.sh apps list|install <papk>|uninstall
//! <package>`) arrive on the control channel's thread and are serviced on
//! the JVM task at the next tick, which keeps the package directory's
//! single-writer discipline. Replacing the *running* package is refused
//! until the app-switching loop lands (M2).

use std::sync::Mutex;

use papk_format::flash_image::FLAG_BOOT_DEFAULT;

use crate::board_cfg::flash::{MAX_INSTALLED_APPS, PAPK_REGION_LEN};
use crate::install::mem_region::{MemRegion, MemTransport, NoCoordinator};
use crate::install::{install, uninstall, PapkFlash};
use crate::packages::{self, SECTOR};

enum Request {
    List,
    Install(String),
    Uninstall(String),
}

static REQUESTS: Mutex<Vec<Request>> = Mutex::new(Vec::new());

// SAFETY: written at init before the scheduler starts, then only by the JVM
// task (`service_requests`); the directory's single-writer rule.
static mut REGION: Option<MemRegion> = None;

fn region() -> Option<&'static mut MemRegion> {
    unsafe { (*core::ptr::addr_of_mut!(REGION)).as_mut() }
}

/// Create the region and seed it. Pre-scheduler, from the simulator's boot.
pub fn init() {
    // The region models flash: it is not charged to the simulated heap.
    let _flash = crate::host::heap_bypass();
    let mut region = MemRegion::new(PAPK_REGION_LEN / SECTOR, MAX_INSTALLED_APPS);
    packages::rescan_region(&region);

    if let Ok(path) = std::env::var("PICODROID_APK_PATH") {
        match std::fs::read(&path) {
            Ok(bytes) => bake(&mut region, &bytes),
            Err(e) => eprintln!("[sim] apps: cannot read {path}: {e}"),
        }
    }
    if let Ok(list) = std::env::var("PICODROID_SIM_APPS") {
        for path in list.split(':').filter(|p| !p.is_empty()) {
            match std::fs::read(path) {
                Ok(bytes) => {
                    if let Err(e) = install_bytes(&mut region, &bytes) {
                        eprintln!("[sim] apps: install of {path} refused: {e}");
                    }
                }
                Err(e) => eprintln!("[sim] apps: cannot read {path}: {e}"),
            }
        }
    }
    unsafe { REGION = Some(region) };
    print_list();
}

/// What `build.rs` links into the region on a device: the image at sector
/// 0 with both meta pages, the boot-default flag and sequence 0.
fn bake(region: &mut MemRegion, bytes: &[u8]) {
    unsafe {
        region.select_run(0);
        for (i, chunk) in bytes.chunks(256).enumerate() {
            let mut page = [0xFFu8; 256];
            page[..chunk.len()].copy_from_slice(chunk);
            if !region.write_page(i as u32, &page) {
                eprintln!("[sim] apps: the app under test does not fit the region");
                return;
            }
        }
        region.commit_metadata(bytes.len() as u32, FLAG_BOOT_DEFAULT, 0);
    }
    packages::rescan_region(&*region);
}

fn install_bytes(region: &mut MemRegion, bytes: &[u8]) -> Result<(), String> {
    let mut t = MemTransport::for_papk(bytes);
    if install(&mut t, &mut NoCoordinator, region, bytes.len() as u32) {
        Ok(())
    } else {
        Err(format!("{:?}", t.error))
    }
}

/// Queue a verb from the control channel: the text after `apps`. Returns
/// `false` when it is not one of ours.
pub fn request(rest: &str) -> bool {
    let mut it = rest.split_whitespace();
    let req = match (it.next(), it.next()) {
        (Some("list"), None) => Request::List,
        (Some("install"), Some(path)) => Request::Install(path.to_string()),
        (Some("uninstall"), Some(package)) => Request::Uninstall(package.to_string()),
        _ => {
            println!("[sim] apps: usage: apps list | apps install <file.papk> | apps uninstall <package>");
            return false;
        }
    };
    REQUESTS.lock().unwrap_or_else(|p| p.into_inner()).push(req);
    true
}

/// Serve queued verbs. Called on the JVM task once per tick.
pub fn service_requests() {
    let pending: Vec<Request> =
        std::mem::take(&mut *REQUESTS.lock().unwrap_or_else(|p| p.into_inner()));
    if pending.is_empty() {
        return;
    }
    let Some(region) = region() else {
        println!("[sim] apps: no region (init did not run)");
        return;
    };
    for req in pending {
        match req {
            Request::List => print_list(),
            Request::Install(path) => match std::fs::read(&path) {
                Err(e) => println!("[sim] apps: cannot read {path}: {e}"),
                Ok(bytes) => {
                    let package =
                        papk_format::find_manifest_value(&bytes, papk_format::keys::PACKAGE_NAME);
                    if package.is_some() && package == packages::running() {
                        println!(
                            "[sim] apps: {} is the running app; replacing it needs the app-switching loop (M2)",
                            package.unwrap_or("?")
                        );
                        continue;
                    }
                    match install_bytes(region, &bytes) {
                        Ok(()) => println!(
                            "[sim] apps: installed {} ({} bytes)",
                            package.unwrap_or("?"),
                            bytes.len()
                        ),
                        Err(e) => println!("[sim] apps: install refused: {e}"),
                    }
                    print_list();
                }
            },
            Request::Uninstall(package) => {
                if Some(package.as_str()) == packages::running() {
                    println!("[sim] apps: {package} is the running app; uninstalling it needs the app-switching loop (M2)");
                    continue;
                }
                match packages::find(&package) {
                    None => println!("[sim] apps: {package} is not installed"),
                    Some(e) => {
                        let (first, sectors) = (e.first_sector as u32, e.sectors as u32);
                        let mut t = MemTransport::for_papk(&[]);
                        uninstall(&mut t, &mut NoCoordinator, region, first, sectors);
                        println!("[sim] apps: uninstalled {package}");
                        print_list();
                    }
                }
            }
        }
    }
}

fn print_list() {
    let mut rows: Vec<&packages::Entry> = packages::apps().collect();
    rows.sort_by_key(|e| e.first_sector);
    if rows.is_empty() {
        println!("[sim] apps: (none installed)");
    }
    for e in rows {
        println!(
            "[sim] apps: sector {:>3}  {:<24} {:>4}  {:<8} {:>7} B  {}  {}",
            e.first_sector,
            e.package(),
            e.version_code(),
            e.version(),
            e.size(),
            if e.is_boot_default() { "boot" } else { "    " },
            e.label()
        );
    }
    let (largest, total) = packages::free_space();
    println!(
        "[sim] apps: free: largest {} KB, total {} KB, apps {}/{}",
        largest as usize * SECTOR / 1024,
        total as usize * SECTOR / 1024,
        packages::installed_count(),
        MAX_INSTALLED_APPS
    );
}
