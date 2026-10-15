// SPDX-License-Identifier: GPL-3.0-only
//! The on-screen BACK/HOME control for a touch board with no system button
//! (`soft_nav = true` in board.toml; docs/designs/app-portability-2026-10.md
//! K3). A small round control in the bottom-left corner of the window, on
//! LVGL's top layer so it rides above every screen: a tap is BACK, a hold of
//! `HOME_HOLD_MS` is HOME — the same two gestures a physical system button
//! gives (K2). Both go through the soft-key queue
//! (`input_inject::push_soft_key`), so the framework routes them exactly as
//! it routes a key with a pin behind it.
//!
//! Android's answer to a device without hardware keys is a software
//! navigation bar that takes a strip of the screen; this is the one-button
//! version of it, because a 240 px panel cannot spare a strip. The control
//! hides while the system keyboard is up (the keyboard has its own dismiss
//! affordances and its bottom-left key sits where the control does) and is
//! raised above a dialog's scrim when one shows, so a cancelable dialog can
//! always be backed out of.
//!
//! Module-level state is widget-layer state reached only from JVM tasks, as
//! for every other overlay here.

use super::lifecycle;
use crate::board_cfg::input::HOME_HOLD_MS;
use crate::lvgl_ffi::*;
use crate::util::local::Core0;
use core::cell::Cell;

/// The control's diameter and its inset from the window's corner.
const SIZE: i32 = 34;
const INSET: i32 = 6;
/// The press area grows this far beyond the circle: a corner target, not a
/// precision one.
const EXT_CLICK: i32 = 8;

/// `LV_SYMBOL_LEFT`: a chevron pointing back, from the symbol range the
/// 14 px face carries.
const GLYPH: &core::ffi::CStr = c"\u{F053}";

struct State {
    /// The control, once made. Kept as an `Obj` across natives because
    /// nothing deletes it: it lives on the top layer, which nothing cleans.
    control: Cell<Option<Obj>>,
    pressed_at_ms: Cell<i64>,
    home_fired: Cell<bool>,
}

// SAFETY: created by LVGL init and driven by the control's own press
// callbacks, which LVGL runs from the UI task's timer handler: core-0 JVM
// tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        control: Cell::new(None),
        pressed_at_ms: Cell::new(0),
        home_fired: Cell::new(false),
    })
};

/// Create the control once LVGL is up. Idempotent.
pub fn ensure() {
    if STATE.control.get().is_some() {
        return;
    }
    let Some(btn) = lifecycle::overlay_layer().and_then(|o| lv_button_create(o)) else {
        return;
    };
    {
        lv_obj_set_size(btn, SIZE, SIZE);
        lv_obj_align(btn, LV_ALIGN_BOTTOM_LEFT, INSET, -INSET);
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
        // Out of the keypad focus ring: a soft-nav board has no nav keys, and
        // a future one with both must not land ENTER on the control.
        lv_obj_remove_flag(btn, LV_OBJ_FLAG_CLICK_FOCUSABLE);
        if let Some(label) = lv_label_create(btn) {
            // SAFETY: `label` was just made by `lv_label_create`; the glyph
            // is a NUL-terminated literal LVGL copies.
            unsafe { lv_label_set_text(label, GLYPH.as_ptr()) };
            lv_obj_set_style_text_color(label, lv_color_hex(0xE0E0E0), 0);
            lv_obj_center(label);
        }
        add_event_cb(btn, pressed_cb, LV_EVENT_PRESSED);
        add_event_cb(btn, pressing_cb, LV_EVENT_LONG_PRESSED_REPEAT);
        add_event_cb(btn, released_cb, LV_EVENT_RELEASED);
        STATE.control.set(Some(btn));
    }
}

/// Hide or show the control: hidden while the system keyboard is up.
pub fn set_hidden(hidden: bool) {
    let Some(btn) = STATE.control.get() else {
        return;
    };
    if hidden {
        lv_obj_add_flag(btn, LV_OBJ_FLAG_HIDDEN);
    } else {
        lv_obj_remove_flag(btn, LV_OBJ_FLAG_HIDDEN);
    }
}

/// Bring the control above whatever was added to the top layer since — a
/// dialog's scrim — so BACK stays reachable while the dialog shows.
pub fn raise() {
    let Some(btn) = STATE.control.get() else {
        return;
    };
    lv_obj_move_to_index(btn, -1);
}

fn now_ms() -> i64 {
    crate::hal::system_clock::elapsed_realtime_nanos() / 1_000_000
}

lv_event_cb! {
    fn pressed_cb(_e) {
        STATE.pressed_at_ms.set(now_ms());
        STATE.home_fired.set(false);
    }
}

lv_event_cb! {
    /// LVGL repeats this every `long_press_repeat_time` while the control is
    /// held, from `long_press_time` on: the clock for the hold-for-HOME.
    fn pressing_cb(_e) {
        if STATE.home_fired.get() {
            return;
        }
        if now_ms() - STATE.pressed_at_ms.get() >= i64::from(HOME_HOLD_MS) {
            STATE.home_fired.set(true);
            crate::pd_info!("soft nav: held -> HOME");
            crate::input_inject::push_soft_key_press(crate::input_inject::KEYCODE_HOME);
        }
    }
}

lv_event_cb! {
    fn released_cb(_e) {
        if STATE.home_fired.get() {
            STATE.home_fired.set(false);
            return;
        }
        crate::pd_info!("soft nav: tap -> BACK");
        crate::input_inject::push_soft_key_press(crate::input_inject::KEYCODE_BACK);
    }
}
