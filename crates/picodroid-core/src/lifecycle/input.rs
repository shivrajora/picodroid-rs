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
/// first key with its full field span (action, keyCode, repeatCount, flags,
/// downTime, eventTime), every field rewritten per event, so steady-state
/// key dispatch allocates nothing. Listeners must not retain the event past
/// the callback (Android's KeyEvent pool contract).
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
    key_repeat().reset();
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

/// Held keys and their auto-repeat clocks — `KeyEvent.getRepeatCount()` and
/// the long-press. Fed by [`dispatch_key_events`] as it drains the edge queue
/// and polled once per tick after the drain; reset between app runs by
/// [`reset_dispatch_event_state`] and on an Activity transition.
static mut KEY_REPEAT: crate::graphics::lvgl::key_repeat::KeyRepeat =
    crate::graphics::lvgl::key_repeat::KeyRepeat::new();

fn key_repeat() -> &'static mut crate::graphics::lvgl::key_repeat::KeyRepeat {
    unsafe { &mut *core::ptr::addr_of_mut!(KEY_REPEAT) }
}

/// Mirrors of `KeyEvent`'s Java constants. Hard-coded because there's no
/// enum bridge from Java to Rust for these.
#[cfg(not(test))]
const ACTION_DOWN: i32 = 0;
#[cfg(not(test))]
const ACTION_UP: i32 = 1;
#[cfg(not(test))]
const KEYCODE_HOME: i32 = 3;
#[cfg(not(test))]
const KEYCODE_BACK: i32 = 4;
/// `KeyEvent.KEYCODE_POWER` / `SLEEP` / `WAKEUP`: display power keys the
/// framework takes (power.rs).
const KEYCODE_POWER: i32 = 26;
const KEYCODE_SLEEP: i32 = 223;
const KEYCODE_WAKEUP: i32 = 224;
/// `KeyEvent.FLAG_CANCELED`: a release whose press's action must not run —
/// here, the release of a BACK whose hold became HOME.
const FLAG_CANCELED: i32 = 0x20;
/// `KeyEvent.FLAG_LONG_PRESS`: set on the first repeat, the long-press.
#[cfg(not(test))]
const FLAG_LONG_PRESS: i32 = 0x80;

/// One key edge or synthetic repeat, as the recycled `KeyEvent` describes it.
#[cfg(not(test))]
#[derive(Clone, Copy)]
struct KeyRecord {
    keycode: i32,
    action: i32,
    repeat_count: i32,
    flags: i32,
    /// `KeyEvent.getDownTime()`: elapsedRealtime millis of the press.
    down_ms: u64,
    /// `KeyEvent.getEventTime()`: elapsedRealtime millis of this edge.
    event_ms: u64,
}

/// Drain the hardware key-event queue and dispatch each edge to the focused
/// View's `OnKeyListener`, then to the top Activity's key callbacks; then
/// synthesise the auto-repeats due for keys still held. The BACK route to
/// `onBackPressed` lives in `Activity.java` (`KeyEvent.dispatch`).
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
    use crate::graphics::lvgl::key_repeat::edge_time_ms;

    let now_us = crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000;
    let now_ms = now_us / 1_000;

    // 0) Keys with no pin behind them (`input_inject::push_soft_key`): the
    //    soft-nav control's BACK and HOME, the HOME a held BACK synthesises
    //    on a board with no HOME key, and `input keyevent` for a key this
    //    board lacks. Routed exactly as a pin's edge is, stamped with the
    //    moment of dispatch, and ahead of the ring: a held BACK's HOME must
    //    land before that BACK's own release.
    while let Some(soft) = crate::input_inject::drain_soft_key() {
        let down_ms = track_hold(soft.keycode, soft.action, now_ms);
        let stop = route_key(
            jvm,
            heap,
            handler,
            KeyRecord {
                keycode: soft.keycode,
                action: soft.action,
                repeat_count: soft.repeat,
                flags: soft.flags,
                down_ms,
                event_ms: now_ms,
            },
        );
        if stop {
            return;
        }
    }

    while let Some(raw) = events::drain_key_event() {
        let keycode = match events::pin_to_keycode(raw.pin) {
            Some(k) => k,
            None => continue,
        };
        let action = if raw.rising { ACTION_UP } else { ACTION_DOWN };
        let edge_ms = edge_time_ms(now_us, raw.t_us);
        // A BACK whose hold became HOME: its release arrives cancelled, as
        // Android cancels the release after a handled long-press, so the
        // default `onKeyUp` keeps `onBackPressed` out of the launcher's way.
        let flags =
            if keycode == KEYCODE_BACK && action == ACTION_UP && events::take_back_up_cancelled() {
                FLAG_CANCELED
            } else {
                0
            };
        let down_ms = track_hold(keycode, action, edge_ms);
        let stop = route_key(
            jvm,
            heap,
            handler,
            KeyRecord {
                keycode,
                action,
                repeat_count: 0,
                flags,
                down_ms,
                event_ms: edge_ms,
            },
        );
        if stop {
            return;
        }
    }

    // 4) Auto-repeat: a key held past the repeat timeout yields a synthetic
    //    DOWN every repeat delay, the first flagged as the long-press
    //    (`KeyEvent.dispatch` turns that into `onKeyLongPress`). Polled after
    //    the drain, so a press and release queued together never repeat, and
    //    once per tick, so a repeat is late by at most one tick.
    while let Some(rep) = key_repeat().next_due(now_ms) {
        let flags = if rep.repeat_count == 1 {
            FLAG_LONG_PRESS
        } else {
            0
        };
        deliver_key(
            jvm,
            heap,
            handler,
            KeyRecord {
                keycode: rep.keycode,
                action: ACTION_DOWN,
                repeat_count: rep.repeat_count,
                flags,
                down_ms: rep.down_ms,
                event_ms: now_ms,
            },
        );
        if handler.has_pending_activity_transition() {
            key_repeat().reset();
            return;
        }
    }
}

/// Hold bookkeeping before any route, so a release the framework consumes
/// (HOME, a keyboard or dialog dismissal) still ends its key's repeats.
/// Returns `KeyEvent.getDownTime()` for the edge. HOME never reaches Java,
/// so it never repeats either.
#[cfg(not(test))]
fn track_hold(keycode: i32, action: i32, edge_ms: u64) -> u64 {
    if keycode == KEYCODE_HOME {
        edge_ms
    } else if action == ACTION_UP {
        key_repeat().release(keycode).unwrap_or(edge_ms)
    } else {
        key_repeat().press(keycode, edge_ms);
        edge_ms
    }
}

/// Route one key edge — the framework's claims first, then the focused View
/// and the Activity. Returns `true` when the key started an Activity
/// transition and the caller must stop draining: the remaining queued keys
/// belong to the *next* top Activity and must wait until the transition is
/// applied (between frames). Without this, a fast burst whose key events
/// land in a single tick — before the push/pop is processed — is delivered
/// entirely to the *departing* Activity, e.g. double-launching it; combined
/// with a deferred service bind that then mutates the first instance's freed
/// views, that was the History-screen segfault. See
/// project_picoenvmon_history_segfault. The held keys belong to the
/// departing screen too: their repeats stop here, and their eventual release
/// finds nothing tracked on the new one.
#[cfg(not(test))]
fn route_key(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    rec: KeyRecord,
) -> bool {
    let (keycode, action) = (rec.keycode, rec.action);

    // 0) HOME goes to the launcher from anywhere, and nothing on the way
    //    gets a say — not a focused View's OnKeyListener, not a showing
    //    dialog, not `onBackPressed`. Android reserves HOME the same way:
    //    an app cannot trap the user by consuming it. The op tears every
    //    Activity down and returns, and the supervisor then runs
    //    `packages::next_image()`, which hands back the launcher this
    //    request just made pending.
    if keycode == KEYCODE_HOME {
        if action == ACTION_UP {
            if crate::packages::request_home() {
                use crate::native_handler::{PendingActivityOp, PendingOp};
                handler.enqueue_op(PendingOp::Activity(PendingActivityOp::Launch));
                crate::pd_info!("key: HOME -> launcher");
            } else {
                crate::pd_info!("key: HOME swallowed (nothing to go home to)");
            }
        }
        // Both edges are consumed: a board with no launcher swallows HOME
        // rather than delivering a keycode no app is expected to handle,
        // and the press must not reach `onKeyDown` either — it used to,
        // which let an app watch for a key it could never act on.
        return false;
    }

    // 0b) The power keys are the framework's too (power.rs): SLEEP dozes,
    //     WAKEUP wakes, POWER toggles — on the release, like HOME.
    if keycode == KEYCODE_SLEEP || keycode == KEYCODE_WAKEUP || keycode == KEYCODE_POWER {
        if action == ACTION_UP {
            match keycode {
                KEYCODE_SLEEP => crate::power::request_doze(crate::power::DozeCause::SleepKey),
                KEYCODE_WAKEUP => crate::power::request_wake(),
                _ => crate::power::request_toggle(),
            }
            crate::pd_info!("key: power key {} -> display", keycode);
        }
        return false;
    }

    // 1) BACK release first tries to dismiss the system soft keyboard
    //    if it's visible. Consumed if so — Activity stays on screen.
    if keycode == KEYCODE_BACK && action == ACTION_UP {
        use crate::graphics::lvgl::widgets::keyboard;
        if keyboard::hide_system() {
            crate::pd_info!("key: BACK -> keyboard dismissed");
            return false;
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
            return false;
        }
    }

    // 2) + 3) The focused View, then the Activity.
    deliver_key(jvm, heap, handler, rec);

    if handler.has_pending_activity_transition() {
        key_repeat().reset();
        return true;
    }
    false
}

/// Fill the recycled `KeyEvent` from `rec` and offer it to the focused View's
/// `OnKeyListener`, then — Android's fallback when no View took the edge — to
/// the Activity's `performKeyEvent`, which runs `KeyEvent.dispatch`:
/// `onKeyDown` / `onKeyLongPress` / `onKeyUp` and the press-to-release
/// tracking the default BACK handling relies on.
#[cfg(not(test))]
fn deliver_key(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    rec: KeyRecord,
) {
    use crate::graphics::lvgl::events;

    let event_obj = match fill_key_event(&rec, heap, handler) {
        Some(o) => o,
        None => return,
    };
    let (mut consumed, had_focus) = match events::focused_view_obj() {
        Some(view_ref) => (
            fire_key_site(
                jvm,
                dispatch_sites::VIEW_KEY,
                view_ref,
                &[Value::ObjectRef(event_obj)],
                heap,
                handler,
            ),
            true,
        ),
        None => (false, false),
    };
    if !consumed {
        if let Some((act_ref, _)) = handler.current_activity() {
            consumed = fire_key_site(
                jvm,
                dispatch_sites::ACTIVITY_KEY_EVENT,
                act_ref,
                &[Value::ObjectRef(event_obj)],
                heap,
                handler,
            );
        }
    }
    // The repeat stream would be a line every 50 ms: the press, the
    // long-press and the release are the edges worth one.
    if rec.repeat_count <= 1 {
        crate::pd_info!(
            "key: code={} action={} repeat={} consumed={} focus={}",
            rec.keycode,
            rec.action,
            rec.repeat_count,
            consumed,
            had_focus
        );
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

/// Write one key edge or repeat into the recycled `KeyEvent` and return it.
/// Every field is rewritten, `flags` included: the tracking and cancel bits
/// live in the Activity's Java `DispatcherState` between edges, not here.
#[cfg(not(test))]
fn fill_key_event(
    rec: &KeyRecord,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> Option<u16> {
    use crate::graphics::fields::key_event as f;

    let event_obj = ensure_recycled_key_event(heap, handler)?;
    for (slot, value) in [
        (f::ACTION, Value::Int(rec.action)),
        (f::KEY_CODE, Value::Int(rec.keycode)),
        (f::REPEAT_COUNT, Value::Int(rec.repeat_count)),
        (f::FLAGS, Value::Int(rec.flags)),
        (f::DOWN_TIME, Value::Long(rec.down_ms as i64)),
        (f::EVENT_TIME, Value::Long(rec.event_ms as i64)),
    ] {
        heap.objects.set_field(event_obj, slot, value)?;
    }
    Some(event_obj)
}

/// Invoke a `boolean`-returning key site (`View.fireKey`,
/// `Activity.performKeyEvent`) on `target_ref` and return whether it
/// consumed the event (returned non-zero). Helper for [`deliver_key`].
#[cfg(not(test))]
fn fire_key_site(
    jvm: &mut Jvm,
    site: usize,
    target_ref: u16,
    args: &[pico_jvm::types::Value],
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> bool {
    use pico_jvm::types::Value;

    let mut ret = jvm.invoke_instance_with_args_returning(
        dispatch_class(site),
        dispatch_method(site),
        target_ref,
        args,
        heap,
        handler,
    );
    if matches!(ret, Err(pico_jvm::types::JvmError::StackOverflow)) {
        // Allocation failure inside the handler's Java — collect with
        // native-only roots (no safepoint runs out here) and retry once.
        heap.collect_now(handler);
        ret = jvm.invoke_instance_with_args_returning(
            dispatch_class(site),
            dispatch_method(site),
            target_ref,
            args,
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
