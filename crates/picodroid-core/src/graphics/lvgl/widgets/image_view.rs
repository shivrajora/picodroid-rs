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

pub(in crate::graphics) fn create() -> i32 {
    match lifecycle::screen_ptr().and_then(|o| lv_image_create(o)) {
        Some(img) => handle_table::register(img),
        None => 0,
    }
}

/// `id`'s image object. `ImageView`'s natives only: its constructors give
/// `View` the `lv_image` [`create`] made, and no other handle.
fn image(id: i32) -> Option<Obj> {
    handle_table::get(id)
}

pub(in crate::graphics) fn set_scale_type(id: i32, scale_type: i32) {
    let Some(obj) = image(id) else {
        return;
    };
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
    // SAFETY: `image` answers an `ImageView`'s `lv_image`.
    unsafe { lv_image_set_inner_align(obj, align) };
}

pub(in crate::graphics) fn set_tint(id: i32, argb: i32) {
    let Some(obj) = image(id) else {
        return;
    };
    // Top byte is alpha (blend opacity); low 24 bits are the tint color.
    // Cast through u32 first so a negative i32 (e.g. 0xFFRRGGBB stored as a
    // signed Java int) keeps its bit pattern.
    let argb_u = argb as u32;
    let alpha = (argb_u >> 24) as u8;
    let rgb = argb_u & 0x00FF_FFFF;
    lv_obj_set_style_image_recolor(obj, lv_color_hex(rgb), 0);
    lv_obj_set_style_image_recolor_opa(obj, alpha, 0);
}

pub(in crate::graphics) fn set_scale(id: i32, zoom: i32) {
    let Some(obj) = image(id) else {
        return;
    };
    // LVGL takes uint32_t. Negative zoom values are nonsensical; clamp to 0.
    let z = if zoom < 0 { 0u32 } else { zoom as u32 };
    // SAFETY: `image` answers an `ImageView`'s `lv_image`.
    unsafe { lv_image_set_scale(obj, z) };
}

/// Hand a bundled-asset descriptor to LVGL for this image widget.
#[cfg(not(test))] // the asset registry is `cfg(not(test))`
pub(in crate::graphics) fn set_src(id: i32, src: crate::graphics::assets::AssetImage) {
    let Some(obj) = image(id) else {
        return;
    };
    // SAFETY: `image` answers an `ImageView`'s `lv_image`, and an
    // `AssetImage` lives in the asset registry until the app reset, after
    // the widget tree that shows it is gone (`assets.rs`).
    unsafe { lv_image_set_src(obj, src.as_src()) };
}
