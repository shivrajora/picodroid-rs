// SPDX-License-Identifier: GPL-3.0-only
//! Cross-widget view operations on `LvglGfx`.
//!
//! Every widget hits this surface (set_pos, set_size, set_bg_color, …)
//! regardless of widget kind. Implementation is one indirection: resolve
//! the [`Handle`] to a `*mut lv_obj_t` via `handle_table::lookup`, then
//! issue the LVGL call.
//!
//! These functions are called from `impl Gfx for LvglGfx` in
//! [`super::mod`]. Free functions rather than inherent methods so the
//! trait impl block stays the canonical readable summary.

use crate::lvgl_ffi::*;

use super::super::gfx::{Handle, Visibility};
use super::handle_table;

#[inline]
fn obj(h: Handle) -> *mut lv_obj_t {
    handle_table::lookup(h.to_java())
}

/// Convert an ARGB packed `0xAARRGGBB` to an `lv_color_t` (RGB888 — alpha
/// is currently ignored; use [`set_alpha`] for whole-widget opacity).
fn argb_to_lv_color(argb: u32) -> lv_color_t {
    lv_color_t {
        red: ((argb >> 16) & 0xFF) as u8,
        green: ((argb >> 8) & 0xFF) as u8,
        blue: (argb & 0xFF) as u8,
    }
}

pub(in crate::system::picodroid::graphics) fn set_pos(h: Handle, x: i32, y: i32) {
    unsafe { lv_obj_set_pos(obj(h), x, y) };
}

pub(in crate::system::picodroid::graphics) fn set_size(h: Handle, w: i32, height: i32) {
    unsafe { lv_obj_set_size(obj(h), w, height) };
}

pub(in crate::system::picodroid::graphics) fn set_bg_color(h: Handle, argb: u32) {
    let color = argb_to_lv_color(argb);
    unsafe { lv_obj_set_style_bg_color(obj(h), color, 0) };
}

pub(in crate::system::picodroid::graphics) fn set_padding(
    h: Handle,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) {
    let o = obj(h);
    unsafe {
        lv_obj_set_style_pad_left(o, left, 0);
        lv_obj_set_style_pad_top(o, top, 0);
        lv_obj_set_style_pad_right(o, right, 0);
        lv_obj_set_style_pad_bottom(o, bottom, 0);
    }
}

pub(in crate::system::picodroid::graphics) fn set_visibility(h: Handle, v: Visibility) {
    let o = obj(h);
    unsafe {
        match v {
            Visibility::Visible => lv_obj_remove_flag(o, LV_OBJ_FLAG_HIDDEN),
            // Both INVISIBLE and GONE map to the HIDDEN flag — Android's
            // GONE additionally collapses layout space, but we don't
            // distinguish at this layer today.
            Visibility::Invisible | Visibility::Gone => lv_obj_add_flag(o, LV_OBJ_FLAG_HIDDEN),
        }
    }
}

pub(in crate::system::picodroid::graphics) fn set_enabled(h: Handle, on: bool) {
    let o = obj(h);
    unsafe {
        if on {
            lv_obj_remove_state(o, LV_STATE_DISABLED);
        } else {
            lv_obj_add_state(o, LV_STATE_DISABLED);
        }
    }
}

pub(in crate::system::picodroid::graphics) fn set_alpha(h: Handle, alpha: u8) {
    unsafe { lv_obj_set_style_opa(obj(h), alpha, 0) };
}

pub(in crate::system::picodroid::graphics) fn set_parent(h: Handle, parent: Handle) {
    unsafe { lv_obj_set_parent(obj(h), obj(parent)) };
}

pub(in crate::system::picodroid::graphics) fn delete(h: Handle) {
    unsafe { lv_obj_delete(obj(h)) };
}
