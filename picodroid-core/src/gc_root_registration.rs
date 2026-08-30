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
pub const EXPECTED_PROVIDERS: usize = 19;

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
    use crate::graphics::lvgl::{animations, events, widgets};

    // The Display singleton is cached by `getInstance`, so after the first
    // call nothing on the Java side references it. Left unrooted it is swept
    // on the first GC and its slot reused — which broke every navigation in
    // picoenvmon with `NoSuchMethod` once a SensorEvent landed in the reused
    // slot.
    register(crate::graphics::display::visit_gc_roots);

    // A View (or AlertDialog) referenced only by a native listener map —
    // key/touch/swipe/click/dialog — and not by any Java field would
    // otherwise be swept, after which dispatch resolves a live lv_obj to a
    // dead ref and input silently dies (the keypad appears to "lose focus"
    // a few seconds in, post-GC).
    register_object_refs(events::visit_view_listener_roots);
    register_object_refs(widgets::button::visit_click_listener_roots);
    register_object_refs(widgets::button::visit_long_click_listener_roots);
    register_object_refs(widgets::list_view::visit_item_click_listener_roots);
    register_object_refs(widgets::alert_dialog::visit_dialog_obj_roots);

    // Compound buttons and EditText: same hazard. One of these kept alive
    // only by its listener map (a local in onCreate, never stored in a
    // field) is swept on the first GC, its slot reused, and the next
    // Activity's onCreate hits a dead ref → NoSuchMethod.
    register_object_refs(widgets::switch::visit_checked_change_listener_roots);
    register_object_refs(widgets::check_box::visit_checked_change_listener_roots);
    register_object_refs(widgets::radio_button::visit_checked_change_listener_roots);
    register_object_refs(widgets::toggle_button::visit_checked_change_listener_roots);
    register_object_refs(widgets::edit_text::visit_editor_action_listener_roots);
    register_object_refs(widgets::edit_text::visit_text_changed_listener_roots);

    // NumberPicker registers every instance, not just listener holders: the
    // obj_ref is the fireStep dispatch target.
    register_object_refs(widgets::number_picker::visit_picker_roots);

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

    // Sensor registrations hold the listener and the recycled SensorEvent;
    // the Activity stack holds each live Activity object and its pending
    // intents; bound services hold their connection objects. None of these
    // is reachable from a Java field once native code is the only holder.
    register(crate::hardware::sensors::visit_gc_roots);
    register(crate::lifecycle::visit_gc_roots);
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
#[path = "../../test_support/gc_root_scan.rs"]
mod gc_root_scan;

#[cfg(test)]
mod tests {
    #[test]
    fn every_root_provider_is_registered() {
        let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        super::gc_root_scan::check(&src_root, super::EXPECTED_PROVIDERS);
    }
}
