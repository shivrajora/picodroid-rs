//! 4-point interactive touch calibration.
//!
//! Displays a "+" target at each screen corner and waits for a debounced touch.
//! After all four points are collected, calibration constants are computed and
//! applied to the touch driver.  The screen is cleared afterwards.

#[cfg(not(feature = "sim"))]
use crate::hal;
#[cfg(not(feature = "sim"))]
use crate::lvgl_ffi::*;

#[cfg(not(feature = "sim"))]
use super::engine;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

#[cfg(not(feature = "sim"))]
const CAL_MARGIN: i32 = 30;
#[cfg(not(feature = "sim"))]
const GLYPH_HALF_W: i32 = 5;
#[cfg(not(feature = "sim"))]
const GLYPH_HALF_H: i32 = 8;
#[cfg(not(feature = "sim"))]
const DEBOUNCE_COUNT: usize = 8;
#[cfg(not(feature = "sim"))]
const DEBOUNCE_TOLERANCE: u16 = 60;

#[cfg(not(feature = "sim"))]
const CAL_TARGETS: [(i32, i32); 4] = [
    (CAL_MARGIN, CAL_MARGIN),
    (hal::display::WIDTH as i32 - 1 - CAL_MARGIN, CAL_MARGIN),
    (
        hal::display::WIDTH as i32 - 1 - CAL_MARGIN,
        hal::display::HEIGHT as i32 - 1 - CAL_MARGIN,
    ),
    (CAL_MARGIN, hal::display::HEIGHT as i32 - 1 - CAL_MARGIN),
];

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run interactive 4-point touch calibration.
///
/// No-op in sim mode (no touch hardware).
#[cfg(not(feature = "sim"))]
pub fn calibrate() {
    unsafe { calibrate_inner() }
}

#[cfg(feature = "sim")]
pub fn calibrate() {}

// ---------------------------------------------------------------------------
// Implementation (hardware only)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "sim"))]
fn stopped() -> bool {
    crate::pdb::pending::is_stop_jvm()
}

#[cfg(not(feature = "sim"))]
unsafe fn calibrate_inner() {
    let scr = engine::screen();

    let instr = lv_label_create(scr);
    lv_label_set_text(instr, c"Touch each + target".as_ptr());
    lv_obj_set_pos(instr, 60, 10);

    let cross = lv_label_create(scr);
    lv_label_set_text(cross, c"+".as_ptr());

    let step_lbl = lv_label_create(scr);

    let mut raw_pts: [(u16, u16); 4] = [(0, 0); 4];

    for (i, &(tx, ty)) in CAL_TARGETS.iter().enumerate() {
        if stopped() {
            lv_obj_clean(scr);
            return;
        }

        let mut buf = [0u8; 16];
        buf[0] = b'1' + i as u8;
        buf[1..6].copy_from_slice(b" / 4\0");
        lv_label_set_text(step_lbl, buf.as_ptr() as *const _);
        lv_obj_set_pos(step_lbl, 130, 110);

        lv_obj_set_pos(cross, tx - GLYPH_HALF_W, ty - GLYPH_HALF_H);
        engine::tick(16);

        wait_for_release();
        raw_pts[i] = wait_for_debounced_touch();

        // Brief visual feedback
        lv_obj_set_pos(cross, -50, -50);
        engine::tick(16);
        hal::system_clock::sleep(200);
    }

    if stopped() {
        lv_obj_clean(scr);
        return;
    }
    apply_calibration(&raw_pts);

    lv_obj_clean(scr);
    engine::tick(16);
}

#[cfg(not(feature = "sim"))]
fn wait_for_release() {
    let mut quiet: usize = 0;
    loop {
        if stopped() {
            return;
        }
        engine::tick(16);
        let (rx, ry) = hal::touch::read_raw_unfiltered();
        if !(50..=4050).contains(&rx) || !(50..=4050).contains(&ry) {
            quiet += 1;
            if quiet >= DEBOUNCE_COUNT {
                return;
            }
        } else {
            quiet = 0;
        }
        hal::system_clock::sleep(16);
    }
}

#[cfg(not(feature = "sim"))]
fn wait_for_debounced_touch() -> (u16, u16) {
    let mut streak: usize = 0;
    let mut sum_x: u32 = 0;
    let mut sum_y: u32 = 0;
    let mut base_x: u16 = 0;
    let mut base_y: u16 = 0;

    loop {
        if stopped() {
            return (0, 0);
        }
        engine::tick(16);
        let (rx, ry) = hal::touch::read_raw_unfiltered();

        if !(50..=4050).contains(&rx) || !(50..=4050).contains(&ry) {
            streak = 0;
            hal::system_clock::sleep(16);
            continue;
        }

        if streak == 0 {
            base_x = rx;
            base_y = ry;
            sum_x = rx as u32;
            sum_y = ry as u32;
            streak = 1;
        } else if rx.abs_diff(base_x) <= DEBOUNCE_TOLERANCE
            && ry.abs_diff(base_y) <= DEBOUNCE_TOLERANCE
        {
            sum_x += rx as u32;
            sum_y += ry as u32;
            streak += 1;
            if streak >= DEBOUNCE_COUNT {
                return (
                    (sum_x / streak as u32) as u16,
                    (sum_y / streak as u32) as u16,
                );
            }
        } else {
            base_x = rx;
            base_y = ry;
            sum_x = rx as u32;
            sum_y = ry as u32;
            streak = 1;
        }
        hal::system_clock::sleep(16);
    }
}

#[cfg(not(feature = "sim"))]
fn apply_calibration(pts: &[(u16, u16); 4]) {
    let w = hal::display::WIDTH as i32 - 1;
    let h = hal::display::HEIGHT as i32 - 1;
    let m = CAL_MARGIN;

    let raw_x_left = (pts[0].0 as i32 + pts[3].0 as i32) / 2;
    let raw_x_right = (pts[1].0 as i32 + pts[2].0 as i32) / 2;
    let raw_y_top = (pts[0].1 as i32 + pts[1].1 as i32) / 2;
    let raw_y_bottom = (pts[2].1 as i32 + pts[3].1 as i32) / 2;

    let span_x = w - 2 * m;
    let span_y = h - 2 * m;

    let cal_x_min = (raw_x_left - m * (raw_x_right - raw_x_left) / span_x).clamp(0, 4095) as u16;
    let cal_x_max = (raw_x_right + m * (raw_x_right - raw_x_left) / span_x).clamp(0, 4095) as u16;
    let cal_y_min = (raw_y_top - m * (raw_y_bottom - raw_y_top) / span_y).clamp(0, 4095) as u16;
    let cal_y_max = (raw_y_bottom + m * (raw_y_bottom - raw_y_top) / span_y).clamp(0, 4095) as u16;

    hal::touch::set_calibration(cal_x_min, cal_x_max, cal_y_min, cal_y_max);
}
