// SPDX-License-Identifier: GPL-3.0-only
//! The Activity back stack: what a pending push, pop or recreate does to the
//! stack and to the instances on it, and reclaiming Activities that are
//! covered when memory runs short (T3.1-D).

use super::*;

// ── Lifecycle invocation + transition processing ─────────────────────────────

/// Result of a lifecycle invocation, used by [`run_activity`] to decide
/// whether to continue the loop or unwind.
#[cfg(not(test))]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum LifecycleControl {
    /// Method ran (or was the framework default); continue the loop.
    Continue,
    /// JVM cooperative interrupt — caller should return immediately.
    Break,
}

#[cfg(not(test))]
impl LifecycleControl {
    pub(crate) fn is_break(self) -> bool {
        matches!(self, LifecycleControl::Break)
    }
}

/// Park the current top Activity's content view: hide it and snapshot the
/// handle into its stack entry, then clear CURRENT_ROOT_ID so the next
/// `setContentView` lands on a clean slate.
#[cfg(not(test))]
pub(super) fn park_top_view(handler: &mut crate::native_handler::PicodroidNativeHandler) {
    use crate::graphics::display;
    use crate::graphics::gfx::{Handle, Visibility};
    use crate::graphics::lvgl::with_gfx;

    let prev_root = display::current_root_id();
    if prev_root != 0 {
        with_gfx(|g| g.set_visibility(Handle::from_java(prev_root), Visibility::Gone));
        handler.set_current_root_handle(prev_root);
        display::set_current_root_id(0);
    }
    // Dismiss any dialog the now-covered Activity left on screen. Its modal
    // scrim is parented to the screen (not the root hidden above), so it would
    // otherwise float over the incoming child Activity AND, as the topmost
    // "shown" dialog, steal the child's BACK. At push time every shown dialog
    // belongs to the Activity being covered, so dismissing all is correct —
    // the mirror of handle_pop_op's finish cleanup. See
    // project_picoenvmon_alertdialog_leak.
    while crate::graphics::widgets::dismiss_topmost_dialog() {}
}

/// Inverse of [`park_top_view`]: restore the top Activity's saved view
/// handle back into CURRENT_ROOT_ID and make it visible again. Used both
/// after a Pop uncovers the parent and as the rollback path when a Push
/// hits the stack-overflow cap.
#[cfg(not(test))]
pub(super) fn restore_top_view(handler: &mut crate::native_handler::PicodroidNativeHandler) {
    use crate::graphics::display;
    use crate::graphics::gfx::{Handle, Visibility};
    use crate::graphics::lvgl::with_gfx;

    let saved = handler.current_root_handle();
    if saved != 0 {
        with_gfx(|g| g.set_visibility(Handle::from_java(saved), Visibility::Visible));
        display::set_current_root_id(saved);
        handler.set_current_root_handle(0);
    }
}

/// Handle `PendingActivityOp::Push` — pause + park the current top (if
/// any), push the new Activity, drive onCreate→onStart→onResume on it, then
/// trailing onStop on the previous top per Android ordering.
#[cfg(not(test))]
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_push_op(
    jvm: &mut Jvm,
    new_class: &'static str,
    new_intent: Option<u16>,
    request_code: Option<i32>,
    caller: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    // Make room first: the new screen is about to be built out of whatever
    // the covered ones can give back.
    if reclaim_covered_activities(jvm, heap, handler).is_break() {
        return LifecycleControl::Break;
    }
    // Framework owns instantiation: allocate the Activity and run its
    // no-arg constructor before the lifecycle callbacks.
    let new_ref = match instantiate_component(jvm, new_class, heap, handler) {
        Some(r) => r,
        None => {
            log_error!("failed to instantiate Activity {}", new_class);
            return LifecycleControl::Continue;
        }
    };
    // Capture the previous top before pushing — needed for the trailing
    // onStop call after the new top is fully resumed.
    let prev = handler.current_activity();
    if let Some((prev_ref, _)) = prev {
        if invoke_lifecycle(
            jvm,
            dispatch_sites::ACTIVITY_ON_PAUSE,
            prev_ref,
            heap,
            handler,
        )
        .is_break()
        {
            return LifecycleControl::Break;
        }
        park_top_view(handler);
    }
    if !handler.push_activity(new_ref, new_class, new_intent, request_code, caller) {
        log_error!("activity stack overflow on push: {}", new_class);
        // Rollback: unpark prev's view so it isn't left hidden forever.
        if prev.is_some() {
            restore_top_view(handler);
        }
        return LifecycleControl::Continue;
    }
    crate::pd_info!("activity: push {}", new_class);
    // New top gets its own keypad focus group before onCreate, isolating its
    // focus from the parent's (which is parked with its focus intact).
    crate::graphics::lvgl::events::push_activity_group();
    if start_activity_instance(jvm, new_ref, None, None, heap, handler).is_break() {
        return LifecycleControl::Break;
    }
    // New top is now fully resumed — stop the previous one. Order matches
    // Android: `prev.onStop` lands AFTER `new.onResume`.
    if let Some((prev_ref, _)) = prev {
        if invoke_lifecycle(
            jvm,
            dispatch_sites::ACTIVITY_ON_STOP,
            prev_ref,
            heap,
            handler,
        )
        .is_break()
        {
            return LifecycleControl::Break;
        }
    }
    // The previous top is now stopped, which makes it a reclaim candidate:
    // at once under "don't keep activities", and otherwise if building the
    // new screen left memory short.
    if reclaim_covered_activities(jvm, heap, handler).is_break() {
        return LifecycleControl::Break;
    }
    // Native heap use legitimately steps with the new screen's construction;
    // re-baseline the native growth sentinel so it watches for steady-state
    // drift from here, not from the first Activity's arm point.
    #[cfg(feature = "mem-diag")]
    crate::mem_diag::note_activity_transition();
    LifecycleControl::Continue
}

/// Handle `PendingActivityOp::Pop` — drive onPause→onStop→onDestroy on the
/// top, free its content view, auto-unbind any Service connections it
/// owned, then uncover the parent and restore its parked view.
#[cfg(not(test))]
pub(super) fn handle_pop_op(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    let (top_ref, top_class) = match handler.current_activity() {
        Some(t) => t,
        None => return LifecycleControl::Continue, // already empty
    };
    crate::pd_info!("activity: pop {}", top_class);
    // Snapshot the finishing Activity's result BEFORE the pop (it lives on the
    // entry being removed). Delivered to the uncovered caller below.
    let pending_result = handler.top_activity_result();
    if destroy_top_instance(jvm, top_ref, false, heap, handler).is_break() {
        return LifecycleControl::Break;
    }
    pop_and_resume_parent(jvm, pending_result, heap, handler)
}

/// Take the top Activity instance from resumed to destroyed and free what it
/// owned — content view, dialogs, Service bindings — leaving its stack entry
/// in place for the caller to pop (`finish`) or hand to a new instance
/// (`recreate`). `save_state` asks for `onSaveInstanceState` between `onStop`
/// and `onDestroy` (where Android P and later put it) into a fresh Bundle
/// rooted on the entry; a finishing Activity is never asked, as on Android.
#[cfg(not(test))]
pub(super) fn destroy_top_instance(
    jvm: &mut Jvm,
    top_ref: u16,
    save_state: bool,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    use crate::graphics::display;
    use crate::graphics::gfx::Handle;
    use crate::graphics::lvgl::with_gfx;

    for site in [
        dispatch_sites::ACTIVITY_ON_PAUSE,
        dispatch_sites::ACTIVITY_ON_STOP,
    ] {
        if invoke_lifecycle(jvm, site, top_ref, heap, handler).is_break() {
            return LifecycleControl::Break;
        }
    }
    if save_state {
        // `Bundle()` has an empty body, so the defaults are the constructed
        // object. An allocation failure degrades to a re-creation with no
        // saved state, which the app sees as a fresh launch.
        let bundle = heap
            .objects
            .alloc_with_defaults(crate::shrink_names::c::picodroid_os_Bundle, jvm.classes());
        if bundle.is_none() {
            crate::pd_warn!("recreate: no memory for the saved-state Bundle");
        }
        handler.set_top_saved_state(bundle);
        if let Some(b) = bundle {
            if invoke_trampoline(
                jvm,
                dispatch_sites::ACTIVITY_SAVE_INSTANCE_STATE,
                top_ref,
                pico_jvm::types::Value::ObjectRef(b),
                heap,
                handler,
            )
            .is_break()
            {
                return LifecycleControl::Break;
            }
        }
    }
    if invoke_lifecycle(
        jvm,
        dispatch_sites::ACTIVITY_ON_DESTROY,
        top_ref,
        heap,
        handler,
    )
    .is_break()
    {
        return LifecycleControl::Break;
    }
    // Free the destroyed activity's content view BEFORE popping the entry,
    // so the popped entry's saved root_handle (which tracks CURRENT_ROOT_ID
    // for the topmost) is consumed cleanly.
    let top_root = display::current_root_id();
    if top_root != 0 {
        with_gfx(|g| g.delete(Handle::from_java(top_root)));
        display::set_current_root_id(0);
    }
    // Tear down any AlertDialog the finishing Activity left on screen. The
    // dialog's modal scrim is parented to the screen, not the content view
    // deleted above, so it would otherwise outlive the Activity and leak onto
    // the one beneath as an input-absorbing modal — Android dismisses an
    // Activity's dialogs on destroy ("leaked window" prevention).
    while crate::graphics::widgets::dismiss_topmost_dialog() {}
    // Auto-unbind any Service connections this Activity owned — mirrors
    // Android's behaviour for an Activity destroyed while holding bindings.
    // Runs after the Activity's own onDestroy so the Activity can still call
    // unbindService itself.
    crate::service_lifecycle::unbind_owned_by(top_ref, jvm, heap, handler);
    LifecycleControl::Continue
}

/// Pop the (already destroyed) top entry and bring the Activity beneath it
/// back to the foreground, delivering `pending_result` if that Activity is
/// the one that asked for it.
#[cfg(not(test))]
pub(super) fn pop_and_resume_parent(
    jvm: &mut Jvm,
    pending_result: Option<(i32, u16, i32, Option<u16>)>,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    handler.pop_activity();
    // Tear down the popped Activity's keypad focus group and reactivate the
    // parent's (with its focus intact) — done after its view tree was deleted
    // above so the group is empty. No-op on boards without buttons.
    crate::graphics::lvgl::events::pop_activity_group();
    // Restore the resumed parent's parked view, if any. Apps that build UI
    // in onCreate get their tree back without rebuilding; apps that rebuild
    // in onResume will replace it (the saved root will be deleted by
    // set_content_view's prev-delete branch).
    // The result goes to the uncovered entry only if that entry is the one
    // that asked — matched by stack-entry token, which a reclaimed caller
    // keeps — so a deeper finish (A→B→C, C finishes to B) never misdelivers
    // A's result to C.
    let caller_token = handler.top_activity_token();
    let result: Option<ActivityResult> = pending_result
        .filter(|&(_, caller, _, _)| caller == caller_token)
        .map(|(request_code, _, result_code, intent)| (request_code, result_code, intent));
    if handler.top_activity_destroyed() {
        return recreate_uncovered(jvm, result, heap, handler);
    }
    if let Some((new_top_ref, _)) = handler.current_activity() {
        restore_top_view(handler);
        // Result delivery (AOSP order): deliverResults precedes
        // performResume→performRestart, so onActivityResult lands AFTER
        // restore_top_view but BEFORE onRestart.
        if let Some(result) = result {
            if deliver_result(jvm, new_top_ref, result, heap, handler).is_break() {
                return LifecycleControl::Break;
            }
        }
        // Android's stopped->foreground edge: onRestart precedes onStart when
        // returning after a child Activity finished above this one.
        for site in [
            dispatch_sites::ACTIVITY_ON_RESTART,
            dispatch_sites::ACTIVITY_ON_START,
            dispatch_sites::ACTIVITY_ON_RESUME,
        ] {
            if invoke_lifecycle(jvm, site, new_top_ref, heap, handler).is_break() {
                return LifecycleControl::Break;
            }
        }
    }
    // Mirror of the push-side re-baseline: the pop released a screen's worth
    // of native allocations, another legitimate step the sentinel must not
    // judge against a stale baseline.
    #[cfg(feature = "mem-diag")]
    crate::mem_diag::note_activity_transition();
    LifecycleControl::Continue
}

/// The top entry was reclaimed while it was covered and has just been
/// uncovered: start a new instance in it from the Bundle its old one saved,
/// `onCreate(saved)` → `onStart` → `onRestoreInstanceState` → (`result`) →
/// `onResume`. No `onRestart`: this instance was never stopped.
#[cfg(not(test))]
pub(super) fn recreate_uncovered(
    jvm: &mut Jvm,
    result: Option<ActivityResult>,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    let Some((_, class)) = handler.current_activity() else {
        return LifecycleControl::Continue;
    };
    crate::pd_info!("activity: re-create {} (was reclaimed)", class);
    // The entry that held the result Intent is gone, and the constructor and
    // three callbacks run before it is handed over.
    handler.set_delivery_intent(result.and_then(|r| r.2));
    let Some(new_ref) = instantiate_component(jvm, class, heap, handler) else {
        // Nothing to put in the entry: finish it too, and carry on down.
        log_error!(
            "failed to re-create reclaimed Activity {}; finishing it",
            class
        );
        handler.set_delivery_intent(None);
        handler.set_top_saved_state(None);
        let pending_result = handler.top_activity_result();
        return pop_and_resume_parent(jvm, pending_result, heap, handler);
    };
    let saved = handler.top_saved_state();
    handler.replace_top_activity(new_ref);
    // A fresh keypad focus group for the new view tree, as in recreate(): the
    // entry's old one emptied when its tree was deleted.
    crate::graphics::lvgl::events::pop_activity_group();
    crate::graphics::lvgl::events::push_activity_group();
    let control = start_activity_instance(jvm, new_ref, saved, result, heap, handler);
    handler.set_top_saved_state(None);
    handler.set_delivery_intent(None);
    #[cfg(feature = "mem-diag")]
    crate::mem_diag::note_activity_transition();
    control
}

// ── Reclaiming covered Activities (T3.1-D) ───────────────────────────────────

/// Android's developer option "Don't keep activities": destroy every
/// Activity as soon as it is covered, so the save/re-create path runs on
/// every navigation instead of only when memory is short.
pub(super) static DONT_KEEP_ACTIVITIES: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Turn "don't keep activities" on or off. Takes effect at the next push.
pub fn set_dont_keep_activities(on: bool) {
    DONT_KEEP_ACTIVITIES.store(on, core::sync::atomic::Ordering::Relaxed);
}

pub fn dont_keep_activities() -> bool {
    DONT_KEEP_ACTIVITIES.load(core::sync::atomic::Ordering::Relaxed)
}

/// Seed the switch from `PICODROID_DONT_KEEP_ACTIVITIES` (anything but empty
/// or `0` is on): read when the simulator starts, and baked in at build time
/// on a device. Leaves a value set some other way alone when the variable is
/// absent.
#[cfg(not(test))]
pub(super) fn init_dont_keep_activities() {
    #[cfg(feature = "sim")]
    let var = std::env::var("PICODROID_DONT_KEEP_ACTIVITIES").ok();
    #[cfg(not(feature = "sim"))]
    let var = option_env!("PICODROID_DONT_KEEP_ACTIVITIES");
    if let Some(v) = var {
        let on = !matches!(v.trim(), "" | "0");
        set_dont_keep_activities(on);
        if on {
            crate::pd_info!("activity: don't keep activities is ON");
        }
    }
}

/// Reclaim below this much of the LVGL pool free (1/N of its size). A
/// covered Activity's view tree is parked in that pool, hidden but whole.
pub(super) const POOL_PRESSURE_DIVISOR: usize = 8;
/// Reclaim below this much of the native heap free (1/N of its size) — the
/// heap the JVM's object storage grows in.
pub(super) const HEAP_PRESSURE_DIVISOR: usize = 16;

/// Is either arena short enough that a covered Activity should give its
/// memory back? Deliberately late: a re-created Activity loses whatever its
/// `onSaveInstanceState` did not save, and most apps save nothing, so this
/// is for the push that would otherwise fail, not for keeping memory tidy.
#[cfg(not(test))]
pub(super) fn under_memory_pressure() -> bool {
    let (pool_free, pool_total) = crate::graphics::lvgl::pool_free_bytes();
    if pool_free < pool_total / POOL_PRESSURE_DIVISOR {
        return true;
    }
    let native = crate::host::native_heap_stats();
    let native_total = native.used_bytes + native.free_bytes;
    native.free_bytes < native_total / HEAP_PRESSURE_DIVISOR
}

/// Destroy covered Activities, oldest first, while "don't keep activities"
/// is on or memory is short. Each gets `onSaveInstanceState` → `onDestroy`
/// (it is already paused and stopped), its parked view tree is freed, and
/// its stack entry stays — token, launch Intent, for-result metadata and the
/// Bundle — for [`recreate_uncovered`]. The foreground Activity is never
/// touched.
#[cfg(not(test))]
pub(super) fn reclaim_covered_activities(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    for index in 0..handler.activity_depth().saturating_sub(1) {
        if handler.covered_activity(index).is_none() {
            continue; // already reclaimed
        }
        if !dont_keep_activities() && !under_memory_pressure() {
            break;
        }
        if reclaim_covered(jvm, index, heap, handler).is_break() {
            return LifecycleControl::Break;
        }
        // The instance and its widgets' Java side are garbage only once
        // collected, and the pressure test above reads the heap.
        heap.collect_now(handler);
    }
    LifecycleControl::Continue
}

/// Destroy the covered Activity at stack `index` with its state saved.
#[cfg(not(test))]
pub(super) fn reclaim_covered(
    jvm: &mut Jvm,
    index: usize,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    use crate::graphics::gfx::Handle;
    use crate::graphics::lvgl::with_gfx;

    let Some((obj_ref, class, root)) = handler.covered_activity(index) else {
        return LifecycleControl::Continue;
    };
    crate::pd_info!("activity: reclaim {}", class);
    // As in `destroy_top_instance`: no Bundle degrades to a re-creation with
    // no saved state, which the app sees as a fresh launch.
    let bundle = heap
        .objects
        .alloc_with_defaults(crate::shrink_names::c::picodroid_os_Bundle, jvm.classes());
    if bundle.is_none() {
        crate::pd_warn!("reclaim: no memory for the saved-state Bundle");
    }
    // Rooted on the entry before any Java runs.
    handler.set_saved_state_at(index, bundle);
    if let Some(b) = bundle {
        if invoke_trampoline(
            jvm,
            dispatch_sites::ACTIVITY_SAVE_INSTANCE_STATE,
            obj_ref,
            pico_jvm::types::Value::ObjectRef(b),
            heap,
            handler,
        )
        .is_break()
        {
            return LifecycleControl::Break;
        }
    }
    if invoke_lifecycle(
        jvm,
        dispatch_sites::ACTIVITY_ON_DESTROY,
        obj_ref,
        heap,
        handler,
    )
    .is_break()
    {
        return LifecycleControl::Break;
    }
    // The parked view tree. Its dialogs went when it was covered
    // (`park_top_view`); its keypad focus group empties with the tree and
    // stays in the group stack until the entry is uncovered or popped.
    if root != 0 {
        with_gfx(|g| g.delete(Handle::from_java(root)));
    }
    crate::service_lifecycle::unbind_owned_by(obj_ref, jvm, heap, handler);
    handler.mark_activity_destroyed(index);
    LifecycleControl::Continue
}

/// Handle `PendingActivityOp::Recreate` — `Activity.recreate()`: destroy the
/// top instance with its state saved, then start a new instance of the same
/// class in the same stack entry (launch Intent and for-result metadata
/// carry over) with that state. The Bundle lives on the entry from
/// `onSaveInstanceState` to the new instance's `onResume` and nowhere after.
#[cfg(not(test))]
pub(super) fn handle_recreate_op(
    jvm: &mut Jvm,
    target: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    let (top_ref, top_class) = match handler.current_activity() {
        Some(t) if t.0 == target => t,
        // Covered or finished since it asked: a parked Activity keeps its
        // instance here, so there is nothing to re-create it for.
        _ => {
            crate::pd_warn!("recreate(): not the foreground Activity any more; ignored");
            return LifecycleControl::Continue;
        }
    };
    crate::pd_info!("activity: recreate {}", top_class);
    if destroy_top_instance(jvm, top_ref, true, heap, handler).is_break() {
        return LifecycleControl::Break;
    }
    let saved = handler.top_saved_state();
    let Some(new_ref) = instantiate_component(jvm, top_class, heap, handler) else {
        // Nothing to put in the entry: finish it instead, as a plain pop.
        log_error!(
            "recreate: failed to instantiate {}; finishing it",
            top_class
        );
        let pending_result = handler.top_activity_result();
        return pop_and_resume_parent(jvm, pending_result, heap, handler);
    };
    // Straight into the entry: from here the stack roots the new instance.
    handler.replace_top_activity(new_ref);
    // A fresh keypad focus group for the new view tree; the old one emptied
    // when its tree was deleted.
    crate::graphics::lvgl::events::pop_activity_group();
    crate::graphics::lvgl::events::push_activity_group();
    let control = start_activity_instance(jvm, new_ref, saved, None, heap, handler);
    handler.set_top_saved_state(None);
    #[cfg(feature = "mem-diag")]
    crate::mem_diag::note_activity_transition();
    control
}

/// Process a single Activity or Service transition, invoking the canonical
/// Android lifecycle callback sequence. See the doc comment on
/// [`run_activity`] for the v1 view-preservation caveat.
#[cfg(not(test))]
pub(super) fn process_pending_op(
    jvm: &mut Jvm,
    op: crate::native_handler::PendingOp,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    use crate::native_handler::{PendingActivityOp, PendingOp};

    match op {
        PendingOp::Service(s) => {
            crate::service_lifecycle::process_pending_service_op(jvm, s, heap, handler)
        }
        PendingOp::Activity(PendingActivityOp::Push {
            class_name,
            intent_ref,
            request_code,
            caller,
        }) => handle_push_op(
            jvm,
            class_name,
            intent_ref,
            request_code,
            caller,
            heap,
            handler,
        ),
        PendingOp::Activity(PendingActivityOp::Pop { .. }) => handle_pop_op(jvm, heap, handler),
        PendingOp::Activity(PendingActivityOp::Recreate { target }) => {
            handle_recreate_op(jvm, target, heap, handler)
        }
        // Leave for another package: the loop exits through
        // `teardown_activity` (every Activity gets onPause, onStop and
        // onDestroy) and the supervisor runs `packages::next_image()`.
        PendingOp::Activity(PendingActivityOp::Launch) => {
            crate::pd_info!("leaving for a cross-package launch");
            LifecycleControl::Break
        }
    }
}
