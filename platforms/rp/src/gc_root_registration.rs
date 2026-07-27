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
/// Checked twice: against the source by
/// [`tests::every_root_provider_is_registered`], and against what actually
/// registered by the assertion in [`register_all`]. When a module moves to
/// `picodroid-core`, this drops by exactly the number of providers that went
/// with it, and the core-side constant rises by the same amount. A change
/// here that is not matched there means a provider was dropped.
pub const EXPECTED_PROVIDERS: usize = 0;

/// Register this family's root providers. Reached from `boot::run_app` via
/// [`picodroid_core::host::PlatformHooks::register_gc_roots`], before the
/// first class load and therefore before any GC can run.
///
/// Idempotent: `run_app` re-runs on a PDB app reload. Registering twice
/// would double-visit every root and, past `gc_roots::MAX_PROVIDERS`, hit
/// the registry-full assertion. The providers are stateless `fn` pointers,
/// so nothing needs re-registering when the app changes. A plain load/store
/// rather than a compare-and-swap: thumbv6m has no CAS, and boot is
/// single-threaded by construction.
///
/// No longer `cfg(not(test))`: with every provider moved to picodroid-core,
/// the body names no LVGL or HAL module, and glue.rs calls it from an
/// ungated hook impl.
pub fn register_all() {
    use core::sync::atomic::{AtomicBool, Ordering};

    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.load(Ordering::Relaxed) {
        return;
    }
    REGISTERED.store(true, Ordering::Relaxed);

    // Empty rather than deleted: a family that adds a native module holding
    // Java object references registers it here, and the guard below keeps
    // that honest. picodroid-core registers its own before this runs.

    // Closes the blind spot the source scanner cannot see (A2 of the
    // shared-core extraction design, deferred there and landed here): the
    // scanner reads text, so a `register` call compiled out by a `cfg` still
    // reads as present. This counts what actually registered. Both halves of
    // the union are checked at once because core registers first — that
    // ordering is `run_app`'s, not an accident.
    //
    // A real `assert!`, not `debug_assert!`: device builds turn
    // debug-assertions off to buy flash headroom (`scripts/lib.sh`), which is
    // exactly the configuration this needs to hold in. A static message
    // rather than `assert_eq!` so the cost is the string, not two operands'
    // worth of formatting machinery.
    assert!(
        picodroid_core::gc_roots::provider_count()
            == picodroid_core::gc_root_registration::EXPECTED_PROVIDERS + EXPECTED_PROVIDERS,
        "GC root provider count mismatch: a cfg-gated registration was \
         compiled out, so live objects will be swept"
    );
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
