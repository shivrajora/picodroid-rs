// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ProgressBar`.
//!
//! Two flavours share one Java surface:
//! - **Determinate** — backed by `lv_bar`; `set_progress(0..100)` updates it.
//! - **Indeterminate** — backed by `lv_spinner`; `set_progress` is a silent
//!   no-op (the spinner animates by itself).
//!
//! Mode is chosen at construction time; the `nativeHandle` stored on the
//! Java View slot is stable for the widget's lifetime. We track the bar/
//! spinner distinction in a tiny static set so `set_progress` doesn't need
//! to call into LVGL's class registry on each invocation.

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

/// Maximum simultaneously-alive indeterminate ProgressBars. The static set
/// is tiny — most apps use 1, a few use 2 (e.g. one in a list row + a
/// global one in the action bar). 8 covers any realistic case without
/// burning RAM.
const MAX_SPINNERS: usize = 8;

static mut SPINNER_HANDLES: [usize; MAX_SPINNERS] = [0; MAX_SPINNERS];

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let b = lv_bar_create(lifecycle::screen_ptr());
        lv_bar_set_value(b, 0, LV_ANIM_OFF);
        b
    };
    handle_table::register(ptr)
}

pub(in crate::system::picodroid::graphics) fn create_indeterminate(argb: i32) -> i32 {
    let ptr = unsafe {
        let s = lv_spinner_create(lifecycle::screen_ptr());
        lv_spinner_set_anim_params(s, SPINNER_ANIM_DURATION_MS, SPINNER_ARC_SWEEP_DEG);
        apply_indeterminate_style(s, argb);
        s
    };
    register_spinner(ptr as usize);
    handle_table::register(ptr)
}

/// Stamps the visual defaults expected of an Android-style indeterminate
/// ProgressBar onto a freshly-created spinner. The track ring keeps LVGL's
/// default theme color (faint gray) — only the moving sweep is tinted, so
/// the result reads as "indicator over groove" rather than a flat ring.
unsafe fn apply_indeterminate_style(obj: *mut crate::lvgl_ffi::lv_obj_t, argb: i32) {
    // Cast through u32 first so a negative i32 (Java ints are signed) keeps
    // its bit pattern. Top byte is alpha, low 24 bits are the color — we
    // ignore alpha here because LVGL arc-color is opaque; opacity is set
    // separately via `lv_obj_set_style_opa` if ever needed.
    let rgb = (argb as u32) & 0x00FF_FFFF;
    lv_obj_set_style_arc_color(obj, lv_color_hex(rgb), LV_PART_INDICATOR);
    lv_obj_set_style_arc_width(obj, SPINNER_ARC_WIDTH_PX, LV_PART_INDICATOR);
    lv_obj_set_style_arc_width(obj, SPINNER_ARC_WIDTH_PX, LV_PART_MAIN);
    lv_obj_set_style_arc_rounded(obj, true, LV_PART_INDICATOR);
    lv_obj_set_style_arc_rounded(obj, true, LV_PART_MAIN);
}

pub(in crate::system::picodroid::graphics) fn set_tint(id: i32, argb: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() || !is_spinner(obj as usize) {
        // No-op on the determinate `lv_bar`. Matches Android's
        // `setIndeterminateTintList` which only affects the indeterminate
        // drawable.
        return;
    }
    let rgb = (argb as u32) & 0x00FF_FFFF;
    unsafe { lv_obj_set_style_arc_color(obj, lv_color_hex(rgb), LV_PART_INDICATOR) };
}

pub(in crate::system::picodroid::graphics) fn set_progress(id: i32, value: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    if is_spinner(obj as usize) {
        // Indeterminate: silently ignore. Matches Android's
        // ProgressBar.setProgress() under indeterminate=true.
        return;
    }
    unsafe { lv_bar_set_value(obj, value, LV_ANIM_ON) };
}

pub fn reset_progress_bar_state() {
    unsafe {
        for slot in &mut SPINNER_HANDLES[..] {
            *slot = 0;
        }
    }
}

fn register_spinner(handle: usize) {
    unsafe {
        for slot in &mut SPINNER_HANDLES[..] {
            if *slot == 0 {
                *slot = handle;
                return;
            }
        }
    }
    // Slot table full: spinner still renders, but `set_progress` will fall
    // through to `lv_bar_set_value` on the spinner pointer. LVGL's class
    // dispatch should ignore the call gracefully (the setter is a no-op
    // outside `lv_bar`), so the worst-case is the silent ignore we already
    // documented for the indeterminate path.
    let _ = handle;
}

fn is_spinner(handle: usize) -> bool {
    unsafe {
        for &slot in &SPINNER_HANDLES[..] {
            if slot == handle {
                return true;
            }
        }
    }
    false
}
