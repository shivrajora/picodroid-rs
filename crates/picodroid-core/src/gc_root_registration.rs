// SPDX-License-Identifier: GPL-3.0-only
//! Registers this crate's GC root providers.
//!
//! The counterpart of the platform crate's list. Together the two cover
//! every native holder of Java objects; as the extraction moves modules
//! here, each provider's line moves from that list to this one in the same
//! commit as its file, so the union never changes size.
//!
//! Why a registry at all — and why missing an entry fails silently rather
//! than loudly — is explained in [`crate::gc_roots`].

/// Providers registered by this crate.
///
/// Rises by exactly the amount the platform crate's `EXPECTED_PROVIDERS`
/// falls whenever modules move. If only one of the two changes in a commit,
/// a provider was dropped.
pub const EXPECTED_PROVIDERS: usize = 11;

/// Register every root provider owned by this crate.
///
/// Called from the platform's own registration, before the first class load
/// and therefore before any GC can run.
///
/// `cfg(not(test))` to match the modules it names — see
/// [`crate::graphics::lvgl`] for why the LVGL-calling ones are gated.
#[cfg(not(test))]
pub fn register_all() {
    use core::sync::atomic::{AtomicBool, Ordering};

    use crate::gc_roots::{register, register_object_refs};

    // Idempotent: `run_app` re-runs on a PDB app reload, and registering
    // twice would both double-visit every root and exhaust
    // `gc_roots::MAX_PROVIDERS` — which asserts rather than silently
    // dropping. The providers are stateless `fn` pointers, so nothing needs
    // re-registering when the app changes.
    //
    // A plain load/store rather than a compare-and-swap: thumbv6m has no
    // CAS, and boot is single-threaded by construction.
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.load(Ordering::Relaxed) {
        return;
    }
    REGISTERED.store(true, Ordering::Relaxed);
    use crate::graphics::lvgl::{animations, widgets};

    // The Display singleton is cached by `getInstance`, so after the first
    // call nothing on the Java side references it. Left unrooted it is swept
    // on the first GC and its slot reused — which broke every navigation in
    // picoenvmon with `NoSuchMethod` once a SensorEvent landed in the reused
    // slot.
    register(crate::graphics::display::visit_gc_roots);

    // A View is NOT rooted by having a listener: the listener is a field of
    // the view and the view lives while Java reaches it, through its
    // parent's child list or a field, as on Android. The handle table's
    // binding of a widget to its view is dropped by `prune_dead_java` when
    // a collection frees the view (docs/designs/view-listeners-2026-10.md).
    // What remain rooted here are objects nothing in Java holds while the
    // framework still owes them a callback.
    register_object_refs(widgets::alert_dialog::visit_dialog_obj_roots);
    register_object_refs(widgets::snackbar::visit_snackbar_dismiss_roots);

    // Animation end-actions hold the Runnable to fire on completion;
    // nothing on the Java side references it for the animation's duration.
    register_object_refs(animations::visit_end_action_roots);

    // Runnables in flight in the executor queues: between
    // Executors.execute() and the drain, the queued word can be a lambda's
    // only reference — a GC in that window swept it and the dispatch ran on
    // a reused slot (bugbash F2; the same hazard class as the listener
    // maps, one queue further out).
    register_object_refs(crate::executors::main_queue::visit_pending_runnable_roots);
    register_object_refs(crate::executors::background_pool::visit_pending_runnable_roots);
    // A scheduled task's Runnable, armed in the deadline table for seconds
    // or minutes with its future typically dropped by the caller.
    register_object_refs(crate::executors::scheduled::visit_pending_runnable_roots);
    // Every live Thread object — reserved in the registry before its task
    // exists, so it (and through its `target` field the Runnable) is rooted
    // from before the child's first frame; idiomatic Java drops every other
    // reference at start() (bugbash B1). Adopted tasks' Thread objects too.
    register_object_refs(crate::threads::visit_thread_roots);

    // Sensor registrations hold the listener and the recycled SensorEvent;
    // the Activity stack holds each live Activity object and its pending
    // intents; bound services hold their connection objects. None of these
    // is reachable from a Java field once native code is the only holder.
    register(crate::hardware::sensors::visit_gc_roots);
    register(crate::lifecycle::input::visit_gc_roots);
    register(crate::service_lifecycle::visit_gc_roots);
}

/// Completeness guard — see [`gc_root_scan`] for what it catches and why it
/// scans source rather than calling [`register_all`].
///
/// Shared with the platform crate's identical guard, so the two cannot
/// drift. This crate holds the majority of the providers, so leaving it with
/// only an unasserted constant would put most of the silent-sweep surface
/// outside any check.
#[cfg(test)]
use test_support::gc_root_scan;

#[cfg(test)]
mod tests {
    #[test]
    fn every_root_provider_is_registered() {
        let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        super::gc_root_scan::check(&src_root, super::EXPECTED_PROVIDERS);
    }
}
