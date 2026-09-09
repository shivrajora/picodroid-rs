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
//! The system apps a multi-app firmware links in (`PICODROID_SYSTEM_APKS`,
//! the same variable `build.rs` reads for a device) are loaded from disk and
//! registered before the first scan, as `main.rs` does on a device.
//!
//! The verbs (`./scripts/sim-ctrl.sh apps list|install <papk>|uninstall
//! <package>`) arrive on the control channel's thread and are serviced on
//! the JVM task at the next tick, which keeps the package directory's
//! single-writer discipline. A verb that targets the *running* package is
//! held instead: the app is stopped (`STOP_JVM`, as an install park stops
//! it on a device) and the verb runs from the app-switching loop once the
//! app is gone (`service_deferred`), after which a reinstalled package is
//! launched again and an uninstalled one gives way to the launcher.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use papk_format::flash_image::FLAG_BOOT_DEFAULT;

use crate::board_cfg::flash::{MAX_INSTALLED_APPS, PAPK_REGION_LEN};
use crate::install::mem_region::{MemRegion, MemTransport, NoCoordinator};
use crate::install::{install, uninstall, InstallError, PapkFlash};
use crate::packages::{self, Kind, SECTOR};

enum Request {
    List,
    Install(String),
    Uninstall(String),
}

static REQUESTS: Mutex<Vec<Request>> = Mutex::new(Vec::new());
/// Set with every queued verb, so the JVM task's polls cost one load while
/// nothing waits.
static PENDING: AtomicBool = AtomicBool::new(false);

/// A verb held until the running app has stopped.
enum Deferred {
    /// Install `bytes`, then launch `resume` — the package that was running,
    /// or the reinstalled one when it was that package.
    Install {
        package: String,
        bytes: Vec<u8>,
        resume: String,
    },
    Uninstall(String),
}

static DEFERRED: Mutex<Option<Deferred>> = Mutex::new(None);

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

    // System apps first, as on a device, so a run naming one is caught by
    // the scans. Leaked: they model `.rodata`.
    if let Ok(list) = std::env::var("PICODROID_SYSTEM_APKS") {
        let mut images: Vec<&'static [u8]> = Vec::new();
        for path in list.split(':').filter(|p| !p.is_empty()) {
            match std::fs::read(path) {
                Ok(bytes) => images.push(Box::leak(bytes.into_boxed_slice())),
                Err(e) => eprintln!("[sim] apps: cannot read system app {path}: {e}"),
            }
        }
        packages::register_system(&images);
    }
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

/// Erase the run at `first_sector` and rescan: the simulator's half of a
/// Java `PackageInstaller.uninstall` (`PlatformHooks::uninstall_run`), on
/// the JVM task, the directory's single writer.
pub fn uninstall_run(first_sector: u32, sectors: u32) -> bool {
    let Some(region) = region() else {
        return false;
    };
    // SAFETY: the region is a buffer; `erase_run` asserts the sectors lie
    // inside it, and nothing else writes the region while the JVM task
    // runs a native.
    unsafe { region.erase_run(first_sector, sectors) };
    packages::rescan_region(region);
    true
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
    // The transport's copy of the image models the host's USB stream, not
    // the device's heap: a device never holds a whole PAPK in RAM, so
    // neither may the simulated arena be charged for one.
    let _host = crate::host::heap_bypass();
    let mut t = MemTransport::for_papk(bytes);
    if install(&mut t, &mut NoCoordinator, region, bytes.len() as u32) {
        Ok(())
    } else {
        Err(describe(t.error))
    }
}

/// The refusal in the words `pdb install` uses, not the error's `Debug` form.
fn describe(error: Option<InstallError>) -> String {
    match error {
        Some(InstallError::TooLarge) => "too large for the app region".to_string(),
        Some(InstallError::NoRoom {
            need,
            largest_free,
            total_free,
            installed,
            max,
        }) => format!(
            "no room: needs {} KB, largest free {} KB, total free {} KB, apps {installed}/{max}",
            need as usize * SECTOR / 1024,
            largest_free as usize * SECTOR / 1024,
            total_free as usize * SECTOR / 1024
        ),
        Some(InstallError::NoPackageName) => "the manifest has no package-name".to_string(),
        Some(InstallError::SystemPackage) => "names a system app".to_string(),
        Some(InstallError::Incompat) => "built for another framework-map-version".to_string(),
        Some(other) => format!("{other:?}"),
        None => "unknown".to_string(),
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
    PENDING.store(true, Ordering::Release);
    true
}

/// Serve queued verbs. Called on the JVM task — once per tick while an
/// Activity runs, and from the stop poll every app reaches (`SystemClock.sleep`,
/// interpreter yields) for the apps that have no tick. An install, and an
/// uninstall of the running package, are held (see [`service_deferred`])
/// and the app is stopped first.
pub fn service_requests() {
    if !PENDING.load(Ordering::Acquire) {
        return;
    }
    let pending = take_requests();
    if pending.is_empty() {
        return;
    }
    let Some(region) = region() else {
        println!("[sim] apps: no region (init did not run)");
        return;
    };
    serve(region, pending, packages::running());
}

/// Run the verb that waited for the running app to stop, then whatever
/// queued up while nothing ran. Called from the app-switching loop between
/// apps, on the JVM task.
pub fn service_deferred() {
    let Some(region) = region() else { return };
    let deferred = DEFERRED.lock().unwrap_or_else(|p| p.into_inner()).take();
    match deferred {
        None => {}
        Some(Deferred::Install {
            package,
            bytes,
            resume,
        }) => {
            match install_bytes(region, &bytes) {
                Ok(()) => println!("[sim] apps: installed {package} ({} bytes)", bytes.len()),
                Err(e) => println!("[sim] apps: install refused: {e}"),
            }
            // A device reboots after an install and boots by its policy;
            // here what was running comes back — the new copy, when it was
            // the reinstalled package — and a launcher that comes back
            // lists the new app.
            launch_again(&resume);
            print_list();
        }
        Some(Deferred::Uninstall(package)) => {
            do_uninstall(region, &package);
        }
    }
    serve(region, take_requests(), None);
}

/// Read a PAPK from the host without charging the simulated heap: the file
/// is the host's, and on a device the image streams in 256-byte pages.
fn read_papk(path: &str) -> std::io::Result<Vec<u8>> {
    let _host = crate::host::heap_bypass();
    std::fs::read(path)
}

fn take_requests() -> Vec<Request> {
    let mut queue = REQUESTS.lock().unwrap_or_else(|p| p.into_inner());
    PENDING.store(false, Ordering::Release);
    std::mem::take(&mut *queue)
}

/// Run `package` next, in place of the launcher the switching loop would
/// otherwise start.
#[cfg(has_multi_app)]
fn launch_again(package: &str) {
    let _ = packages::request_launch(package);
}

#[cfg(not(has_multi_app))]
fn launch_again(_package: &str) {}

fn defer(d: Deferred) {
    *DEFERRED.lock().unwrap_or_else(|p| p.into_inner()) = Some(d);
    crate::hal::sim::platform::set_stop_jvm(true);
}

fn serve(region: &mut MemRegion, pending: Vec<Request>, running: Option<&str>) {
    for req in pending {
        match req {
            Request::List => print_list(),
            Request::Install(path) => match read_papk(&path) {
                Err(e) => println!("[sim] apps: cannot read {path}: {e}"),
                Ok(bytes) => {
                    let package =
                        papk_format::find_manifest_value(&bytes, papk_format::keys::PACKAGE_NAME)
                            .unwrap_or("?")
                            .to_string();
                    // Never install under a running app: placement may
                    // compact the region, which moves runs — the running
                    // app's image among them — while the interpreter holds
                    // slices into it. A device parks the JVM and reboots
                    // for every install; the simulator stops the app and
                    // brings it (or the reinstalled copy) back afterwards.
                    if let Some(running) = running {
                        if package == running {
                            println!("[sim] apps: {package} is running; stopping it to reinstall");
                        } else {
                            println!("[sim] apps: stopping {running} to install {package}");
                        }
                        let resume = if package == running {
                            package.clone()
                        } else {
                            running.to_string()
                        };
                        defer(Deferred::Install {
                            package,
                            bytes,
                            resume,
                        });
                        continue;
                    }
                    match install_bytes(region, &bytes) {
                        Ok(()) => {
                            println!("[sim] apps: installed {package} ({} bytes)", bytes.len())
                        }
                        Err(e) => println!("[sim] apps: install refused: {e}"),
                    }
                    print_list();
                }
            },
            Request::Uninstall(package) => {
                if Some(package.as_str()) == running {
                    println!("[sim] apps: {package} is running; stopping it to uninstall");
                    defer(Deferred::Uninstall(package));
                    continue;
                }
                do_uninstall(region, &package);
            }
        }
    }
}

fn do_uninstall(region: &mut MemRegion, package: &str) {
    match packages::find(package) {
        None => println!("[sim] apps: {package} is not installed"),
        Some(e) if e.kind == Kind::System => {
            println!("[sim] apps: {package} is a system app; it cannot be uninstalled")
        }
        Some(e) => {
            let (first, sectors) = (e.first_sector as u32, e.sectors as u32);
            let mut t = MemTransport::for_papk(&[]);
            uninstall(&mut t, &mut NoCoordinator, region, first, sectors);
            // The package's data goes with it (D10).
            crate::storage::wipe_package(package);
            println!("[sim] apps: uninstalled {package}");
            print_list();
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
    for e in packages::entries().filter(|e| e.kind == Kind::System) {
        println!(
            "[sim] apps: system      {:<24} {:>4}  {:<8} {:>7} B        {}",
            e.package(),
            e.version_code(),
            e.version(),
            e.size(),
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
    if let Some(running) = packages::running() {
        println!("[sim] apps: running: {running}");
    }
}
