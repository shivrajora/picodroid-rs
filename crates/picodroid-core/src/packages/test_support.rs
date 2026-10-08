// SPDX-License-Identifier: GPL-3.0-only
use papk_format::{AssetSpec, EntryPoint, ManifestSpec, PapkBuilder};

/// The directory is a process-wide static; tests that touch it take this.
pub static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

pub const FW: &str = crate::framework_map::FRAMEWORK_MAP_VERSION;

/// The one class every test PAPK carries: a real, linkable `t/Main`.
pub fn t_main() -> alloc::vec::Vec<u8> {
    class_link::build::minimal_class(b"t/Main")
}

/// A real PAPK for `package`, padded with `extra` bytes of filler (an
/// asset record, as `papk-pack --pad-asset` pads: a class must link, so it
/// cannot be the filler).
pub fn papk(package: &str, extra: usize) -> alloc::vec::Vec<u8> {
    let filler: alloc::vec::Vec<u8> = (0..extra).map(|i| (i * 7 % 251) as u8).collect();
    let main = t_main();
    let mut b = PapkBuilder::new(ManifestSpec {
        entry: EntryPoint::MainClass("t/Main"),
        package_name: package,
        version: "1.0",
        framework_map_version: FW,
        version_code: Some(1),
        label: None,
        icon: None,
        design_size: None,
        requires_features: None,
    });
    b.class("t/Main", &main);
    if extra > 0 {
        b.asset(AssetSpec {
            name: "pad",
            width: 1,
            height: 1,
            cf: 0,
            stride: 0,
            data: &filler,
        });
    }
    b.build().unwrap()
}

/// A system app's image, leaked as `.rodata` would be.
pub fn system(package: &str) -> &'static [u8] {
    alloc::boxed::Box::leak(papk(package, 50).into_boxed_slice())
}
