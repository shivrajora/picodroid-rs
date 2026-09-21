// SPDX-License-Identifier: GPL-3.0-only
//! Touch, hardware-key, keyboard and editor-action dispatch, and the two
//! recycled event objects (`MotionEvent`, `KeyEvent`) they deliver through —
//! which is why this file owns the GC-root provider for them.

use super::*;

// ── Touch-event dispatch ────────────────────────────────────────────────────

/// Recycled MotionEvent handed to every `View.fireTouch` dispatch. Allocated
/// on first touch with its full field span, rooted via [`visit_gc_roots`],
/// cleared by [`reset_dispatch_event_state`] between app runs. All six fields
/// are rewritten per event, so steady-state touch dispatch allocates nothing.
/// Matches Android, whose MotionEvent.obtain()/recycle() pool reuses event
/// instances (listeners must not retain them past the callback).
///
/// A fresh-per-event MotionEvent leaked: the allocations ran outside any
/// interpreter safepoint (no GC pacing, no emergency GC on failure), and the
/// default-0 field count lazy-grew the fields arena six times per event —
/// ~250 tap gestures ratcheted the arena past 75 KB with zero GCs until its
/// contiguous realloc failed, exactly the sensor-delivery OOM (1ac965f).
pub(super) static mut RECYCLED_MOTION_EVENT: Option<u16> = None;

pub(super) fn recycled_motion_event() -> &'static mut Option<u16> {
    unsafe { &mut *core::ptr::addr_of_mut!(RECYCLED_MOTION_EVENT) }
}

/// Recycled KeyEvent handed to every `View.fireKey` dispatch — the
/// [`RECYCLED_MOTION_EVENT`] pattern applied to hardware keys. Allocated on
/// first key with its full 2-field span (ACTION, KEY_CODE), both fields
/// rewritten per event, so steady-state key dispatch allocates nothing.
/// Listeners must not retain the event past the callback (Android's
/// KeyEvent pool contract).
pub(super) static mut RECYCLED_KEY_EVENT: Option<u16> = None;

pub(super) fn recycled_key_event() -> &'static mut Option<u16> {
    unsafe { &mut *core::ptr::addr_of_mut!(RECYCLED_KEY_EVENT) }
}

/// Emit GC roots owned by the event dispatchers. Called from
/// `PicodroidNativeHandler::gc_visit_roots`.
pub fn visit_gc_roots(visit: &mut dyn FnMut(pico_jvm::types::Value)) {
    if let Some(idx) = *recycled_motion_event() {
        visit(pico_jvm::types::Value::ObjectRef(idx));
    }
    if let Some(idx) = *recycled_key_event() {
        visit(pico_jvm::types::Value::ObjectRef(idx));
    }
}

/// Drop dispatcher-owned heap refs — call before running a new app, next to
/// `SharedJvmHeap::reset()` (the old index would dangle into the new heap).
pub fn reset_dispatch_event_state() {
    *recycled_motion_event() = None;
    *recycled_key_event() = None;
}

/// Return the recycled MotionEvent, allocating it on first use. On
/// allocation failure runs an emergency GC — no interpreter safepoint can
/// relieve pressure out here — and retries once.
#[cfg(not(test))]
pub(super) fn ensure_recycled_motion_event(
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> Option<u16> {
    if let Some(idx) = *recycled_motion_event() {
        return Some(idx);
    }
    let class = c::picodroid_view_MotionEvent;
    let n_fields = crate::graphics::fields::motion_event::SLOTS;
    let idx = match heap.objects.alloc_with_field_count(class, n_fields) {
        Some(i) => i,
        None => {
            heap.collect_now(handler);
            heap.objects.alloc_with_field_count(class, n_fields)?
        }
    };
    #[cfg(feature = "mem-diag")]
    crate::mem_diag::note_native_alloc(1);
    *recycled_motion_event() = Some(idx);
    Some(idx)
}

/// Drain the touch-event queue and invoke `View.fireTouch(MotionEvent)` on
/// the registered listener for each event. Each record carries action
/// (DOWN / UP / LONG_PRESS), display-pixel position, and a tick-clock
/// millisecond timestamp consumed by `GestureDetector` for fling velocity.
#[cfg(not(test))]
pub(super) fn dispatch_touch_events(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::lvgl::events;
    use pico_jvm::types::Value;

    // MotionEvent.ACTION_DOWN value (see MotionEvent.java).
    const ACTION_DOWN: i32 = 0;

    while let Some(rec) = events::drain_touch_event() {
        // ACTION_DOWN starts a fresh gesture — clear any stale suppress flag.
        if rec.action == ACTION_DOWN {
            events::clear_click_suppressed(rec.view_handle);
        }

        let view_ref = match events::lookup_touch_view_obj(rec.view_handle) {
            Some(r) => r,
            None => continue,
        };

        let event_obj = match ensure_recycled_motion_event(heap, handler) {
            Some(o) => o,
            None => continue,
        };
        let mut all_fields_set = true;
        for (slot, value) in [
            (
                crate::graphics::fields::motion_event::ACTION,
                Value::Int(rec.action),
            ),
            (
                // getX/getY are view-relative: screen point minus the
                // target's screen-absolute origin (Android semantics).
                crate::graphics::fields::motion_event::X,
                Value::Int(rec.x - rec.origin_x),
            ),
            (
                crate::graphics::fields::motion_event::Y,
                Value::Int(rec.y - rec.origin_y),
            ),
            (
                crate::graphics::fields::motion_event::EVENT_TIME,
                Value::Long(rec.time_ms as i64),
            ),
            (
                // getRawX/getRawY stay screen-absolute.
                crate::graphics::fields::motion_event::RAW_X,
                Value::Int(rec.x),
            ),
            (
                crate::graphics::fields::motion_event::RAW_Y,
                Value::Int(rec.y),
            ),
        ] {
            if heap.objects.set_field(event_obj, slot, value).is_none() {
                all_fields_set = false;
                break;
            }
        }
        if !all_fields_set {
            continue;
        }

        // fireTouch returns the onTouchListener's boolean. A consumed touch
        // (true) suppresses the synthetic click for this gesture (Android).
        let mut ret = jvm.invoke_instance_with_args_returning(
            dispatch_class(dispatch_sites::VIEW_TOUCH),
            dispatch_method(dispatch_sites::VIEW_TOUCH),
            view_ref,
            &[Value::ObjectRef(event_obj)],
            heap,
            handler,
        );
        if matches!(ret, Err(pico_jvm::types::JvmError::StackOverflow)) {
            // Allocation failure inside the listener's Java — collect with
            // native-only roots (no safepoint runs out here) and retry once.
            heap.collect_now(handler);
            ret = jvm.invoke_instance_with_args_returning(
                dispatch_class(dispatch_sites::VIEW_TOUCH),
                dispatch_method(dispatch_sites::VIEW_TOUCH),
                view_ref,
                &[Value::ObjectRef(event_obj)],
                heap,
                handler,
            );
        }
        // Consuming any event of the gesture suppresses the click; the
        // DOWN-reset above re-arms it each fresh gesture.
        let consumed = matches!(ret, Ok(Some(Value::Int(v))) if v != 0);
        if consumed {
            events::set_click_suppressed(rec.view_handle);
        }
    }
}

// ── Hardware key-event dispatch ─────────────────────────────────────────────

/// Drain the hardware key-event queue, dispatch each event to the focused
/// View's `OnKeyListener`, and route un-consumed BACK releases to the top
/// Activity's `onBackPressed` (which defaults to `finish()`).
///
/// Note: on the host, the sim control channel (`input keyevent …` via
/// stdin/FIFO) injects edges into the same GPIO queue, so this dispatcher
/// fires in sim builds too.
#[cfg(not(test))]
pub(super) fn dispatch_key_events(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::lvgl::events;

    /// Mirrors `KeyEvent.ACTION_UP` and `KeyEvent.KEYCODE_BACK` on the
    /// Java side. Hard-coded because there's no enum bridge from Java to
    /// Rust for these constants.
    const ACTION_UP: i32 = 1;
    const KEYCODE_HOME: i32 = 3;
    const KEYCODE_BACK: i32 = 4;

    while let Some(raw) = events::drain_key_event() {
        let keycode = match events::pin_to_keycode(raw.pin) {
            Some(k) => k,
            None => continue,
        };
        let action = if raw.rising { 1 } else { 0 }; // ACTION_UP : ACTION_DOWN

        // 0) HOME goes to the launcher from anywhere, and nothing on the way
        //    gets a say — not a focused View's OnKeyListener, not a showing
        //    dialog, not `onBackPressed`. Android reserves HOME the same way:
        //    an app cannot trap the user by consuming it. The op tears every
        //    Activity down and returns, and the supervisor then runs
        //    `packages::next_image()`, which hands back the launcher this
        //    request just made pending.
        if keycode == KEYCODE_HOME && action == ACTION_UP {
            if crate::packages::request_home() {
                use crate::native_handler::{PendingActivityOp, PendingOp};
                handler.enqueue_op(PendingOp::Activity(PendingActivityOp::Launch));
                crate::pd_info!("key: HOME -> launcher");
            }
            // Consumed either way: a board with no launcher swallows HOME
            // rather than delivering a keycode no app is expected to handle.
            continue;
        }

        // 1) BACK release first tries to dismiss the system soft keyboard
        //    if it's visible. Consumed if so — Activity stays on screen.
        if keycode == KEYCODE_BACK && action == ACTION_UP {
            use crate::graphics::lvgl::widgets::keyboard;
            if keyboard::hide_system() {
                crate::pd_info!("key: BACK -> keyboard dismissed");
                continue;
            }
        }

        // 1b) BACK then dismisses a showing AlertDialog (Android's cancelable
        //     default) before reaching the focused View or onBackPressed. This
        //     is also the only way to dismiss a dialog on a keypad-only board
        //     with no touch — and it stops the modal scrim from outliving its
        //     Activity. See project_picoenvmon_alertdialog_leak.
        if keycode == KEYCODE_BACK && action == ACTION_UP {
            use crate::graphics::widgets;
            if widgets::has_shown_dialog() {
                widgets::dismiss_topmost_dialog();
                crate::pd_info!("key: BACK -> dialog dismissed");
                continue;
            }
        }

        // 2) Dispatch to the focused View's OnKeyListener, if any. Capture
        //    fireKey's `boolean` return so an un-consumed BACK release can
        //    fall through to onBackPressed below.
        let (consumed, had_focus) = match events::focused_view_obj() {
            Some(view_ref) => (
                fire_view_key(jvm, view_ref, keycode, action, heap, handler),
                true,
            ),
            None => (false, false),
        };
        crate::pd_info!(
            "key: code={} action={} consumed={} focus={}",
            keycode,
            action,
            consumed,
            had_focus
        );

        // 3) Default BACK handler: invoke `Activity.onBackPressed` on the
        //    top activity when no View consumed the BACK release. Apps
        //    that want to suppress finish() can override `onBackPressed`
        //    to a no-op (or show a confirm dialog).
        if !consumed && keycode == KEYCODE_BACK && action == ACTION_UP {
            if let Some((act_ref, _)) = handler.current_activity() {
                let _ = invoke_lifecycle(
                    jvm,
                    dispatch_sites::ACTIVITY_ON_BACK_PRESSED,
                    act_ref,
                    heap,
                    handler,
                );
            }
        }

        // If this key initiated an Activity transition (startActivity /
        // finish), stop draining the batch. The remaining queued keys belong
        // to the *next* top Activity and must wait until the transition is
        // applied (between frames). Without this, a fast burst whose key
        // events land in a single tick — before the push/pop is processed — is
        // delivered entirely to the *departing* Activity, e.g. double-launching
        // it; combined with a deferred service bind that then mutates the
        // first instance's freed views, that was the History-screen segfault.
        // See project_picoenvmon_history_segfault.
        if handler.has_pending_activity_transition() {
            break;
        }
    }
}

// ── Keyboard READY-event dispatch ──────────────────────────────────────────

/// Drain the per-instance Keyboard READY ring buffer and invoke
/// `Keyboard.fireReady()` on each matching Java object. The system
/// keyboard does *not* go through here — it self-hides on its own
/// READY callback in [`crate::graphics::lvgl::widgets::keyboard`].
#[cfg(not(test))]
pub(super) fn dispatch_keyboard_ready(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_keyboard_ready_queue() {
        if let Some(obj_ref) = widgets::lookup_keyboard_obj(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::KEYBOARD_READY),
                dispatch_method(dispatch_sites::KEYBOARD_READY),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── EditText editor-action dispatch ────────────────────────────────────────

/// Drain the system keyboard's pending editor-action and invoke
/// `EditText.fireEditorAction(int)` on the bound Java object. Set by
/// the system keyboard's OK callback in
/// [`crate::graphics::lvgl::widgets::keyboard`].
#[cfg(not(test))]
pub(super) fn dispatch_editor_actions(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;
    use pico_jvm::types::Value;

    if let Some(rec) = widgets::drain_editor_action() {
        let _ = jvm.invoke_instance_with_args(
            dispatch_class(dispatch_sites::EDIT_TEXT_EDITOR_ACTION),
            dispatch_method(dispatch_sites::EDIT_TEXT_EDITOR_ACTION),
            rec.edit_text_ref,
            &[Value::Int(rec.action_id)],
            heap,
            handler,
        );
    }
}

/// Build a `KeyEvent`, invoke `View.fireKey`, and return `true` if the
/// listener consumed the event (`fireKey` returned non-zero). Helper for
/// [`dispatch_key_events`] — keeps the BACK-routing logic readable.
#[cfg(not(test))]
pub(super) fn fire_view_key(
    jvm: &mut Jvm,
    view_ref: u16,
    keycode: i32,
    action: i32,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> bool {
    use pico_jvm::types::Value;

    let event_obj = match ensure_recycled_key_event(heap, handler) {
        Some(o) => o,
        None => return false,
    };
    if heap
        .objects
        .set_field(
            event_obj,
            crate::graphics::fields::key_event::ACTION,
            Value::Int(action),
        )
        .is_none()
    {
        return false;
    }
    if heap
        .objects
        .set_field(
            event_obj,
            crate::graphics::fields::key_event::KEY_CODE,
            Value::Int(keycode),
        )
        .is_none()
    {
        return false;
    }

    let mut ret = jvm.invoke_instance_with_args_returning(
        dispatch_class(dispatch_sites::VIEW_KEY),
        dispatch_method(dispatch_sites::VIEW_KEY),
        view_ref,
        &[Value::ObjectRef(event_obj)],
        heap,
        handler,
    );
    if matches!(ret, Err(pico_jvm::types::JvmError::StackOverflow)) {
        // Allocation failure inside the listener's Java — collect with
        // native-only roots (no safepoint runs out here) and retry once.
        heap.collect_now(handler);
        ret = jvm.invoke_instance_with_args_returning(
            dispatch_class(dispatch_sites::VIEW_KEY),
            dispatch_method(dispatch_sites::VIEW_KEY),
            view_ref,
            &[Value::ObjectRef(event_obj)],
            heap,
            handler,
        );
    }
    matches!(ret, Ok(Some(Value::Int(v))) if v != 0)
}

/// Return the recycled KeyEvent, allocating it on first use with its full
/// field span (no lazy-grow ever). On allocation failure runs an emergency
/// GC — no interpreter safepoint can relieve pressure out here — and
/// retries once. Mirrors [`ensure_recycled_motion_event`].
#[cfg(not(test))]
pub(super) fn ensure_recycled_key_event(
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> Option<u16> {
    if let Some(idx) = *recycled_key_event() {
        return Some(idx);
    }
    let class = c::picodroid_view_KeyEvent;
    let n_fields = crate::graphics::fields::key_event::SLOTS;
    let idx = match heap.objects.alloc_with_field_count(class, n_fields) {
        Some(i) => i,
        None => {
            heap.collect_now(handler);
            heap.objects.alloc_with_field_count(class, n_fields)?
        }
    };
    #[cfg(feature = "mem-diag")]
    crate::mem_diag::note_native_alloc(1);
    *recycled_key_event() = Some(idx);
    Some(idx)
}
