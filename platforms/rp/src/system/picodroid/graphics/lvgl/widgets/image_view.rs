// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ImageView` (LVGL `lv_image`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

// Java-side `ImageView.SCALE_*` constants — keep in sync with ImageView.java.
const JAVA_SCALE_FIT_CENTER: i32 = 0;
const JAVA_SCALE_CENTER_CROP: i32 = 1;
const JAVA_SCALE_FIT_XY: i32 = 2;
const JAVA_SCALE_TILE: i32 = 3;
const JAVA_SCALE_CENTER: i32 = 4;

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe { lv_image_create(lifecycle::screen_ptr()) };
    handle_table::register(ptr)
}

pub(in crate::system::picodroid::graphics) fn set_scale_type(id: i32, scale_type: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    let align = match scale_type {
        JAVA_SCALE_FIT_CENTER => LV_IMAGE_ALIGN_CONTAIN,
        JAVA_SCALE_CENTER_CROP => LV_IMAGE_ALIGN_COVER,
        JAVA_SCALE_FIT_XY => LV_IMAGE_ALIGN_STRETCH,
        JAVA_SCALE_TILE => LV_IMAGE_ALIGN_TILE,
        JAVA_SCALE_CENTER => LV_IMAGE_ALIGN_CENTER,
        // Anything else falls back to plain centering — matches LVGL's
        // default and is the least-surprising behavior for an out-of-range
        // ordinal.
        _ => LV_IMAGE_ALIGN_CENTER,
    };
    unsafe { lv_image_set_inner_align(obj, align) };
}

pub(in crate::system::picodroid::graphics) fn set_tint(id: i32, argb: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    // Top byte is alpha (blend opacity); low 24 bits are the tint color.
    // Cast through u32 first so a negative i32 (e.g. 0xFFRRGGBB stored as a
    // signed Java int) keeps its bit pattern.
    let argb_u = argb as u32;
    let alpha = (argb_u >> 24) as u8;
    let rgb = argb_u & 0x00FF_FFFF;
    unsafe {
        lv_obj_set_style_image_recolor(obj, lv_color_hex(rgb), 0);
        lv_obj_set_style_image_recolor_opa(obj, alpha, 0);
    }
}

pub(in crate::system::picodroid::graphics) fn set_scale(id: i32, zoom: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    // LVGL takes uint32_t. Negative zoom values are nonsensical; clamp to 0.
    let z = if zoom < 0 { 0u32 } else { zoom as u32 };
    unsafe { lv_image_set_scale(obj, z) };
}

/// Hand a bundled-asset descriptor to LVGL for this image widget.
///
/// `dsc` must outlive the widget — it is `'static` in practice because we
/// only call this with pointers from `graphics::assets`, which leaks each
/// descriptor for the firmware's lifetime.
pub(in crate::system::picodroid::graphics) fn set_src(id: i32, dsc: *const lv_image_dsc_t) {
    let obj = handle_table::lookup(id);
    if obj.is_null() || dsc.is_null() {
        return;
    }
    unsafe { lv_image_set_src(obj, dsc as *const core::ffi::c_void) };
}
