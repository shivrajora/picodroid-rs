// SPDX-License-Identifier: GPL-3.0-only
//! Registers this crate's GC root providers with `picodroid_core::gc_roots`.
//!
//! Every object that native code holds but Java does not — a View referenced
//! only by a listener map, a bound Service, the Display singleton — must be
//! reported to the collector or it is swept while still live. Missing one is
//! silent: the object disappears, its slot is reused, and the failure shows
//! up much later as dead input or `NoSuchMethod`.
//!
//! As the extraction moves modules into `picodroid-core`, each provider's
//! line moves from here to that crate's own registration in the *same commit
//! as the file* — so the union of registered providers never changes, even
//! though its two halves do. [`EXPECTED_PROVIDERS`] pins this crate's share
//! across those moves.
//!
//! See `docs/designs/shared-core-extraction.md` §3.G.

/// Total providers this crate registers.
///
/// Asserted by [`tests::every_root_provider_is_registered`]. When a module
/// moves to `picodroid-core`, this drops by exactly the number of providers
/// that went with it, and the core-side constant rises by the same amount. A
/// change here that is not matched there means a provider was dropped.
#[cfg_attr(not(test), allow(dead_code))] // asserted by the guard below
pub const EXPECTED_PROVIDERS: usize = 3;

/// Register every root provider. Call before the first class load, and
/// therefore before any GC can run.
///
/// Idempotent: `run_jvm_with` re-runs on a PDB app reload, and registering
/// twice would visit every root twice — harmless but wasteful. The providers
/// themselves are stateless `fn` pointers, so nothing needs re-registering
/// when the app changes. A plain load/store rather than a compare-and-swap:
/// thumbv6m has no CAS, and boot is single-threaded by construction.
///
/// `cfg(not(test))` because every module named below pulls in LVGL and the
/// HAL. The completeness guard in this file's `tests` module is a source
/// scan, so it still covers this list under `scripts/test.sh`.
#[cfg(not(test))]
pub fn register_all() {
    use core::sync::atomic::{AtomicBool, Ordering};

    use picodroid_core::gc_roots;

    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.load(Ordering::Relaxed) {
        return;
    }
    REGISTERED.store(true, Ordering::Relaxed);

    // Providers owned by picodroid-core register themselves. Every LVGL
    // listener map and the Display singleton now live there; what remains
    // below is this crate's JVM-facing state.
    picodroid_core::gc_root_registration::register_all();

    // Modules that own their own native object references.
    gc_roots::register(crate::system::picodroid::hardware::sensors::visit_gc_roots);
    gc_roots::register(crate::service_lifecycle::visit_gc_roots);
    gc_roots::register(crate::lifecycle::visit_gc_roots);
}

/// Completeness guard — see [`gc_root_scan`] for what it catches and why it
/// scans source rather than calling [`register_all`].
///
/// Shared with `picodroid-core`'s identical guard, so the two cannot drift.
#[cfg(test)]
#[path = "../../../test_support/gc_root_scan.rs"]
mod gc_root_scan;

#[cfg(test)]
mod tests {
    #[test]
    fn every_root_provider_is_registered() {
        let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        super::gc_root_scan::check(&src_root, super::EXPECTED_PROVIDERS);
    }
}
