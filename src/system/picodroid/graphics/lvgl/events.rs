//! LVGL keypad indev + Java-visible key-event queue.
//!
//! Splits cleanly into two paths fed by the same hardware GPIO ISR queue:
//! 1. **LVGL keypad indev** — drives focus navigation (`lv_group_*`).
//! 2. **Java-visible queue** — drained by the framework event loop in
//!    `lifecycle.rs` and converted into `picodroid.view.KeyEvent` objects
//!    routed to focused widgets' `OnKeyListener`.
//!
//! Both are populated from the same `keypad_read_cb` so the keypad indev
//! and the Java path see events in lockstep.

#[cfg(has_buttons)]
use crate::hal;
#[allow(unused_imports)]
use crate::lvgl_ffi::*;

// Board-specific button table generated from `[[button]]` in board.toml.
// Entries: (pin, LV_KEY_*, android_keycode). Empty on boards without buttons.
mod button_generated {
    #[allow(unused_imports)]
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/button_config.rs"));
}
use button_generated::BUTTONS;

// ── Public API (kept stable across the migration; engine.rs re-exports) ─────

#[derive(Copy, Clone)]
pub struct KeyEventRaw {
    pub pin: u8,
    pub rising: bool,
}

/// Look up the Android keycode for a hardware button pin.
pub fn pin_to_keycode(pin: u8) -> Option<i32> {
    BUTTONS
        .iter()
        .find(|&&(p, _, _)| p == pin)
        .map(|&(_, _, k)| k)
}

/// Pop one key event from the Java-visible queue, if any.
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_key_event() -> Option<KeyEventRaw> {
    unsafe {
        if KEY_EVENT_QUEUE_TAIL == KEY_EVENT_QUEUE_HEAD {
            return None;
        }
        let event = KEY_EVENT_QUEUE[KEY_EVENT_QUEUE_TAIL];
        KEY_EVENT_QUEUE_TAIL = (KEY_EVENT_QUEUE_TAIL + 1) % KEY_EVENT_QUEUE_SIZE;
        Some(event)
    }
}

/// Clear the key event queue between app runs.
pub fn reset_key_event_queue() {
    unsafe {
        KEY_EVENT_QUEUE_HEAD = 0;
        KEY_EVENT_QUEUE_TAIL = 0;
    }
}

/// Return the Java `View` object reference for LVGL's currently focused
/// widget, if one is registered as a key listener via
/// [`register_view_key_listener`].
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn focused_view_obj() -> Option<u16> {
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return None;
        }
        let focused = lv_group_get_focused(group);
        if focused.is_null() {
            return None;
        }
        lookup_view_obj(focused as usize)
    }
}

// ── View key-listener registry (raw lv_obj_t* → Java View ObjectRef) ────────

const MAX_KEY_LISTENERS: usize = 32;
static mut VIEW_KEY_MAP: [(usize, u16); MAX_KEY_LISTENERS] = [(0, 0); MAX_KEY_LISTENERS];
static mut VIEW_KEY_MAP_LEN: usize = 0;

/// Record a Java `View` object as the key-listener target for the given
/// `nativeHandle` id. The registry keys on the raw `lv_obj_t*` from the
/// handle table because LVGL's focus group also exposes raw pointers.
pub fn register_view_key_listener(id: i32, obj_ref: u16) {
    let raw_ptr = super::handle_table::lookup(id) as usize;
    unsafe {
        for entry in &mut VIEW_KEY_MAP[..VIEW_KEY_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if VIEW_KEY_MAP_LEN < MAX_KEY_LISTENERS {
            VIEW_KEY_MAP[VIEW_KEY_MAP_LEN] = (raw_ptr, obj_ref);
            VIEW_KEY_MAP_LEN += 1;
        }
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
fn lookup_view_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &VIEW_KEY_MAP[..VIEW_KEY_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_view_key_listener_state() {
    unsafe {
        VIEW_KEY_MAP_LEN = 0;
    }
}

/// Initialize the LVGL keypad indev, focus group, and hardware button GPIO
/// pins. Called from `LvglGfx::init` after [`lifecycle::init`] has run.
/// No-op on boards without `[[button]]` entries in board.toml.
pub(in crate::system::picodroid::graphics) fn init_keypad() {
    #[cfg(has_buttons)]
    unsafe {
        let keypad = lv_indev_create();
        lv_indev_set_type(keypad, LV_INDEV_TYPE_KEYPAD);
        lv_indev_set_read_cb(keypad, Some(keypad_read_cb));

        let group = lv_group_create();
        lv_group_set_default(group);
        lv_indev_set_group(keypad, group);
    }

    init_button_pins();
}

#[cfg(has_buttons)]
fn init_button_pins() {
    for &(pin, _, _) in BUTTONS {
        hal::gpio::set_input(pin, hal::gpio::Pull::Up);
        hal::gpio::enable_edge_irq(pin, hal::gpio::EdgeTrigger::Both);
    }
    hal::gpio::init_gpio_irq();
}

#[cfg(not(has_buttons))]
fn init_button_pins() {}

// ── Java-visible key event queue (parallel to LVGL's internal queue) ────────

const KEY_EVENT_QUEUE_SIZE: usize = 16;
static mut KEY_EVENT_QUEUE: [KeyEventRaw; KEY_EVENT_QUEUE_SIZE] = [KeyEventRaw {
    pin: 0,
    rising: false,
}; KEY_EVENT_QUEUE_SIZE];
static mut KEY_EVENT_QUEUE_HEAD: usize = 0;
static mut KEY_EVENT_QUEUE_TAIL: usize = 0;

#[cfg(has_buttons)]
fn push_key_event_raw(pin: u8, rising: bool) {
    unsafe {
        let head = KEY_EVENT_QUEUE_HEAD;
        let next = (head + 1) % KEY_EVENT_QUEUE_SIZE;
        if next != KEY_EVENT_QUEUE_TAIL {
            KEY_EVENT_QUEUE[head] = KeyEventRaw { pin, rising };
            KEY_EVENT_QUEUE_HEAD = next;
        }
    }
}

#[cfg(has_buttons)]
unsafe extern "C" fn keypad_read_cb(_indev: *mut lv_indev_t, data: *mut lv_indev_data_t) {
    let d = unsafe { &mut *data };
    if let Some(event) = hal::gpio::drain_gpio_event() {
        push_key_event_raw(event.pin, event.rising);

        let key = BUTTONS
            .iter()
            .find(|&&(p, _, _)| p == event.pin)
            .map(|&(_, k, _)| k);
        if let Some(k) = key {
            d.key = k;
            d.state = if event.rising {
                LV_INDEV_STATE_RELEASED
            } else {
                LV_INDEV_STATE_PRESSED
            };
        }
        d.continue_reading = hal::gpio::has_pending_event();
    } else {
        d.state = LV_INDEV_STATE_RELEASED;
        d.continue_reading = false;
    }
}

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

const MAX_TOUCH_LISTENERS: usize = 32;
static mut VIEW_TOUCH_MAP: [(usize, u16); MAX_TOUCH_LISTENERS] = [(0, 0); MAX_TOUCH_LISTENERS];
static mut VIEW_TOUCH_MAP_LEN: usize = 0;

/// Action codes — must match the constants on `picodroid.view.MotionEvent`.
const ACTION_DOWN: i32 = 0;
const ACTION_UP: i32 = 1;
const ACTION_MOVE: i32 = 2;
const ACTION_LONG_PRESS: i32 = 3;

#[derive(Copy, Clone)]
pub struct TouchRecord {
    pub view_handle: usize,
    pub action: i32,
    pub x: i32,
    pub y: i32,
    pub time_ms: u64,
}

const EMPTY_TOUCH: TouchRecord = TouchRecord {
    view_handle: 0,
    action: 0,
    x: 0,
    y: 0,
    time_ms: 0,
};

// Bumped from 32 to 64 when ACTION_MOVE delivery landed: a fast finger
// crossing 240 px at 60 Hz produces ~60 distinct positions even after
// coalescing identical samples, and 32 became the obvious choke point.
const TOUCH_QUEUE_SIZE: usize = 64;
static mut TOUCH_QUEUE: [TouchRecord; TOUCH_QUEUE_SIZE] = [EMPTY_TOUCH; TOUCH_QUEUE_SIZE];
static mut TOUCH_QUEUE_HEAD: usize = 0;
static mut TOUCH_QUEUE_TAIL: usize = 0;

// Producer-local "last MOVE we pushed" snapshot used to coalesce LVGL
// PRESSING events that report the same coordinates as the previous
// sample. LVGL fires PRESSING every indev refresh tick whether or not
// the finger moved; without this filter a held finger floods the queue.
// Single shared slot is correct under the v1 single-touch assumption
// (matching every other touch path). i32::MIN sentinel ensures the
// first PRESSING after each fresh press always pushes.
static mut LAST_PRESSING_VIEW: usize = 0;
static mut LAST_PRESSING_X: i32 = i32::MIN;
static mut LAST_PRESSING_Y: i32 = i32::MIN;

/// Read the current monotonic ms from the LVGL tick clock. Used as the
/// timestamp on each touch event; aligns with the same clock GestureDetector
/// uses for fling-velocity duration math.
fn now_ms_for_touch() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

fn push_touch(record: TouchRecord) {
    unsafe {
        let head = TOUCH_QUEUE_HEAD;
        let next = (head + 1) % TOUCH_QUEUE_SIZE;
        if next != TOUCH_QUEUE_TAIL {
            TOUCH_QUEUE[head] = record;
            TOUCH_QUEUE_HEAD = next;
        }
    }
}

unsafe fn touch_event_record(e: *mut lv_event_t, action: i32) -> Option<TouchRecord> {
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
    Some(TouchRecord {
        view_handle: target as usize,
        action,
        x: p.x,
        y: p.y,
        time_ms: now_ms_for_touch(),
    })
}

fn reset_pressing_coalesce() {
    unsafe {
        LAST_PRESSING_VIEW = 0;
        LAST_PRESSING_X = i32::MIN;
        LAST_PRESSING_Y = i32::MIN;
    }
}

unsafe extern "C" fn touch_press_cb(e: *mut lv_event_t) {
    reset_pressing_coalesce();
    if let Some(rec) = unsafe { touch_event_record(e, ACTION_DOWN) } {
        push_touch(rec);
    }
}

unsafe extern "C" fn touch_release_cb(e: *mut lv_event_t) {
    reset_pressing_coalesce();
    if let Some(rec) = unsafe { touch_event_record(e, ACTION_UP) } {
        push_touch(rec);
    }
}

unsafe extern "C" fn touch_long_press_cb(e: *mut lv_event_t) {
    if let Some(rec) = unsafe { touch_event_record(e, ACTION_LONG_PRESS) } {
        push_touch(rec);
    }
}

unsafe extern "C" fn touch_pressing_cb(e: *mut lv_event_t) {
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
    let raw_obj = super::handle_table::lookup(id);
    if raw_obj.is_null() {
        return;
    }
    let raw_ptr = raw_obj as usize;

    unsafe {
        for entry in &mut VIEW_TOUCH_MAP[..VIEW_TOUCH_MAP_LEN] {
            if entry.0 == raw_ptr {
                // Already registered with LVGL — just refresh the obj_ref.
                entry.1 = obj_ref;
                return;
            }
        }
        if VIEW_TOUCH_MAP_LEN < MAX_TOUCH_LISTENERS {
            VIEW_TOUCH_MAP[VIEW_TOUCH_MAP_LEN] = (raw_ptr, obj_ref);
            VIEW_TOUCH_MAP_LEN += 1;
        } else {
            return; // table full, silently drop
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
    }
}

/// Pop one touch event from the queue, if any.
#[cfg_attr(feature = "sim", allow(dead_code))]
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
#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_touch_view_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &VIEW_TOUCH_MAP[..VIEW_TOUCH_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_view_touch_listener_state() {
    unsafe {
        VIEW_TOUCH_MAP_LEN = 0;
        TOUCH_QUEUE_HEAD = 0;
        TOUCH_QUEUE_TAIL = 0;
    }
    reset_pressing_coalesce();
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

#[allow(unused_imports)]
use crate::lvgl_ffi::{
    lv_event_cb_t, lv_obj_add_event_cb, lv_obj_remove_event_cb, lv_screen_active, LV_EVENT_PRESSED,
};

static mut SCREEN_PRESS_HOOK: lv_event_cb_t = None;

/// Attach `cb` to the active screen as an `LV_EVENT_PRESSED` listener.
/// Idempotent — a second call detaches whatever was previously attached
/// before re-attaching, so only one screen hook is ever live.
///
/// Note: we don't short-circuit when the previous and current `cb` are
/// the same fn pointer because Rust's `unpredictable_function_pointer_comparisons`
/// lint correctly warns that fn-pointer equality isn't reliable across
/// codegen units. Detach-then-re-attach is two cheap LVGL list
/// operations and is unconditionally correct.
#[cfg_attr(test, allow(dead_code))]
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
#[cfg_attr(test, allow(dead_code))]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(has_buttons)]
    #[test]
    fn pin_to_keycode_roundtrips_declared_pins() {
        for &(pin, _, keycode) in BUTTONS {
            assert_eq!(pin_to_keycode(pin), Some(keycode));
        }
    }

    #[test]
    fn pin_to_keycode_returns_none_for_unmapped() {
        assert_eq!(pin_to_keycode(99), None);
    }

    #[cfg(has_buttons)]
    #[test]
    fn key_event_queue_roundtrips_in_fifo_order() {
        reset_key_event_queue();
        push_key_event_raw(12, false);
        push_key_event_raw(13, true);
        push_key_event_raw(14, false);

        let a = drain_key_event().unwrap();
        assert_eq!(a.pin, 12);
        assert!(!a.rising);
        let b = drain_key_event().unwrap();
        assert_eq!(b.pin, 13);
        assert!(b.rising);
        let c = drain_key_event().unwrap();
        assert_eq!(c.pin, 14);
        assert!(!c.rising);
        assert!(drain_key_event().is_none());
    }

    #[cfg(has_buttons)]
    #[test]
    fn key_event_queue_wraps_around() {
        reset_key_event_queue();
        for cycle in 0..4 {
            for i in 0..KEY_EVENT_QUEUE_SIZE - 1 {
                push_key_event_raw(i as u8, cycle % 2 == 0);
            }
            for i in 0..KEY_EVENT_QUEUE_SIZE - 1 {
                let e = drain_key_event().unwrap();
                assert_eq!(e.pin, i as u8);
            }
            assert!(drain_key_event().is_none());
        }
    }
}
