// SPDX-License-Identifier: GPL-3.0-only
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
use super::lifecycle;

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

/// Has a debug bridge asked the JVM to stop? Bails out of the calibration
/// wait loop so a stop request is not blocked by a user who never taps.
///
/// Was two `family-rp`-gated arms — the real debug-bridge poll, and `false`
/// for every other build. The platform-hook seam answers this for any
/// platform, so there is one implementation.
#[cfg(not(feature = "sim"))]
fn stopped() -> bool {
    crate::host::stop_requested()
}

#[cfg(not(feature = "sim"))]
unsafe fn calibrate_inner() {
    let scr = lifecycle::screen_ptr();

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
        lifecycle::tick(crate::executors::tick_source::step_ms());

        wait_for_release();
        raw_pts[i] = wait_for_debounced_touch();

        // Brief visual feedback
        lv_obj_set_pos(cross, -50, -50);
        lifecycle::tick(crate::executors::tick_source::step_ms());
        hal::system_clock::sleep(200);
    }

    if stopped() {
        lv_obj_clean(scr);
        return;
    }
    apply_calibration(&raw_pts);

    lv_obj_clean(scr);
    lifecycle::tick(crate::executors::tick_source::step_ms());
}

#[cfg(not(feature = "sim"))]
fn wait_for_release() {
    let mut quiet: usize = 0;
    loop {
        if stopped() {
            return;
        }
        lifecycle::tick(crate::executors::tick_source::step_ms());
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
        lifecycle::tick(crate::executors::tick_source::step_ms());
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
    let (x_min, x_max, y_min, y_max) = fit_calibration(
        pts,
        hal::display::WIDTH as i32,
        hal::display::HEIGHT as i32,
        CAL_MARGIN,
    );
    hal::touch::set_calibration(x_min, x_max, y_min, y_max);
}

/// The raw-ADC range that maps onto the full panel, from four crosshair
/// samples taken `margin` pixels in from each corner, in the order top-left,
/// top-right, bottom-right, bottom-left.
///
/// Each edge is the mean of its two corners, then pushed outwards by the
/// margin's share of the measured span -- the crosshairs sit inside the
/// panel, the calibration describes its edges. Results clamp to the 12-bit
/// ADC range. An inverted axis (left reads higher than right) comes out with
/// `min > max`, which is how the touch driver learns the axis is flipped.
// The simulator has no resistive panel to calibrate; its test build still
// runs the tests below.
#[cfg(any(test, not(feature = "sim")))]
fn fit_calibration(
    pts: &[(u16, u16); 4],
    width: i32,
    height: i32,
    margin: i32,
) -> (u16, u16, u16, u16) {
    let w = width - 1;
    let h = height - 1;
    let m = margin;

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

    (cal_x_min, cal_x_max, cal_y_min, cal_y_max)
}

#[cfg(test)]
mod tests {
    use super::fit_calibration;

    /// A 241 x 321 panel with a 20 px margin has crosshair spans of exactly
    /// 200 and 280, so the expected edges are whole numbers.
    const W: i32 = 241;
    const H: i32 = 321;
    const M: i32 = 20;

    #[test]
    fn edges_are_extrapolated_outwards_by_the_margins_share() {
        // x: 1000..3000 over a 200 px span = 10 counts/px -> +-200 for 20 px.
        // y: 800..3600 over a 280 px span = 10 counts/px -> +-200.
        let pts = [(1000, 800), (3000, 800), (3000, 3600), (1000, 3600)];
        assert_eq!(fit_calibration(&pts, W, H, M), (800, 3200, 600, 3800));
    }

    #[test]
    fn each_edge_is_the_mean_of_its_two_corners() {
        // Left edge sampled at 980 and 1020, top edge at 780 and 820: a panel
        // mounted slightly skewed still gets the axis-aligned best fit.
        let pts = [(980, 780), (3000, 820), (3000, 3600), (1020, 3600)];
        let (x_min, _, y_min, _) = fit_calibration(&pts, W, H, M);
        assert_eq!((x_min, y_min), (800, 600));
    }

    #[test]
    fn an_inverted_axis_comes_out_with_min_above_max() {
        let pts = [(3000, 800), (1000, 800), (1000, 3600), (3000, 3600)];
        let (x_min, x_max, y_min, y_max) = fit_calibration(&pts, W, H, M);
        assert_eq!((x_min, x_max), (3200, 800));
        assert!(y_min < y_max);
    }

    #[test]
    fn extrapolation_clamps_to_the_adc_range() {
        let pts = [(50, 40), (4050, 40), (4050, 4060), (50, 4060)];
        assert_eq!(fit_calibration(&pts, W, H, M), (0, 4095, 0, 4095));
    }
}
