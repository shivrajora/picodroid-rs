// SPDX-License-Identifier: GPL-3.0-only
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

/// Visit the Java `View` object ref of every view registered for a key, touch,
/// or swipe callback so the GC keeps it alive. Such a View is referenced only
/// by these native maps (raw `lv_obj_t*` -> Java obj_ref), not the Java heap,
/// unless the app also keeps a field for it — so a focused/registered content
/// root the app didn't field would otherwise be swept by the first GC, after
/// which dispatch resolves the live `lv_obj` to a dead/reused ref and input
/// silently drops (the keypad appears to "lose focus" a few seconds in).
/// Called from `PicodroidNativeHandler::gc_visit_roots`.
pub fn visit_view_listener_roots(visit: &mut dyn FnMut(u16)) {
    unsafe {
        for &(_, r) in &VIEW_KEY_MAP[..] {
            if r != 0 {
                visit(r);
            }
        }
        for &(_, r) in &VIEW_TOUCH_MAP[..] {
            if r != 0 {
                visit(r);
            }
        }
        for &(_, r) in &VIEW_SWIPE_MAP[..] {
            if r != 0 {
                visit(r);
            }
        }
        for &(_, r) in &VIEW_FOCUS_MAP[..] {
            if r != 0 {
                visit(r);
            }
        }
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
        KEYPAD_INDEV = keypad;
        // No default group yet: each Activity owns its own keypad focus group,
        // created by `push_activity_group()` as the Activity is launched (see
        // the "Per-Activity keypad focus groups" section). Until the first
        // Activity pushes a group, the keypad has nothing to navigate.
    }

    init_button_pins();
}

// ── Per-Activity keypad focus groups ────────────────────────────────────────
//
// Android gives every Activity its own Window with an isolated focus scope: a
// backgrounded Activity's focus is retained untouched and restored on resume,
// and one Activity can never traverse into another's focus. We mirror that with
// one `lv_group` per Activity. While an Activity is on top its group is both
// the LVGL *default* group (so the Activity's group-def widgets — Button,
// EditText, … — auto-join IT, not a shared global) and the keypad indev's group
// (so PREV/NEXT navigation stays within it). Push creates the child's group;
// pop deletes it and reactivates the parent's, whose focus state is intact —
// no cross-Activity focus bleed, and resume-focus needs no special handling.
//
// The group stack is kept in lockstep with the framework Activity stack by
// `push_activity_group`/`pop_activity_group` calls from `lifecycle.rs` at the
// same points it pushes/pops Activities.

/// Upper bound on nested Activities, matching the documented range of the
/// `[jvm] activity_stack_depth` tunable (1..=32). The framework Activity stack
/// caps depth first, so this group stack never overflows.
#[cfg(has_buttons)]
const MAX_ACTIVITY_GROUPS: usize = 32;

#[cfg(has_buttons)]
static mut KEYPAD_INDEV: *mut lv_indev_t = core::ptr::null_mut();

#[cfg(has_buttons)]
static mut ACTIVITY_GROUPS: [*mut lv_group_t; MAX_ACTIVITY_GROUPS] =
    [core::ptr::null_mut(); MAX_ACTIVITY_GROUPS];

#[cfg(has_buttons)]
static mut ACTIVITY_GROUP_DEPTH: usize = 0;

/// Create a fresh focus group for a newly-launched Activity and make it the
/// active group (LVGL default + keypad indev). Called from the lifecycle
/// bootstrap/push paths *before* the Activity's `onCreate`, so its group-def
/// widgets auto-join this group.
#[cfg(has_buttons)]
pub fn push_activity_group() {
    unsafe {
        if ACTIVITY_GROUP_DEPTH >= MAX_ACTIVITY_GROUPS {
            return; // unreachable: the Activity stack caps depth first
        }
        let group = lv_group_create();
        lv_group_set_default(group);
        if !KEYPAD_INDEV.is_null() {
            lv_indev_set_group(KEYPAD_INDEV, group);
        }
        ACTIVITY_GROUPS[ACTIVITY_GROUP_DEPTH] = group;
        ACTIVITY_GROUP_DEPTH += 1;
    }
}

/// Tear down the top Activity's focus group and reactivate the parent's (or
/// none if the stack is now empty). Called from the lifecycle pop path *after*
/// the popped Activity's view tree is deleted. The parent group is reattached
/// to the indev before the child group is freed, so the indev never references
/// a deleted group.
#[cfg(has_buttons)]
pub fn pop_activity_group() {
    unsafe {
        if ACTIVITY_GROUP_DEPTH == 0 {
            return;
        }
        ACTIVITY_GROUP_DEPTH -= 1;
        let group = ACTIVITY_GROUPS[ACTIVITY_GROUP_DEPTH];
        ACTIVITY_GROUPS[ACTIVITY_GROUP_DEPTH] = core::ptr::null_mut();

        let parent = if ACTIVITY_GROUP_DEPTH > 0 {
            ACTIVITY_GROUPS[ACTIVITY_GROUP_DEPTH - 1]
        } else {
            core::ptr::null_mut()
        };
        lv_group_set_default(parent);
        if !KEYPAD_INDEV.is_null() {
            lv_indev_set_group(KEYPAD_INDEV, parent);
        }
        if !group.is_null() {
            lv_group_delete(group);
        }
    }
}

/// Delete every remaining Activity focus group and reset the stack. Called from
/// the between-app-run reset path so a fresh app starts with a clean keypad.
#[cfg(has_buttons)]
pub fn reset_activity_groups() {
    unsafe {
        // Slice idiom (matching `&mut VIEW_KEY_MAP[..]` elsewhere in this file)
        // rather than an index range — keeps clippy's needless_range_loop quiet
        // without taking a `&mut` to the whole static (static_mut_refs).
        let had_groups = ACTIVITY_GROUP_DEPTH > 0;
        for slot in &mut ACTIVITY_GROUPS[..ACTIVITY_GROUP_DEPTH] {
            let g = *slot;
            *slot = core::ptr::null_mut();
            if !g.is_null() {
                lv_group_delete(g);
            }
        }
        ACTIVITY_GROUP_DEPTH = 0;
        // Only touch LVGL when groups actually existed. This runs at app start
        // before `init_keypad`, so on the very first run LVGL isn't initialized
        // yet and KEYPAD_INDEV is null; a non-zero depth implies a prior run on
        // the persistent graphics singleton, where these pointers are live.
        if had_groups {
            lv_group_set_default(core::ptr::null_mut());
            if !KEYPAD_INDEV.is_null() {
                lv_indev_set_group(KEYPAD_INDEV, core::ptr::null_mut());
            }
        }
    }
}

// No-button boards have no keypad indev — the group machinery is inert, but the
// lifecycle still calls these so they exist as no-ops.
#[cfg(not(has_buttons))]
pub fn push_activity_group() {}
#[cfg(not(has_buttons))]
pub fn pop_activity_group() {}
#[cfg(not(has_buttons))]
pub fn reset_activity_groups() {}

/// `View.setFocusable(boolean)` backing: add this view to — or remove it from —
/// the active Activity's keypad focus group. No-op when the handle is null or no
/// group is active (non-button boards, or before the first Activity launches).
pub fn set_view_focusable(id: i32, on: bool) {
    let raw = super::handle_table::lookup(id);
    if raw.is_null() {
        return;
    }
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return;
        }
        if on {
            lv_group_add_obj(group, raw); // idempotent if already a member
        } else {
            lv_group_remove_obj(raw);
        }
    }
}

/// `View.requestFocus()` backing: ensure the view is in the active group and
/// make it the focused widget. Returns whether it actually became focused —
/// false when the handle is null or there is no active group, matching
/// Android's "a view that can't take focus returns false".
pub fn request_view_focus(id: i32) -> bool {
    let raw = super::handle_table::lookup(id);
    if raw.is_null() {
        return false;
    }
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return false;
        }
        lv_group_add_obj(group, raw); // idempotent if already a member
        lv_group_focus_obj(raw);
        lv_group_get_focused(group) == raw
    }
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

/// Press-state filter — drops the phantom rising-edge IRQs that fire at boot
/// when `enable_edge_irq` arms the GPIO peripheral on pins that were in an
/// indeterminate state during init (observed on Pico Enviro+ Pack: every
/// button GP12-15 fires a phantom release within the first 50 ms, which
/// dispatched BACK and finished the activity before sensors could deliver a
/// second event). Pure logic lives in `super::key_filter`.
#[cfg(has_buttons)]
static mut KEY_PRESS_FILTER: super::key_filter::KeyPressFilter =
    super::key_filter::KeyPressFilter::new();

#[cfg(has_buttons)]
fn push_key_event_raw(pin: u8, rising: bool) {
    unsafe {
        // `&raw mut` then deref: forming `&mut KEY_PRESS_FILTER` directly trips
        // the `static_mut_refs` lint (Rust 2024 compat). See handle_table.rs /
        // socket_table.rs for the same idiom.
        let filter = &raw mut KEY_PRESS_FILTER;
        if !(*filter).observe(pin, rising) {
            return;
        }
        let head = KEY_EVENT_QUEUE_HEAD;
        let next = (head + 1) % KEY_EVENT_QUEUE_SIZE;
        if next != KEY_EVENT_QUEUE_TAIL {
            KEY_EVENT_QUEUE[head] = KeyEventRaw { pin, rising };
            KEY_EVENT_QUEUE_HEAD = next;
        }
    }
}

// ── Keypad edit mode (NumberPicker stepping) ────────────────────────────────
//
// One EditMode instance filters every key edge *before* it fans out to the
// LVGL indev and the Java queue below — the single interception point that
// keeps both paths consistent (see edit_mode.rs for the protocol).

#[cfg(has_buttons)]
static mut EDIT_MODE: super::edit_mode::EditMode = super::edit_mode::EditMode::new();

/// Mirror an edit-mode transition onto the widget: LV_STATE_EDITED drives the
/// theme-matching secondary outline. The carried pointer is the currently
/// focused widget, so it is live.
#[cfg(has_buttons)]
fn apply_edit_transition(t: super::edit_mode::Transition) {
    use super::edit_mode::Transition;
    match t {
        Transition::None => {}
        Transition::Entered(obj) => unsafe {
            lv_obj_add_state(obj as *mut lv_obj_t, LV_STATE_EDITED);
        },
        Transition::Exited(obj) => unsafe {
            lv_obj_remove_state(obj as *mut lv_obj_t, LV_STATE_EDITED);
        },
    }
}

/// Called from the NumberPicker DEFOCUSED/DELETE trampolines: abandon keypad
/// edit mode if `raw_obj` is the widget being edited. The trampoline clears
/// LV_STATE_EDITED itself (the object is live there); this only drops the
/// filter state so PREV/NEXT go back to navigating.
#[cfg(has_buttons)]
pub fn notify_picker_gone(raw_obj: usize) {
    unsafe {
        let em = &raw mut EDIT_MODE;
        (*em).notify_gone(raw_obj);
    }
}

#[cfg(not(has_buttons))]
pub fn notify_picker_gone(_raw_obj: usize) {}

/// Clear edit-mode state between app runs. Called from the `app.rs` reset
/// block alongside the other widget-state resets.
#[cfg(has_buttons)]
pub fn reset_edit_mode() {
    unsafe {
        let em = &raw mut EDIT_MODE;
        *em = super::edit_mode::EditMode::new();
    }
}

#[cfg(not(has_buttons))]
pub fn reset_edit_mode() {}

/// The active group's focused widget as a raw pointer (0 if none), plus
/// whether it is a registered NumberPicker — the edit-mode filter's inputs.
#[cfg(has_buttons)]
fn focused_obj_for_edit_mode() -> (usize, bool) {
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return (0, false);
        }
        let focused = lv_group_get_focused(group) as usize;
        (
            focused,
            super::widgets::number_picker::is_number_picker(focused),
        )
    }
}

#[cfg(has_buttons)]
unsafe extern "C" fn keypad_read_cb(_indev: *mut lv_indev_t, data: *mut lv_indev_data_t) {
    let d = unsafe { &mut *data };
    if let Some(event) = hal::gpio::drain_gpio_event() {
        let key = BUTTONS
            .iter()
            .find(|&&(p, _, _)| p == event.pin)
            .map(|&(_, k, _)| k);

        // Run mapped keys through the edit-mode filter; unmapped pins keep
        // the historical behavior (Java queue only, nothing for the indev).
        let decision = match key {
            Some(k) => {
                let (focused, is_picker) = focused_obj_for_edit_mode();
                let (decision, transition) = unsafe {
                    let em = &raw mut EDIT_MODE;
                    (*em).filter(k, !event.rising, focused, is_picker)
                };
                apply_edit_transition(transition);
                decision
            }
            None => super::edit_mode::Decision {
                lvgl_key: None,
                forward_java: true,
                step: None,
            },
        };

        if decision.forward_java {
            push_key_event_raw(event.pin, event.rising);
        }
        if let Some((obj, direction)) = decision.step {
            super::widgets::number_picker::push_step(obj, direction);
        }
        if let Some(k) = decision.lvgl_key {
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

// ── Swipe-listener registry ─────────────────────────────────────────────────
//
// Mirrors the touch-listener pattern: a `(handle, obj_ref)` map keyed by raw
// `lv_obj_t*` plus a small ring buffer of `(handle, lv_dir_t)` records. The
// trampoline reads `lv_indev_active` + `lv_indev_get_gesture_dir` to capture
// the direction, then pushes onto the queue for the framework loop to drain.

const MAX_SWIPE_LISTENERS: usize = 32;

#[derive(Copy, Clone)]
pub struct SwipeRecord {
    pub view_handle: usize,
    pub direction: i32,
}

const SWIPE_QUEUE_SIZE: usize = 16;
static mut SWIPE_QUEUE: [SwipeRecord; SWIPE_QUEUE_SIZE] = [SwipeRecord {
    view_handle: 0,
    direction: 0,
}; SWIPE_QUEUE_SIZE];
static mut SWIPE_QUEUE_HEAD: usize = 0;
static mut SWIPE_QUEUE_TAIL: usize = 0;

static mut VIEW_SWIPE_MAP: [(usize, u16); MAX_SWIPE_LISTENERS] = [(0, 0); MAX_SWIPE_LISTENERS];
static mut VIEW_SWIPE_MAP_LEN: usize = 0;

unsafe extern "C" fn swipe_gesture_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        let indev = lv_indev_active();
        if indev.is_null() {
            return;
        }
        let dir = lv_indev_get_gesture_dir(indev);
        if dir == LV_DIR_NONE {
            return;
        }
        let head = SWIPE_QUEUE_HEAD;
        let next = (head + 1) % SWIPE_QUEUE_SIZE;
        if next != SWIPE_QUEUE_TAIL {
            SWIPE_QUEUE[head] = SwipeRecord {
                view_handle: obj,
                direction: dir as i32,
            };
            SWIPE_QUEUE_HEAD = next;
        }
    }
}

pub fn register_view_swipe_listener(id: i32, obj_ref: u16) {
    let raw_obj = super::handle_table::lookup(id);
    if raw_obj.is_null() {
        return;
    }
    let raw_ptr = raw_obj as usize;
    unsafe {
        for entry in &mut VIEW_SWIPE_MAP[..VIEW_SWIPE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if VIEW_SWIPE_MAP_LEN < MAX_SWIPE_LISTENERS {
            VIEW_SWIPE_MAP[VIEW_SWIPE_MAP_LEN] = (raw_ptr, obj_ref);
            VIEW_SWIPE_MAP_LEN += 1;
        } else {
            return;
        }
        lv_obj_add_event_cb(
            raw_obj,
            Some(swipe_gesture_cb),
            LV_EVENT_GESTURE,
            core::ptr::null_mut(),
        );
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_swipe_event() -> Option<SwipeRecord> {
    unsafe {
        if SWIPE_QUEUE_TAIL == SWIPE_QUEUE_HEAD {
            return None;
        }
        let r = SWIPE_QUEUE[SWIPE_QUEUE_TAIL];
        SWIPE_QUEUE_TAIL = (SWIPE_QUEUE_TAIL + 1) % SWIPE_QUEUE_SIZE;
        Some(r)
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_swipe_view_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &VIEW_SWIPE_MAP[..VIEW_SWIPE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_view_swipe_listener_state() {
    unsafe {
        VIEW_SWIPE_MAP_LEN = 0;
        SWIPE_QUEUE_HEAD = 0;
        SWIPE_QUEUE_TAIL = 0;
    }
}

// ── View focus-change listener registry ─────────────────────────────────────
//
// Backs `android.view.View.OnFocusChangeListener`. Mirrors the swipe pattern:
// a `(handle, obj_ref)` map keyed by raw `lv_obj_t*` plus a ring buffer of
// `(handle, has_focus)` records fed by `LV_EVENT_FOCUSED`/`LV_EVENT_DEFOCUSED`
// trampolines. The framework loop drains the queue and invokes
// `View.fireFocusChange(boolean)` on the matching object. A view only emits
// these once it is a member of the active Activity's keypad focus group
// (setFocusable/requestFocus or an adapter row), which is exactly when Android
// would deliver focus callbacks.

const MAX_FOCUS_LISTENERS: usize = 32;

#[derive(Copy, Clone)]
pub struct FocusRecord {
    pub view_handle: usize,
    pub has_focus: bool,
}

const FOCUS_QUEUE_SIZE: usize = 16;
static mut FOCUS_QUEUE: [FocusRecord; FOCUS_QUEUE_SIZE] = [FocusRecord {
    view_handle: 0,
    has_focus: false,
}; FOCUS_QUEUE_SIZE];
static mut FOCUS_QUEUE_HEAD: usize = 0;
static mut FOCUS_QUEUE_TAIL: usize = 0;

static mut VIEW_FOCUS_MAP: [(usize, u16); MAX_FOCUS_LISTENERS] = [(0, 0); MAX_FOCUS_LISTENERS];
static mut VIEW_FOCUS_MAP_LEN: usize = 0;

fn push_focus_event(handle: usize, has_focus: bool) {
    unsafe {
        let head = FOCUS_QUEUE_HEAD;
        let next = (head + 1) % FOCUS_QUEUE_SIZE;
        if next != FOCUS_QUEUE_TAIL {
            FOCUS_QUEUE[head] = FocusRecord {
                view_handle: handle,
                has_focus,
            };
            FOCUS_QUEUE_HEAD = next;
        }
    }
}

unsafe extern "C" fn view_focused_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    push_focus_event(obj, true);
}

unsafe extern "C" fn view_defocused_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    push_focus_event(obj, false);
}

/// `View.nativeIsFocused()` backing — whether this view is the active keypad
/// group's focused widget. Mirrors `android.view.View#isFocused`. Cheap:
/// reuses the `lv_group_get_focused(group) == raw` check from
/// [`request_view_focus`].
pub fn view_is_focused(id: i32) -> bool {
    let raw = super::handle_table::lookup(id);
    if raw.is_null() {
        return false;
    }
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return false;
        }
        lv_group_get_focused(group) == raw
    }
}

/// `View.setOnFocusChangeListener` backing: record the Java `View` as the
/// focus-change target and attach the LVGL FOCUSED/DEFOCUSED trampolines.
/// Idempotent — re-registration just refreshes the obj_ref slot.
pub fn register_view_focus_change_listener(id: i32, obj_ref: u16) {
    let raw_obj = super::handle_table::lookup(id);
    if raw_obj.is_null() {
        return;
    }
    let raw_ptr = raw_obj as usize;
    unsafe {
        for entry in &mut VIEW_FOCUS_MAP[..VIEW_FOCUS_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if VIEW_FOCUS_MAP_LEN < MAX_FOCUS_LISTENERS {
            VIEW_FOCUS_MAP[VIEW_FOCUS_MAP_LEN] = (raw_ptr, obj_ref);
            VIEW_FOCUS_MAP_LEN += 1;
        } else {
            return;
        }
        lv_obj_add_event_cb(
            raw_obj,
            Some(view_focused_cb),
            LV_EVENT_FOCUSED,
            core::ptr::null_mut(),
        );
        lv_obj_add_event_cb(
            raw_obj,
            Some(view_defocused_cb),
            LV_EVENT_DEFOCUSED,
            core::ptr::null_mut(),
        );
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_focus_change_event() -> Option<FocusRecord> {
    unsafe {
        if FOCUS_QUEUE_TAIL == FOCUS_QUEUE_HEAD {
            return None;
        }
        let r = FOCUS_QUEUE[FOCUS_QUEUE_TAIL];
        FOCUS_QUEUE_TAIL = (FOCUS_QUEUE_TAIL + 1) % FOCUS_QUEUE_SIZE;
        Some(r)
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_focus_view_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &VIEW_FOCUS_MAP[..VIEW_FOCUS_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_view_focus_listener_state() {
    unsafe {
        VIEW_FOCUS_MAP_LEN = 0;
        FOCUS_QUEUE_HEAD = 0;
        FOCUS_QUEUE_TAIL = 0;
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

    /// Handle `0` maps to a null `lv_obj` on both the 32-bit and 64-bit handle
    /// tables, so the focus helpers must short-circuit on it without touching
    /// LVGL group state. Guards the null-handle path that protects sim builds
    /// and any pre-launch caller.
    #[test]
    fn focus_helpers_short_circuit_on_null_handle() {
        set_view_focusable(0, true);
        set_view_focusable(0, false);
        assert!(
            !request_view_focus(0),
            "requestFocus on a null handle must report no focus"
        );
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
