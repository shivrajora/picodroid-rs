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
    /// The edge's ISR stamp (`GpioEvent::t_us`), so the hold timer behind
    /// auto-repeat runs from when the button moved, not from when the UI
    /// task got round to the queue.
    pub t_us: u32,
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
    KEYPAD.queue.pop()
}

/// Clear the key event queue between app runs.
pub fn reset_key_event_queue() {
    KEYPAD.queue.clear();
}

/// Forget a key held across an app reset (Stage 10): the next app's first
/// quiet read pass must not see a phantom press.
pub fn reset_held_keys() {
    keypad::reset_held();
}

/// The keypad-focused widget's handle and the Java `View` bound to it
/// (`handle_table::bind`, which every listener registration makes), if
/// any. No key trampoline exists — the shared keypad indev feeds the key
/// queue — so whether that view wants keys is its `onKeyListener` field's
/// business: `lifecycle::input` reads it before firing `fireKey`.
pub fn focused_bound_view() -> Option<(i32, u16)> {
    let focused = lv_group_get_default().and_then(|o| lv_group_get_focused(o))?;
    let handle = super::handle_table::handle_of(focused);
    Some((handle, super::handle_table::bound_java(handle)?))
}

/// Initialize the LVGL keypad indev, focus group, and hardware button GPIO
/// pins. Called from `LvglGfx::init` after [`lifecycle::init`] has run.
/// No-op on boards without `[[button]]` entries in board.toml.
pub(in crate::graphics) fn init_keypad() {
    #[cfg(has_buttons)]
    // SAFETY: called from `LvglGfx::init` after `lifecycle::init` has run
    // `lv_init`.
    if let Some(keypad) = unsafe { lv_indev_create() } {
        lv_indev_set_type(keypad, LV_INDEV_TYPE_KEYPAD);
        lv_indev_set_read_cb(keypad, Some(keypad_read_cb));
        GROUPS.keypad_indev.set(Some(keypad));
        // No default group yet: each Activity owns its own keypad focus group,
        // created by `push_activity_group()` as the Activity is launched (see
        // the "Per-Activity keypad focus groups" section). Until the first
        // Activity pushes a group, the keypad has nothing to navigate.
    }

    init_button_pins();
}
