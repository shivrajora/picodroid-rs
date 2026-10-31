// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ImageView` (LVGL `lv_image`).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

// `ImageView.ScaleType.code` — keep in step with ImageView.java and
// papk-pack's `SCALE_TYPE` (the layout compiler writes the same codes).
const SCALE_FIT_CENTER: i32 = 0;
const SCALE_CENTER_CROP: i32 = 1;
const SCALE_FIT_XY: i32 = 2;
const SCALE_CENTER: i32 = 4;
const SCALE_CENTER_INSIDE: i32 = 5;
const SCALE_MATRIX: i32 = 8;

pub(in crate::graphics) fn create() -> i32 {
    lifecycle::screen_ptr().map_or(0, create_under)
}

/// [`create`], under `parent` instead of the screen (the inflater's path).
/// Starts at `FIT_CENTER`, Android's default scale type, which the Java
/// wrapper's `getScaleType` reports.
#[inline(never)]
pub(in crate::graphics) fn create_under(parent: Obj) -> i32 {
    match lv_image_create(parent) {
        Some(img) => {
            // SAFETY: `img` was just made by `lv_image_create`.
            unsafe { lv_image_set_inner_align(img, LV_IMAGE_ALIGN_CONTAIN) };
            handle_table::register(img)
        }
        None => 0,
    }
}

/// `id`'s image object. `ImageView`'s natives only: its constructors give
/// `View` the `lv_image` [`create`] made, and no other handle.
fn image(id: i32) -> Option<Obj> {
    handle_table::get(id)
}

/// The LVGL inner alignment for a `ScaleType` code. `FIT_START` (6) and
/// `FIT_END` (7) fit around the centre, as `FIT_CENTER` does: LVGL scales to
/// fit only centred. `MATRIX` is the identity matrix, unscaled at the
/// top-left. An unknown code (a layout from a newer framework) fits too.
fn inner_align(code: i32) -> lv_image_align_t {
    match code {
        SCALE_CENTER_CROP => LV_IMAGE_ALIGN_COVER,
        SCALE_FIT_XY => LV_IMAGE_ALIGN_STRETCH,
        SCALE_CENTER => LV_IMAGE_ALIGN_CENTER,
        SCALE_CENTER_INSIDE => LV_IMAGE_ALIGN_CONTAIN_DOWNSCALE,
        SCALE_MATRIX => LV_IMAGE_ALIGN_TOP_LEFT,
        SCALE_FIT_CENTER => LV_IMAGE_ALIGN_CONTAIN,
        _ => LV_IMAGE_ALIGN_CONTAIN,
    }
}

pub(in crate::graphics) fn set_scale_type(id: i32, code: i32) {
    let Some(obj) = image(id) else {
        return;
    };
    // SAFETY: `image` answers an `ImageView`'s `lv_image`.
    unsafe { lv_image_set_inner_align(obj, inner_align(code)) };
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
