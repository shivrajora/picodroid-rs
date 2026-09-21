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
use crate::lvgl_ffi::*;

use super::listener_map::{map_mut, map_ref, warn_full, PtrMap, Upsert};

// Board-specific button table generated from `[[button]]` in board.toml.
// Entries: (pin, LV_KEY_*, android_keycode). Empty on boards without buttons.
mod button_generated {
    #[cfg_attr(not(has_buttons), allow(unused_imports))]
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/button_config.rs"));
}
use button_generated::BUTTONS;

mod focus;
mod groups;
mod keypad;
mod swipe;
mod touch;

// One flat `events::*` surface, as when this was a single file: callers in
// `lifecycle`, `graphics::view` and the widgets name no child module.
pub use self::{focus::*, groups::*, keypad::*, swipe::*, touch::*};

#[cfg(test)]
mod tests;

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

/// Reverse of [`pin_to_keycode`]: find the GPIO pin of the first button
/// declared with `keycode`. `None` if this board has no button for that
/// keycode. Used by the PDB `CMD_INPUT` handler to resolve a host-sent Android
/// keycode (`pdb input keyevent …`) to a pin for `hal::gpio::inject`.
pub fn keycode_to_pin(keycode: i32) -> Option<u8> {
    crate::board_cfg::buttons::keycode_to_pin(keycode)
}

/// Pop one key event from the Java-visible queue, if any.
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
static mut VIEW_KEY_MAP: PtrMap<MAX_KEY_LISTENERS> = PtrMap::new();

unsafe extern "C" fn key_map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe { map_mut(&raw mut VIEW_KEY_MAP).remove(obj) }
}

/// Record a Java `View` object as the key-listener target for the given
/// `nativeHandle` id. The registry keys on the raw `lv_obj_t*` from the
/// handle table because LVGL's focus group also exposes raw pointers.
/// No key trampoline exists (the shared keypad indev feeds this map), but
/// first registration still attaches the `LV_EVENT_DELETE` unregistration
/// hook so a destroyed view's entry doesn't pin its Java graph.
pub fn register_view_key_listener(id: i32, obj_ref: u16) {
    let raw_obj = super::handle_table::lookup(id);
    if raw_obj.is_null() {
        return; // deleted/stale view: never hand LVGL a null, never map key 0
    }
    let raw_ptr = raw_obj as usize;
    unsafe {
        match map_mut(&raw mut VIEW_KEY_MAP).upsert(raw_ptr, obj_ref) {
            Upsert::Updated => {}
            Upsert::Full => warn_full("view-key"),
            Upsert::Inserted => {
                lv_obj_add_event_cb(
                    raw_ptr as *mut lv_obj_t,
                    Some(key_map_delete_cb),
                    LV_EVENT_DELETE,
                    core::ptr::null_mut(),
                );
            }
        }
    }
}

fn lookup_view_obj(handle: usize) -> Option<u16> {
    unsafe { map_ref(&raw const VIEW_KEY_MAP).lookup(handle) }
}

pub fn reset_view_key_listener_state() {
    unsafe { map_mut(&raw mut VIEW_KEY_MAP).reset() }
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
        map_ref(&raw const VIEW_KEY_MAP).visit(visit);
        map_ref(&raw const VIEW_TOUCH_MAP).visit(visit);
        map_ref(&raw const VIEW_SWIPE_MAP).visit(visit);
        map_ref(&raw const VIEW_FOCUS_MAP).visit(visit);
    }
}

/// Initialize the LVGL keypad indev, focus group, and hardware button GPIO
/// pins. Called from `LvglGfx::init` after [`lifecycle::init`] has run.
/// No-op on boards without `[[button]]` entries in board.toml.
pub(in crate::graphics) fn init_keypad() {
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
