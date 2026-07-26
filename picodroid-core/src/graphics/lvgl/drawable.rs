// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `picodroid.graphics.drawable.GradientDrawable.applyTo`.
//!
//! A drawable bundles a handful of LVGL style properties (bg fill,
//! optional 2-color gradient, corner radius, stroke) that are applied
//! atomically to a single widget. This mirrors how Android's drawable
//! system feels to use without buying into Android's draw-loop machinery
//! — picodroid views still render through LVGL; the drawable is just a
//! convenient bundle of style setters.
//!
//! All FFI calls go through the explicit setter functions in `lvgl_ffi.rs`
//! (radius, border_*, bg_grad_*) — no `lv_anim_t`-style struct layout
//! assumptions.

use crate::lvgl_ffi::*;

use super::handle_table;

/// Apply every property of a GradientDrawable to a single LVGL widget.
/// The five "fill" args (`fill_color`, optional gradient triple) are
/// mutually exclusive: when `has_gradient` is set, the solid fill is
/// replaced by a vertical or horizontal gradient between
/// `gradient_start` and `gradient_end`. When unset, the gradient
/// direction is forced back to `LV_GRAD_DIR_NONE` so a previously-
/// applied gradient on the same widget is fully cleared.
#[allow(clippy::too_many_arguments)]
pub fn apply_gradient_drawable(
    handle: i32,
    fill_argb: u32,
    radius: i32,
    stroke_width: i32,
    stroke_argb: u32,
    has_gradient: bool,
    gradient_start_argb: u32,
    gradient_end_argb: u32,
    gradient_direction: u32,
) {
    let obj = handle_table::lookup(handle);
    if obj.is_null() {
        return;
    }

    unsafe {
        // Radius and border are independent of fill/gradient — set first.
        lv_obj_set_style_radius(obj, radius.max(0), 0);
        lv_obj_set_style_border_width(obj, stroke_width.max(0), 0);
        if stroke_width > 0 {
            lv_obj_set_style_border_color(obj, lv_color_hex(stroke_argb & 0x00FF_FFFF), 0);
        }

        // Fill: gradient takes precedence when requested. The bg_color
        // doubles as the gradient's start when bg_grad_dir is non-NONE.
        if has_gradient {
            let dir = match gradient_direction {
                LV_GRAD_DIR_HOR => LV_GRAD_DIR_HOR,
                _ => LV_GRAD_DIR_VER, // default to vertical for any unknown code
            };
            lv_obj_set_style_bg_color(obj, lv_color_hex(gradient_start_argb & 0x00FF_FFFF), 0);
            lv_obj_set_style_bg_grad_color(obj, lv_color_hex(gradient_end_argb & 0x00FF_FFFF), 0);
            lv_obj_set_style_bg_grad_dir(obj, dir, 0);
            // Use the start color's alpha as the overall background opacity.
            lv_obj_set_style_bg_opa(obj, ((gradient_start_argb >> 24) & 0xFF) as u8, 0);
        } else {
            // Reset any previously-applied gradient. Without this, swapping
            // a gradient drawable for a solid one would leave the gradient
            // descriptor in place and the second color would still bleed
            // through.
            lv_obj_set_style_bg_grad_dir(obj, LV_GRAD_DIR_NONE, 0);
            lv_obj_set_style_bg_color(obj, lv_color_hex(fill_argb & 0x00FF_FFFF), 0);
            lv_obj_set_style_bg_opa(obj, ((fill_argb >> 24) & 0xFF) as u8, 0);
        }
    }
}
