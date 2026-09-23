// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `CircularProgressIndicator`: a passive `lv_arc`.
//!
//! LVGL's arc is a control — the theme gives it a knob and it is clickable, so a
//! touch drags the value. A progress indicator is neither: `create` sheds the
//! knob style and the clickable flag and the arc only ever moves through
//! `set_value`. The Java geometry (Material's `indicatorSize`/`trackThickness`,
//! `Canvas.drawArc`'s `startAngle`/`sweepAngle`) maps onto LVGL as follows:
//!
//! - the track is `bg_angles(0, sweep)` and `rotation = start`, because
//!   `lv_arc_set_bg_angles` wraps either angle past 360, so `(start,
//!   start + 360)` would collapse to an empty ring while `(0, 360)` is the one
//!   spelling LVGL accepts for a full circle;
//! - a colour's alpha byte becomes the part's `arc_opa`, `lv_color_t` carrying
//!   none, so `Color.TRANSPARENT` hides a track;
//! - one `arc_width` on both parts, the way Material's `trackThickness` works.
//!
//! Every setter looks the handle up and returns on a stale one, as the other
//! widgets do. The pure mappings sit apart so `cargo test` covers them; LVGL is
//! linked on the host but never initialised.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

/// Material's medium indicator: a 48 px ring.
const DEFAULT_SIZE_PX: i32 = 48;
const DEFAULT_TRACK_THICKNESS_PX: i32 = 4;
/// The indicator fills from 12 o'clock, round the whole circle.
const DEFAULT_START_DEG: i32 = 270;
const FULL_SWEEP_DEG: i32 = 360;
const DEFAULT_MIN: i32 = 0;
const DEFAULT_MAX: i32 = 100;

/// `CircularProgressIndicator.INDICATOR_DIRECTION_COUNTERCLOCKWISE`; keep in step
/// with the Java constant.
const INDICATOR_DIRECTION_COUNTERCLOCKWISE: i32 = 1;

pub(in crate::graphics) fn create(indicator_argb: i32, track_argb: i32) -> i32 {
    let ptr = unsafe {
        let a = lv_arc_create(lifecycle::screen_ptr());
        // The theme styles a knob at the indicator's end (lv_theme_default.c
        // binds `styles.knob` to LV_PART_KNOB) and the arc constructor makes it
        // clickable; a progress indicator is knobless and passive.
        lv_obj_remove_style(a, core::ptr::null(), LV_PART_KNOB);
        lv_obj_set_style_bg_opa(a, 0, LV_PART_KNOB);
        lv_obj_remove_flag(a, LV_OBJ_FLAG_CLICKABLE);
        lv_obj_set_size(a, DEFAULT_SIZE_PX, DEFAULT_SIZE_PX);
        lv_arc_set_rotation(a, DEFAULT_START_DEG);
        lv_arc_set_bg_angles(a, 0, FULL_SWEEP_DEG);
        lv_arc_set_mode(a, LV_ARC_MODE_NORMAL);
        lv_arc_set_range(a, DEFAULT_MIN, DEFAULT_MAX);
        // An arc starts VALUE_UNSET and draws no indicator until a value lands.
        lv_arc_set_value(a, DEFAULT_MIN);
        for part in [LV_PART_MAIN, LV_PART_INDICATOR] {
            lv_obj_set_style_arc_width(a, DEFAULT_TRACK_THICKNESS_PX, part);
            lv_obj_set_style_arc_rounded(a, true, part);
        }
        apply_color(a, LV_PART_INDICATOR, indicator_argb);
        apply_color(a, LV_PART_MAIN, track_argb);
        a
    };
    handle_table::register(ptr)
}

/// `ProgressBar.setProgress` on a ring: LVGL clamps into the range itself.
pub(in crate::graphics) fn set_value(id: i32, value: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe { lv_arc_set_value(obj, value) };
}

pub(in crate::graphics) fn set_range(id: i32, min: i32, max: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe { lv_arc_set_range(obj, min, max) };
}

pub(in crate::graphics) fn set_indicator_color(id: i32, argb: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe { apply_color(obj, LV_PART_INDICATOR, argb) };
}

pub(in crate::graphics) fn set_track_color(id: i32, argb: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe { apply_color(obj, LV_PART_MAIN, argb) };
}

pub(in crate::graphics) fn set_track_thickness(id: i32, px: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe {
        for part in [LV_PART_MAIN, LV_PART_INDICATOR] {
            lv_obj_set_style_arc_width(obj, px, part);
        }
    }
}

pub(in crate::graphics) fn set_indicator_direction(id: i32, direction: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe { lv_arc_set_mode(obj, arc_mode(direction)) };
}

pub(in crate::graphics) fn set_track_corner_radius(id: i32, radius: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe {
        for part in [LV_PART_MAIN, LV_PART_INDICATOR] {
            lv_obj_set_style_arc_rounded(obj, radius > 0, part);
        }
    }
}

/// `startAngle`/`sweepAngle` in `Canvas.drawArc` terms: degrees clockwise
/// from 3 o'clock, which is also LVGL's convention.
pub(in crate::graphics) fn set_angles(id: i32, start_deg: i32, sweep_deg: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe {
        // Rotation normalises itself into [0, 360).
        lv_arc_set_rotation(obj, start_deg);
        lv_arc_set_bg_angles(obj, 0, sweep_end(sweep_deg));
    }
}

unsafe fn apply_color(obj: *mut lv_obj_t, part: lv_style_selector_t, argb: i32) {
    let (rgb, opa) = split_argb(argb);
    lv_obj_set_style_arc_color(obj, lv_color_hex(rgb), part);
    lv_obj_set_style_arc_opa(obj, opa, part);
}

/// A Java `0xAARRGGBB` as LVGL's `0xRRGGBB` plus the part's opacity. Cast
/// through `u32` so a negative Java int keeps its bit pattern.
fn split_argb(argb: i32) -> (u32, u8) {
    let bits = argb as u32;
    (bits & 0x00FF_FFFF, (bits >> 24) as u8)
}

/// The track's end angle for a sweep: LVGL takes exactly `0..=360`, and 360
/// is its only spelling of a full circle, so wider sweeps clamp rather than
/// wrap to an empty ring. A negative sweep is empty; a counter-clockwise fill
/// is the indicator direction, not a negative sweep.
fn sweep_end(sweep_deg: i32) -> i32 {
    sweep_deg.clamp(0, FULL_SWEEP_DEG)
}

/// Material's `INDICATOR_DIRECTION_*` as the arc's fill mode. Anything but
/// COUNTERCLOCKWISE fills clockwise, as `setIndicatorDirection` documents.
fn arc_mode(direction: i32) -> lv_arc_mode_t {
    if direction == INDICATOR_DIRECTION_COUNTERCLOCKWISE {
        LV_ARC_MODE_REVERSE
    } else {
        LV_ARC_MODE_NORMAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_argb_keeps_the_colour_and_lifts_the_alpha() {
        assert_eq!(split_argb(0xFF7B_C47Fu32 as i32), (0x7BC4_7F, 0xFF));
        assert_eq!(split_argb(0x80FF_0000u32 as i32), (0xFF_0000, 0x80));
        // Color.TRANSPARENT: no colour, fully clear.
        assert_eq!(split_argb(0), (0, 0));
    }

    #[test]
    fn sweep_end_clamps_to_lvgls_one_full_circle_spelling() {
        assert_eq!(sweep_end(270), 270);
        assert_eq!(sweep_end(360), 360);
        assert_eq!(sweep_end(630), 360);
        assert_eq!(sweep_end(-10), 0);
        assert_eq!(sweep_end(0), 0);
    }

    #[test]
    fn arc_mode_reverses_only_for_counterclockwise() {
        assert_eq!(arc_mode(0), LV_ARC_MODE_NORMAL);
        assert_eq!(arc_mode(1), LV_ARC_MODE_REVERSE);
        assert_eq!(arc_mode(7), LV_ARC_MODE_NORMAL);
    }
}
