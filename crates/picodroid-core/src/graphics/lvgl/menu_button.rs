// SPDX-License-Identifier: GPL-3.0-only
//! The on-screen menu control for a touch board
//! (docs/designs/app-portability-2026-10.md K9). A small round control in
//! the bottom-right corner of the window, on LVGL's top layer, shown while
//! the resumed Activity has an options menu: a tap is MENU, through the
//! soft-key queue (`input_inject::push_soft_key_press`), so the framework
//! routes it exactly as it routes a MENU key with a pin behind it — the
//! Activity's `onKeyUp` opens the menu. A four-key board opens the menu by
//! holding SELECT instead and draws nothing.
//!
//! The twin of `soft_nav.rs` (bottom-left, BACK/HOME): the same size and
//! look, hidden with it while the system keyboard is up, raised with it over
//! a dialog's scrim.
//!
//! Module-level state is widget-layer state reached only from JVM tasks, as
//! for every other overlay here.

use super::lifecycle;
use crate::lvgl_ffi::*;
use crate::util::local::Core0;
use core::cell::Cell;

/// The control's diameter and its inset from the window's corner — the
/// soft-nav control's, mirrored.
const SIZE: i32 = 34;
const INSET: i32 = 6;
/// The press area grows this far beyond the circle.
const EXT_CLICK: i32 = 8;
/// `LV_SYMBOL_BARS`: the three bars every phone user reads as "menu", from
/// the symbol range the 14 px face carries.
const GLYPH: &core::ffi::CStr = c"\u{F0C9}";

struct State {
    /// The control, once made. Kept as an `Obj` across natives because
    /// nothing deletes it: it lives on the top layer, which nothing cleans.
    control: Cell<Option<Obj>>,
    /// Whether the resumed Activity has a menu to open.
    available: Cell<bool>,
    /// Hidden by the system keyboard, whatever the Activity has.
    covered: Cell<bool>,
}

// SAFETY: created by LVGL init, toggled by the lifecycle and keyboard
// natives and the control's click callback, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        control: Cell::new(None),
        available: Cell::new(false),
        covered: Cell::new(false),
    })
};

/// Create the control once LVGL is up, hidden until an Activity has a menu.
/// Idempotent.
pub fn ensure() {
    if STATE.control.get().is_some() {
        return;
    }
    let Some(btn) = lifecycle::overlay_layer().and_then(|o| lv_button_create(o)) else {
        return;
    };
    {
        lv_obj_set_size(btn, SIZE, SIZE);
        lv_obj_align(btn, LV_ALIGN_BOTTOM_RIGHT, -INSET, -INSET);
        lv_obj_set_style_radius(btn, LV_RADIUS_CIRCLE, 0);
        lv_obj_set_style_bg_color(btn, lv_color_hex(0x202020), 0);
        lv_obj_set_style_bg_opa(btn, 150, 0);
        lv_obj_set_style_border_width(btn, 1, 0);
        lv_obj_set_style_border_color(btn, lv_color_hex(0x808080), 0);
        lv_obj_set_style_pad_left(btn, 0, 0);
        lv_obj_set_style_pad_right(btn, 0, 0);
        lv_obj_set_style_pad_top(btn, 0, 0);
        lv_obj_set_style_pad_bottom(btn, 0, 0);
        lv_obj_set_ext_click_area(btn, EXT_CLICK);
        // Out of the keypad focus ring, like the soft-nav control.
        lv_obj_remove_flag(btn, LV_OBJ_FLAG_CLICK_FOCUSABLE);
        lv_obj_add_flag(btn, LV_OBJ_FLAG_HIDDEN);
        if let Some(label) = lv_label_create(btn) {
            // SAFETY: `label` was just made by `lv_label_create`; the glyph
            // is a NUL-terminated literal LVGL copies.
            unsafe { lv_label_set_text(label, GLYPH.as_ptr()) };
            lv_obj_set_style_text_color(label, lv_color_hex(0xE0E0E0), 0);
            lv_obj_center(label);
        }
        add_event_cb(btn, clicked_cb, LV_EVENT_CLICKED);
        STATE.control.set(Some(btn));
    }
    apply();
}

/// What the resumed Activity reported: `Activity.nativeSetOptionsMenuAvailable`.
pub fn set_available(available: bool) {
    STATE.available.set(available);
    apply();
}

/// Hide or show the control: hidden while the system keyboard is up.
pub fn set_hidden(hidden: bool) {
    STATE.covered.set(hidden);
    apply();
}

/// Bring the control above whatever was added to the top layer since — a
/// dialog's scrim — so the menu stays reachable while a dialog shows.
pub fn raise() {
    let Some(btn) = STATE.control.get() else {
        return;
    };
    lv_obj_move_to_index(btn, -1);
}

/// Between apps: the next app's first Activity says whether it has a menu.
pub fn reset() {
    set_available(false);
}

fn apply() {
    let Some(btn) = STATE.control.get() else {
        return;
    };
    if STATE.available.get() && !STATE.covered.get() {
        lv_obj_remove_flag(btn, LV_OBJ_FLAG_HIDDEN);
    } else {
        lv_obj_add_flag(btn, LV_OBJ_FLAG_HIDDEN);
    }
}

lv_event_cb! {
    fn clicked_cb(_e) {
        crate::pd_info!("soft menu: tap -> MENU");
        crate::input_inject::push_soft_key_press(crate::input_inject::KEYCODE_MENU);
    }
}
