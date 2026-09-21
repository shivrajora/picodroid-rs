// SPDX-License-Identifier: GPL-3.0-only
//! The view touch-listener registry and its event queue, the
//! click-suppression flags (a consumed `onTouch` or long-press swallows the
//! click), and the single-slot screen press hook.

use super::*;

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
pub(super) static mut VIEW_TOUCH_MAP: PtrMap<MAX_TOUCH_LISTENERS> = PtrMap::new();

pub(super) unsafe extern "C" fn touch_map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe { map_mut(&raw mut VIEW_TOUCH_MAP).remove(obj) }
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
pub(super) static mut TOUCH_QUEUE: [TouchRecord; TOUCH_QUEUE_SIZE] =
    [EMPTY_TOUCH; TOUCH_QUEUE_SIZE];
pub(super) static mut TOUCH_QUEUE_HEAD: usize = 0;
pub(super) static mut TOUCH_QUEUE_TAIL: usize = 0;

// Producer-local "last MOVE we pushed" snapshot used to coalesce LVGL
// PRESSING events that report the same coordinates as the previous
// sample. LVGL fires PRESSING every indev refresh tick whether or not
// the finger moved; without this filter a held finger floods the queue.
// Single shared slot is correct under the v1 single-touch assumption
// (matching every other touch path). i32::MIN sentinel ensures the
// first PRESSING after each fresh press always pushes.
pub(super) static mut LAST_PRESSING_VIEW: usize = 0;
pub(super) static mut LAST_PRESSING_X: i32 = i32::MIN;
pub(super) static mut LAST_PRESSING_Y: i32 = i32::MIN;

/// Read the current monotonic ms from the LVGL tick clock. Used as the
/// timestamp on each touch event; aligns with the same clock GestureDetector
/// uses for fling-velocity duration math.
pub(super) fn now_ms_for_touch() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

pub(super) fn push_touch(record: TouchRecord) {
    unsafe {
        let head = TOUCH_QUEUE_HEAD;
        let next = (head + 1) % TOUCH_QUEUE_SIZE;
        if next != TOUCH_QUEUE_TAIL {
            TOUCH_QUEUE[head] = record;
            TOUCH_QUEUE_HEAD = next;
        }
    }
}

pub(super) unsafe fn touch_event_record(e: *mut lv_event_t, action: i32) -> Option<TouchRecord> {
    let target = unsafe { lv_event_get_target_obj(e) };
    if target.is_null() {
        return None;
    }
    let indev = unsafe { lv_event_get_indev(e) };
    if indev.is_null() {
        return None;
    }
    let mut p = lv_point_t { x: 0, y: 0 };
    unsafe { lv_indev_get_point(indev, &mut p as *mut lv_point_t) };
    // Capture the target's screen-absolute origin so the dispatcher can make
    // getX/getY view-relative (Android) while keeping getRawX/getRawY screen.
    let mut coords = lv_area_t {
        x1: 0,
        y1: 0,
        x2: 0,
        y2: 0,
    };
    unsafe { lv_obj_get_coords(target, &mut coords as *mut lv_area_t) };
    Some(TouchRecord {
        view_handle: target as usize,
        action,
        x: p.x,
        y: p.y,
        origin_x: coords.x1,
        origin_y: coords.y1,
        time_ms: now_ms_for_touch(),
    })
}

pub(super) fn reset_pressing_coalesce() {
    unsafe {
        LAST_PRESSING_VIEW = 0;
        LAST_PRESSING_X = i32::MIN;
        LAST_PRESSING_Y = i32::MIN;
    }
}

pub(super) unsafe extern "C" fn touch_press_cb(e: *mut lv_event_t) {
    reset_pressing_coalesce();
    if let Some(rec) = unsafe { touch_event_record(e, ACTION_DOWN) } {
        push_touch(rec);
    }
}

pub(super) unsafe extern "C" fn touch_release_cb(e: *mut lv_event_t) {
    reset_pressing_coalesce();
    if let Some(rec) = unsafe { touch_event_record(e, ACTION_UP) } {
        push_touch(rec);
    }
}

pub(super) unsafe extern "C" fn touch_long_press_cb(e: *mut lv_event_t) {
    if let Some(rec) = unsafe { touch_event_record(e, ACTION_LONG_PRESS) } {
        push_touch(rec);
    }
}

pub(super) unsafe extern "C" fn touch_pressing_cb(e: *mut lv_event_t) {
    if let Some(rec) = unsafe { touch_event_record(e, ACTION_MOVE) } {
        unsafe {
            if LAST_PRESSING_VIEW == rec.view_handle
                && LAST_PRESSING_X == rec.x
                && LAST_PRESSING_Y == rec.y
            {
                return;
            }
            LAST_PRESSING_VIEW = rec.view_handle;
            LAST_PRESSING_X = rec.x;
            LAST_PRESSING_Y = rec.y;
        }
        push_touch(rec);
    }
}

/// Record a Java `View` object as the touch-listener target for the given
/// `nativeHandle` id, and register the LVGL press/pressing/release/long-press
/// callbacks on the underlying object. Idempotent: re-registration just
/// updates the obj_ref slot (no duplicate LVGL callbacks — LVGL tolerates
/// duplicates but we'd waste slot table space).
pub fn register_view_touch_listener(id: i32, obj_ref: u16) {
    let raw_obj = super::super::handle_table::lookup(id);
    if raw_obj.is_null() {
        return;
    }
    let raw_ptr = raw_obj as usize;

    unsafe {
        match map_mut(&raw mut VIEW_TOUCH_MAP).upsert(raw_ptr, obj_ref) {
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
        lv_obj_add_event_cb(
            raw_obj,
            Some(touch_press_cb),
            LV_EVENT_PRESSED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            raw_obj,
            Some(touch_release_cb),
            LV_EVENT_RELEASED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            raw_obj,
            Some(touch_long_press_cb),
            LV_EVENT_LONG_PRESSED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            raw_obj,
            Some(touch_pressing_cb),
            LV_EVENT_PRESSING,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            raw_obj,
            Some(touch_map_delete_cb),
            LV_EVENT_DELETE,
            core::ptr::null_mut(),
        );
    }
}

/// Pop one touch event from the queue, if any.
pub fn drain_touch_event() -> Option<TouchRecord> {
    unsafe {
        if TOUCH_QUEUE_TAIL == TOUCH_QUEUE_HEAD {
            return None;
        }
        let r = TOUCH_QUEUE[TOUCH_QUEUE_TAIL];
        TOUCH_QUEUE_TAIL = (TOUCH_QUEUE_TAIL + 1) % TOUCH_QUEUE_SIZE;
        Some(r)
    }
}

/// Look up the Java `View` object reference for a registered LVGL widget.
pub fn lookup_touch_view_obj(handle: usize) -> Option<u16> {
    unsafe { map_ref(&raw const VIEW_TOUCH_MAP).lookup(handle) }
}

pub fn reset_view_touch_listener_state() {
    unsafe {
        map_mut(&raw mut VIEW_TOUCH_MAP).reset();
        TOUCH_QUEUE_HEAD = 0;
        TOUCH_QUEUE_TAIL = 0;
        CLICK_SUPPRESS_LEN = 0;
    }
    reset_pressing_coalesce();
}

// ── Click-suppression flags (Android: consumed onTouch / consumed long-press
//    suppress the synthetic click) ───────────────────────────────────────────
//
// Per-widget handle (raw lv_obj_t*) flags — no Java refs, so no GC rooting.
// Set when onTouch or onLongClick returns true; cleared on ACTION_DOWN (start
// of a fresh gesture) and consumed (check-and-clear) by the click dispatcher.

pub(super) const MAX_SUPPRESS: usize = 32;
pub(super) static mut CLICK_SUPPRESS: [usize; MAX_SUPPRESS] = [0; MAX_SUPPRESS];
pub(super) static mut CLICK_SUPPRESS_LEN: usize = 0;

/// Mark `handle`'s next synthetic click as suppressed (onTouch / long-press
/// consumed the gesture).
pub fn set_click_suppressed(handle: usize) {
    unsafe {
        for &h in &CLICK_SUPPRESS[..CLICK_SUPPRESS_LEN] {
            if h == handle {
                return;
            }
        }
        if CLICK_SUPPRESS_LEN < MAX_SUPPRESS {
            CLICK_SUPPRESS[CLICK_SUPPRESS_LEN] = handle;
            CLICK_SUPPRESS_LEN += 1;
        }
    }
}

/// Clear `handle`'s suppress flag — called on ACTION_DOWN to start each
/// gesture clean.
pub fn clear_click_suppressed(handle: usize) {
    unsafe {
        let len = CLICK_SUPPRESS_LEN;
        if let Some(i) = CLICK_SUPPRESS[..len].iter().position(|&h| h == handle) {
            CLICK_SUPPRESS[i] = CLICK_SUPPRESS[len - 1];
            CLICK_SUPPRESS_LEN = len - 1;
        }
    }
}

/// Check-and-clear: returns whether the next click for `handle` should be
/// suppressed, consuming the flag. Called by the click dispatcher.
pub fn take_click_suppressed(handle: usize) -> bool {
    unsafe {
        let len = CLICK_SUPPRESS_LEN;
        if let Some(i) = CLICK_SUPPRESS[..len].iter().position(|&h| h == handle) {
            CLICK_SUPPRESS[i] = CLICK_SUPPRESS[len - 1];
            CLICK_SUPPRESS_LEN = len - 1;
            true
        } else {
            false
        }
    }
}

// ── Screen-level press hook (single-slot, used by soft keyboard dismiss) ────
//
// Soft keyboard's press-outside-to-dismiss attaches a transient callback to
// the active screen for `LV_EVENT_PRESSED`. We track the currently-attached
// fn pointer in a static so:
//   - re-attach during the same visibility cycle is a no-op (idempotent),
//   - detach knows which cb to remove.
// Single-slot is sufficient: the keyboard is the only consumer today and
// the plan explicitly defers generalizing until a second one appears.

use crate::lvgl_ffi::{
    lv_event_cb_t, lv_obj_add_event_cb, lv_obj_remove_event_cb, lv_screen_active, LV_EVENT_PRESSED,
};

pub(super) static mut SCREEN_PRESS_HOOK: lv_event_cb_t = None;

/// Attach `cb` to the active screen as an `LV_EVENT_PRESSED` listener.
/// Idempotent — a second call detaches whatever was previously attached
/// before re-attaching, so only one screen hook is ever live.
///
/// Note: we don't short-circuit when the previous and current `cb` are
/// the same fn pointer because Rust's `unpredictable_function_pointer_comparisons`
/// lint correctly warns that fn-pointer equality isn't reliable across
/// codegen units. Detach-then-re-attach is two cheap LVGL list
/// operations and is unconditionally correct.
pub fn attach_screen_press_hook(cb: lv_event_cb_t) {
    unsafe {
        if let Some(prev) = SCREEN_PRESS_HOOK {
            lv_obj_remove_event_cb(lv_screen_active(), Some(prev));
        }
        if cb.is_some() {
            lv_obj_add_event_cb(
                lv_screen_active(),
                cb,
                LV_EVENT_PRESSED,
                core::ptr::null_mut(),
            );
        }
        SCREEN_PRESS_HOOK = cb;
    }
}

/// Detach the screen-level press hook, if one is attached.
pub fn detach_screen_press_hook() {
    unsafe {
        if let Some(prev) = SCREEN_PRESS_HOOK {
            lv_obj_remove_event_cb(lv_screen_active(), Some(prev));
            SCREEN_PRESS_HOOK = None;
        }
    }
}

pub fn reset_screen_press_hook_state() {
    unsafe {
        // The screen widget itself is being torn down, so we only need to
        // drop our cached pointer — the underlying event_cb registration
        // dies with the screen.
        SCREEN_PRESS_HOOK = None;
    }
}
