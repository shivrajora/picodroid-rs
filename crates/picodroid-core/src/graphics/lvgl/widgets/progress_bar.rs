// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ProgressBar`.
//!
//! Two flavours share one Java surface:
//! - **Determinate** — backed by `lv_bar`: value, range and the two part
//!   tints (fill and track) land here.
//! - **Indeterminate** — backed by `lv_spinner`: only the arc tint applies;
//!   the spinner animates by itself.
//!
//! The flavour is fixed at construction and known to Java (`indeterminate`),
//! which routes every call, so the bar entry points here never see a spinner
//! pointer. That matters: `LV_USE_ASSERT_OBJ` is off, so an `lv_bar_*` call on
//! the wrong class would be undefined behaviour rather than a checked no-op.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

/// Default arc-rotation period and sweep for the indeterminate spinner.
/// 1.0 s feels responsive without being twitchy; 60° is the LVGL default
/// for `lv_spinner` and reads as a clear "still working" indicator on a
/// 240×240 panel.
const SPINNER_ANIM_DURATION_MS: u32 = 1000;
const SPINNER_ARC_SWEEP_DEG: u32 = 60;

/// Tuned to match the LVGL 8.0 docs spinner look on a 240×240 panel.
/// Android's public ProgressBar API doesn't expose arc width or rounded
/// caps either — these stay internal.
const SPINNER_ARC_WIDTH_PX: i32 = 6;

/// `android.widget.ProgressBar.PROGRESS_ANIM_DURATION`: `setProgress(v, true)`
/// moves the fill there over 80 ms. Set explicitly because LVGL's default
/// theme leaves a bar's `anim_duration` at the property default, 0 ms, under
/// which `LV_ANIM_ON` completes on the next tick.
const PROGRESS_ANIM_DURATION_MS: u32 = 80;

/// `nativeSetTint` targets — keep in step with `ProgressBar.java`.
const TINT_PROGRESS: i32 = 0;
const TINT_PROGRESS_BACKGROUND: i32 = 1;
const TINT_INDETERMINATE: i32 = 2;

pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let b = lv_bar_create(lifecycle::screen_ptr());
        lv_bar_set_value(b, 0, LV_ANIM_OFF);
        lv_obj_set_style_anim_duration(b, PROGRESS_ANIM_DURATION_MS, LV_PART_MAIN);
        b
    };
    handle_table::register(ptr)
}

pub(in crate::graphics) fn create_indeterminate(argb: i32) -> i32 {
    let ptr = unsafe {
        let s = lv_spinner_create(lifecycle::screen_ptr());
        lv_spinner_set_anim_params(s, SPINNER_ANIM_DURATION_MS, SPINNER_ARC_SWEEP_DEG);
        apply_indeterminate_style(s, argb);
        s
    };
    handle_table::register(ptr)
}

/// Stamps the visual defaults expected of an Android-style indeterminate
/// ProgressBar onto a freshly-created spinner. The track ring keeps LVGL's
/// default theme color (faint gray) — only the moving sweep is tinted, so
/// the result reads as "indicator over groove" rather than a flat ring.
unsafe fn apply_indeterminate_style(obj: *mut lv_obj_t, argb: i32) {
    set_arc_color(obj, argb);
    lv_obj_set_style_arc_width(obj, SPINNER_ARC_WIDTH_PX, LV_PART_INDICATOR);
    lv_obj_set_style_arc_width(obj, SPINNER_ARC_WIDTH_PX, LV_PART_MAIN);
    lv_obj_set_style_arc_rounded(obj, true, LV_PART_INDICATOR);
    lv_obj_set_style_arc_rounded(obj, true, LV_PART_MAIN);
}

/// Cast through u32 so a negative Java int keeps its bit pattern; the arc
/// colour is opaque in LVGL, so the top (alpha) byte is dropped.
unsafe fn set_arc_color(obj: *mut lv_obj_t, argb: i32) {
    let rgb = (argb as u32) & 0x00FF_FFFF;
    lv_obj_set_style_arc_color(obj, lv_color_hex(rgb), LV_PART_INDICATOR);
}

/// `value` is already clamped to the range by Java.
pub(in crate::graphics) fn set_progress(id: i32, value: i32, animate: bool) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    let anim = if animate { LV_ANIM_ON } else { LV_ANIM_OFF };
    unsafe { lv_bar_set_value(obj, value, anim) };
}

/// Java guarantees `min <= max` and `min <= progress <= max`.
///
/// `lv_bar_set_range` clamps the bar's own value, but through an
/// `lv_bar_set_value(.., LV_ANIM_OFF)` that returns early because the value
/// was just overwritten — so an animation still running from
/// `setProgress(v, true)` survives, and rewrites the value with its stale
/// target when it completes (`lv_bar.c`: `lv_bar_set_range`,
/// `lv_bar_set_value_with_anim`, `lv_bar_anim_completed`). Only the
/// `LV_ANIM_OFF` path deletes it, and only for a value that differs from the
/// current one, so bounce through both bounds first: the value cannot equal
/// both, so at least one bounce takes that path. Nothing renders in between.
pub(in crate::graphics) fn set_range(id: i32, min: i32, max: i32, progress: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe {
        lv_bar_set_range(obj, min, max);
        if min < max {
            lv_bar_set_value(obj, min, LV_ANIM_OFF);
            lv_bar_set_value(obj, max, LV_ANIM_OFF);
        }
        lv_bar_set_value(obj, progress, LV_ANIM_OFF);
    }
}

/// Targets 0 and 1 (bar fill and track) set or, with `apply == false`, drop
/// the part's local background colour; target 2 recolours the spinner arc.
pub(in crate::graphics) fn set_tint(id: i32, target: i32, argb: i32, apply: bool) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    let part = match target {
        TINT_PROGRESS => LV_PART_INDICATOR,
        TINT_PROGRESS_BACKGROUND => LV_PART_MAIN,
        TINT_INDETERMINATE => {
            unsafe { set_arc_color(obj, argb) };
            return;
        }
        _ => return,
    };
    // Top byte is alpha, low 24 bits the colour (the ImageView.setTint idiom).
    let argb = argb as u32;
    unsafe {
        if apply {
            lv_obj_set_style_bg_color(obj, lv_color_hex(argb & 0x00FF_FFFF), part);
            lv_obj_set_style_bg_opa(obj, (argb >> 24) as u8, part);
        } else {
            // Back to the theme's own colour. Its track is translucent
            // (`bg_opa` 20 %), so the opacity has to go with the colour.
            lv_obj_remove_local_style_prop(obj, LV_STYLE_BG_COLOR, part);
            lv_obj_remove_local_style_prop(obj, LV_STYLE_BG_OPA, part);
        }
    }
}
