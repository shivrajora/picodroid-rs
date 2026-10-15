// SPDX-License-Identifier: GPL-3.0-only
//! The view touch-listener registry and its event queue, the
//! click-suppression flags (a consumed `onTouch` or long-press swallows the
//! click), and the single-slot screen press hook.

use super::*;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;
use crate::util::local_set::LocalSet;
use core::cell::Cell;

// ── View touch-listener registry ────────────────────────────────────────────
//
// Mirrors the key-listener pattern above: a small (handle, obj_ref) map,
// plus a ring buffer fed by LVGL trampolines on PRESSED / PRESSING /
// RELEASED / LONG_PRESSED. The framework loop drains the queue, allocates
// a Java MotionEvent, and invokes `View.fireTouch` on the matching object.
//
// PRESSING is coalesced down to actual movement (see LAST_PRESSING_*
// snapshot below) so a held finger doesn't flood the queue at the indev
// refresh rate.
//
// Each registered View flips on `LV_OBJ_FLAG_CLICKABLE` so the active
// touch indev actually routes hit-tested events here. This is harmless
// for widgets that are already clickable (Button, Switch, etc.) and is
// what makes touch listeners work on otherwise-passive widgets like
// TextView and LinearLayout.

pub(super) const MAX_TOUCH_LISTENERS: usize = 32;

lv_event_cb! {
    pub(super) fn touch_map_delete_cb(e) {
        let obj = e.target().addr();
        TOUCH.view_map.remove(obj)
    }
}

/// Action codes — must match the constants on `picodroid.view.MotionEvent`.
pub(super) const ACTION_DOWN: i32 = 0;
pub(super) const ACTION_UP: i32 = 1;
pub(super) const ACTION_MOVE: i32 = 2;
pub(super) const ACTION_LONG_PRESS: i32 = 3;

#[derive(Copy, Clone)]
pub struct TouchRecord {
    pub view_handle: usize,
    pub action: i32,
    /// Screen-absolute touch point (LVGL `lv_indev_get_point`).
    pub x: i32,
    pub y: i32,
    /// Screen-absolute top-left of the target view at event time. The
    /// dispatcher subtracts this from `(x, y)` to produce Android's
    /// view-relative `getX()`/`getY()`, keeping `(x, y)` as `getRawX()`/
    /// `getRawY()`.
    pub origin_x: i32,
    pub origin_y: i32,
    pub time_ms: u64,
}

pub(super) const EMPTY_TOUCH: TouchRecord = TouchRecord {
    view_handle: 0,
    action: 0,
    x: 0,
    y: 0,
    origin_x: 0,
    origin_y: 0,
    time_ms: 0,
};

// Bumped from 32 to 64 when ACTION_MOVE delivery landed: a fast finger
// crossing 240 px at 60 Hz produces ~60 distinct positions even after
// coalescing identical samples, and 32 became the obvious choke point.
pub(super) const TOUCH_QUEUE_SIZE: usize = 64;

pub(super) const MAX_SUPPRESS: usize = 32;

pub(super) struct Touch {
    pub(super) view_map: PtrMap<MAX_TOUCH_LISTENERS>,
    queue: LocalRing<TouchRecord, TOUCH_QUEUE_SIZE>,
    // Producer-local "last MOVE we pushed" snapshot used to coalesce LVGL
    // PRESSING events that report the same coordinates as the previous
    // sample. LVGL fires PRESSING every indev refresh tick whether or not
    // the finger moved; without this filter a held finger floods the queue.
    // Single shared slot is correct under the v1 single-touch assumption
    // (matching every other touch path). A view of 0 is the "nothing yet"
    // sentinel: no record names a null view, so the first PRESSING after
    // each fresh press always pushes. (All-zero, so the state sits in .bss.)
    last_pressing_view: Cell<usize>,
    last_pressing_x: Cell<i32>,
    last_pressing_y: Cell<i32>,
    /// The click-suppression flags (below).
    click_suppress: LocalSet<usize, MAX_SUPPRESS>,
    /// The pointer press hook (below).
    pointer_press_hook: Cell<lv_event_cb_t>,
}

// SAFETY: filled by LVGL's touch callbacks, which run from the UI task's
// timer handler, and read and drained by natives: all core-0 JVM tasks.
pub(super) static TOUCH: Core0<Touch> = unsafe {
    Core0::new(Touch {
        view_map: PtrMap::new(),
        queue: LocalRing::new(EMPTY_TOUCH),
        last_pressing_view: Cell::new(0),
        last_pressing_x: Cell::new(0),
        last_pressing_y: Cell::new(0),
        click_suppress: LocalSet::new(0),
        pointer_press_hook: Cell::new(None),
    })
};

/// Read the current monotonic ms from the LVGL tick clock. Used as the
/// timestamp on each touch event; aligns with the same clock GestureDetector
/// uses for fling-velocity duration math.
pub(super) fn now_ms_for_touch() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

pub(super) fn push_touch(record: TouchRecord) {
    TOUCH.queue.push(record);
}

pub(super) fn touch_event_record(e: &ObjEvent<'_>, action: i32) -> Option<TouchRecord> {
    let target = e.target();
    let indev = e.indev()?;
    let mut p = lv_point_t { x: 0, y: 0 };
    lv_indev_get_point(indev, &mut p);
    // Capture the target's screen-absolute origin so the dispatcher can make
    // getX/getY view-relative (Android) while keeping getRawX/getRawY screen.
    let mut coords = lv_area_t {
        x1: 0,
        y1: 0,
        x2: 0,
        y2: 0,
    };
    lv_obj_get_coords(target, &mut coords);
    Some(TouchRecord {
        view_handle: target.addr(),
        action,
        x: p.x,
        y: p.y,
        origin_x: coords.x1,
        origin_y: coords.y1,
        time_ms: now_ms_for_touch(),
    })
}

pub(super) fn reset_pressing_coalesce() {
    TOUCH.last_pressing_view.set(0);
    TOUCH.last_pressing_x.set(0);
    TOUCH.last_pressing_y.set(0);
}

lv_event_cb! {
    pub(super) fn touch_press_cb(e) {
        reset_pressing_coalesce();
        if let Some(rec) = touch_event_record(&e, ACTION_DOWN) {
            push_touch(rec);
        }
    }
}

lv_event_cb! {
    pub(super) fn touch_release_cb(e) {
        reset_pressing_coalesce();
        if let Some(rec) = touch_event_record(&e, ACTION_UP) {
            push_touch(rec);
        }
    }
}

lv_event_cb! {
    pub(super) fn touch_long_press_cb(e) {
        if let Some(rec) = touch_event_record(&e, ACTION_LONG_PRESS) {
            push_touch(rec);
        }
    }
}

lv_event_cb! {
    pub(super) fn touch_pressing_cb(e) {
        if let Some(rec) = touch_event_record(&e, ACTION_MOVE) {
            if TOUCH.last_pressing_view.get() == rec.view_handle
                && TOUCH.last_pressing_x.get() == rec.x
                && TOUCH.last_pressing_y.get() == rec.y
            {
                return;
            }
            TOUCH.last_pressing_view.set(rec.view_handle);
            TOUCH.last_pressing_x.set(rec.x);
            TOUCH.last_pressing_y.set(rec.y);
            push_touch(rec);
        }
    }
}

/// Record a Java `View` object as the touch-listener target for the given
/// `nativeHandle` id, and register the LVGL press/pressing/release/long-press
/// callbacks on the underlying object. Idempotent: re-registration just
/// updates the obj_ref slot (no duplicate LVGL callbacks — LVGL tolerates
/// duplicates but we'd waste slot table space).
pub fn register_view_touch_listener(id: i32, obj_ref: u16) {
    let Some(raw_obj) = super::super::handle_table::get(id) else {
        return;
    };

    {
        match TOUCH.view_map.upsert(raw_obj.addr(), obj_ref) {
            // Already registered with LVGL — the obj_ref was refreshed.
            Upsert::Updated => return,
            Upsert::Full => {
                warn_full("view-touch");
                return;
            }
            Upsert::Inserted => {}
        }

        // Make the widget clickable so the touch indev hit-tests it. This
        // is a no-op for widgets that are already clickable (Button,
        // Switch, etc.) and is what makes touch work on label-based
        // widgets like TextView.
        lv_obj_add_flag(raw_obj, LV_OBJ_FLAG_CLICKABLE);

        // Register the four event callbacks. LVGL allows multiple
        // descriptors on the same (obj, code) pair so existing
        // click-handling on Buttons isn't disturbed.
        add_event_cb(raw_obj, touch_press_cb, LV_EVENT_PRESSED);
        add_event_cb(raw_obj, touch_release_cb, LV_EVENT_RELEASED);
        add_event_cb(raw_obj, touch_long_press_cb, LV_EVENT_LONG_PRESSED);
        add_event_cb(raw_obj, touch_pressing_cb, LV_EVENT_PRESSING);
        add_event_cb(raw_obj, touch_map_delete_cb, LV_EVENT_DELETE);
    }
}

/// Pop one touch event from the queue, if any.
pub fn drain_touch_event() -> Option<TouchRecord> {
    TOUCH.queue.pop()
}

/// Look up the Java `View` object reference for a registered LVGL widget.
pub fn lookup_touch_view_obj(handle: usize) -> Option<u16> {
    TOUCH.view_map.lookup(handle)
}

pub fn reset_view_touch_listener_state() {
    TOUCH.view_map.reset();
    TOUCH.queue.clear();
    TOUCH.click_suppress.clear();
    reset_pressing_coalesce();
}

// ── Click-suppression flags (Android: consumed onTouch / consumed long-press
//    suppress the synthetic click) ───────────────────────────────────────────
//
// Per-widget handle (raw lv_obj_t*) flags — no Java refs, so no GC rooting.
// Set when onTouch or onLongClick returns true; cleared on ACTION_DOWN (start
// of a fresh gesture) and consumed (check-and-clear) by the click dispatcher.

/// Mark `handle`'s next synthetic click as suppressed (onTouch / long-press
/// consumed the gesture). A full registry drops the mark.
pub fn set_click_suppressed(handle: usize) {
    TOUCH.click_suppress.insert(handle);
}

/// Clear `handle`'s suppress flag — called on ACTION_DOWN to start each
/// gesture clean.
pub fn clear_click_suppressed(handle: usize) {
    TOUCH.click_suppress.remove(handle);
}

/// Check-and-clear: returns whether the next click for `handle` should be
/// suppressed, consuming the flag. Called by the click dispatcher.
pub fn take_click_suppressed(handle: usize) -> bool {
    TOUCH.click_suppress.remove(handle)
}

// ── Pointer press hook (single-slot, used by soft keyboard dismiss) ─────────
//
// The soft keyboard's press-outside-to-dismiss attaches a transient callback
// for `LV_EVENT_PRESSED` to the pointer input device. LVGL sends a press to
// the device's own list before the pressed object's, whatever that object's
// bubbling, so every press on the panel reaches it: on the bare screen, on
// the app's root, inside a compat window. A hook on the screen *object* saw
// only presses that reached the screen itself — a tap on the app's own
// container, or anywhere inside a design-size window, left the keyboard up
// (QA F8). We track the attached fn pointer in a static so:
//   - re-attach during the same visibility cycle is a no-op (idempotent),
//   - detach knows which cb to remove.
// Single-slot is sufficient: the keyboard is the only consumer today and
// the plan explicitly defers generalizing until a second one appears.

use crate::lvgl_ffi::{
    lv_event_cb_t, lv_indev_add_event_cb, lv_indev_remove_event_cb_with_user_data, LV_EVENT_PRESSED,
};

/// Attach `cb` to the pointer device as an `LV_EVENT_PRESSED` listener; the
/// pressed object is the event's parameter. Idempotent — a second call
/// detaches whatever was previously attached before re-attaching, so only
/// one hook is ever live. A no-op before the device exists.
///
/// Note: we don't short-circuit when the previous and current `cb` are
/// the same fn pointer because Rust's `unpredictable_function_pointer_comparisons`
/// lint correctly warns that fn-pointer equality isn't reliable across
/// codegen units. Detach-then-re-attach is two cheap LVGL list
/// operations and is unconditionally correct.
///
/// # Safety
/// `cb` must be written for the input device's `LV_EVENT_PRESSED`: its
/// event has no object target, and its parameter is the object pressed.
/// An [`lv_event_cb!`] callback is not one.
pub unsafe fn attach_pointer_press_hook(cb: lv_event_cb_t) {
    let Some(indev) = super::super::lifecycle::pointer_indev() else {
        return;
    };
    if let Some(prev) = TOUCH.pointer_press_hook.get() {
        lv_indev_remove_event_cb_with_user_data(indev, Some(prev), core::ptr::null_mut());
    }
    if cb.is_some() {
        // SAFETY: the caller's contract; a null `user_data`.
        unsafe { lv_indev_add_event_cb(indev, cb, LV_EVENT_PRESSED, core::ptr::null_mut()) };
    }
    TOUCH.pointer_press_hook.set(cb);
}

/// Detach the pointer press hook, if one is attached. The device outlives
/// every app and screen, so this is also what an app reload calls: a
/// registration left behind would fire on the next app's presses.
pub fn detach_pointer_press_hook() {
    if let Some(prev) = TOUCH.pointer_press_hook.get() {
        if let Some(indev) = super::super::lifecycle::pointer_indev() {
            lv_indev_remove_event_cb_with_user_data(indev, Some(prev), core::ptr::null_mut());
        }
        TOUCH.pointer_press_hook.set(None);
    }
}
