//! Native Rust test app for LVGL display integration.
//!
//! Creates a simple UI with a label, a button (with click counter),
//! a progress bar, and a switch to validate the full display + touch stack.
//!
//! On real hardware a 4-point touch calibration runs first so that
//! subsequent tap/toggle interactions map correctly to screen coordinates.

use crate::hal;
use crate::lvgl_ffi::*;
use crate::system::picodroid::graphics::engine;

/// Counter for button taps — updated from the LVGL event callback.
static mut TAP_COUNT: i32 = 0;

/// Pointer to the counter label — updated from the event callback.
static mut COUNTER_LABEL: *mut lv_obj_t = core::ptr::null_mut();

/// Pointer to the progress bar — updated from the tick loop.
static mut PROGRESS_BAR: *mut lv_obj_t = core::ptr::null_mut();

// ---------------------------------------------------------------------------
// Calibration
// ---------------------------------------------------------------------------

/// Inset from screen edges for calibration target points (pixels).
const CAL_MARGIN: i32 = 30;

/// Approximate pixel offset to centre a "+" glyph on its target position.
const GLYPH_HALF_W: i32 = 5;
const GLYPH_HALF_H: i32 = 8;

/// Number of consecutive consistent readings required to accept a touch.
const DEBOUNCE_COUNT: usize = 8;

/// Maximum ADC deviation between consecutive samples to count as "consistent".
/// Real finger presses are stable; ghost noise is erratic.
const DEBOUNCE_TOLERANCE: u16 = 60;

/// Screen-coordinate targets for the four calibration taps.
/// Order: top-left, top-right, bottom-right, bottom-left.
const CAL_TARGETS: [(i32, i32); 4] = [
    (CAL_MARGIN, CAL_MARGIN),
    (hal::display::WIDTH as i32 - 1 - CAL_MARGIN, CAL_MARGIN),
    (
        hal::display::WIDTH as i32 - 1 - CAL_MARGIN,
        hal::display::HEIGHT as i32 - 1 - CAL_MARGIN,
    ),
    (CAL_MARGIN, hal::display::HEIGHT as i32 - 1 - CAL_MARGIN),
];

/// Run touch sensitivity calibration followed by 4-point position calibration.
#[cfg(not(feature = "sim"))]
fn run_calibration() {
    unsafe { run_calibration_inner() }
}

#[cfg(not(feature = "sim"))]
unsafe fn run_calibration_inner() {
    let screen = engine::screen();

    let instr = lv_label_create(screen);
    lv_label_set_text(instr, b"Touch each + target\0".as_ptr() as *const _);
    lv_obj_set_pos(instr, 60, 10);

    let cross = lv_label_create(screen);
    lv_label_set_text(cross, b"+\0".as_ptr() as *const _);

    let step_lbl = lv_label_create(screen);

    let mut raw_pts: [(u16, u16); 4] = [(0, 0); 4];

    for (i, &(tx, ty)) in CAL_TARGETS.iter().enumerate() {
        let mut buf = [0u8; 16];
        let step_text = format_step(i + 1, &mut buf);
        lv_label_set_text(step_lbl, step_text.as_ptr() as *const _);
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

    apply_calibration(&raw_pts);

    lv_obj_clean(screen);
    engine::tick(16);
}

/// Spin until the touch panel is quiet (no stable contact).
///
/// Uses unfiltered reads — considers the screen "released" when
/// `DEBOUNCE_COUNT` consecutive readings are railed (< 50 or > 4050
/// on either axis), indicating no finger contact.
#[cfg(not(feature = "sim"))]
fn wait_for_release() {
    let mut quiet: usize = 0;
    loop {
        engine::tick(16);
        let (rx, ry) = hal::touch::read_raw_unfiltered();
        if rx < 50 || rx > 4050 || ry < 50 || ry > 4050 {
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

/// Wait for a debounced touch using unfiltered reads.
///
/// Requires `DEBOUNCE_COUNT` consecutive readings that are all within
/// `DEBOUNCE_TOLERANCE` ADC counts of each other *and* not railed.
/// Ghost noise is erratic and resets the streak; a real finger press
/// produces stable readings that pass.
///
/// Returns the average of the accepted readings.
#[cfg(not(feature = "sim"))]
fn wait_for_debounced_touch() -> (u16, u16) {
    let mut streak: usize = 0;
    let mut sum_x: u32 = 0;
    let mut sum_y: u32 = 0;
    let mut base_x: u16 = 0;
    let mut base_y: u16 = 0;

    loop {
        engine::tick(16);
        let (rx, ry) = hal::touch::read_raw_unfiltered();

        // Skip railed readings (no contact)
        if rx < 50 || rx > 4050 || ry < 50 || ry > 4050 {
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
            // Inconsistent — restart with this reading
            base_x = rx;
            base_y = ry;
            sum_x = rx as u32;
            sum_y = ry as u32;
            streak = 1;
        }
        hal::system_clock::sleep(16);
    }
}

/// Compute calibration constants from 4 raw corner readings and apply them.
///
/// The maths linearly extrapolates from the known margin-inset positions
/// to the screen edges (pixel 0 and pixel WIDTH-1 / HEIGHT-1).
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

    // Extrapolate to screen edges.
    // cal_x_min / cal_x_max may be inverted if the raw axis runs opposite to
    // the screen axis — map_range handles that correctly.
    let cal_x_min = (raw_x_left - m * (raw_x_right - raw_x_left) / span_x).clamp(0, 4095) as u16;
    let cal_x_max = (raw_x_right + m * (raw_x_right - raw_x_left) / span_x).clamp(0, 4095) as u16;
    let cal_y_min = (raw_y_top - m * (raw_y_bottom - raw_y_top) / span_y).clamp(0, 4095) as u16;
    let cal_y_max = (raw_y_bottom + m * (raw_y_bottom - raw_y_top) / span_y).clamp(0, 4095) as u16;

    hal::touch::set_calibration(cal_x_min, cal_x_max, cal_y_min, cal_y_max);
}

/// Format "N / 4\0" into a buffer for the step counter label.
#[cfg(not(feature = "sim"))]
fn format_step(n: usize, buf: &mut [u8; 16]) -> &[u8] {
    buf[0] = b'0' + n as u8;
    buf[1..6].copy_from_slice(b" / 4\0");
    &buf[..6]
}

// ---------------------------------------------------------------------------
// Main entry
// ---------------------------------------------------------------------------

/// Run the display test. This function sets up the UI and enters the main loop.
///
/// On real hardware this is called from a FreeRTOS task and never returns.
/// In sim mode it runs for a fixed number of ticks.
pub fn run() {
    engine::init();

    // On real hardware, run touch calibration before showing the demo UI.
    #[cfg(not(feature = "sim"))]
    run_calibration();

    unsafe {
        build_ui();
    }

    // Main loop: tick LVGL at ~60 fps
    let mut elapsed_ms: u32 = 0;
    loop {
        engine::tick(16);

        // Auto-advance the progress bar every ~100ms
        elapsed_ms += 16;
        if elapsed_ms % 100 == 0 {
            unsafe {
                if !PROGRESS_BAR.is_null() {
                    let val = (elapsed_ms / 100 % 101) as i32;
                    lv_bar_set_value(PROGRESS_BAR, val, LV_ANIM_ON);
                }
            }
        }

        crate::hal::system_clock::sleep(16);

        // In sim mode, stop after ~5 seconds
        #[cfg(feature = "sim")]
        if elapsed_ms >= 5000 {
            break;
        }
    }
}

/// Build the LVGL widget tree.
///
/// # Safety
/// Must be called after `engine::init()`.
unsafe fn build_ui() {
    let screen = engine::screen();

    // Set up flex column layout on the screen
    lv_obj_set_flex_flow(screen, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_flex_align(
        screen,
        LV_FLEX_ALIGN_CENTER,
        LV_FLEX_ALIGN_CENTER,
        LV_FLEX_ALIGN_CENTER,
    );

    // Title label
    let title = lv_label_create(screen);
    lv_label_set_text(title, b"Picodroid LVGL Test\0".as_ptr() as *const _);

    // Counter label
    COUNTER_LABEL = lv_label_create(screen);
    lv_label_set_text(COUNTER_LABEL, b"Taps: 0\0".as_ptr() as *const _);

    // Button with label
    let btn = lv_button_create(screen);
    lv_obj_set_size(btn, 200, 50);
    let btn_label = lv_label_create(btn);
    lv_label_set_text(btn_label, b"Tap Me!\0".as_ptr() as *const _);
    lv_obj_center(btn_label);

    // Register click event
    lv_obj_add_event_cb(
        btn,
        Some(btn_click_cb),
        LV_EVENT_CLICKED,
        core::ptr::null_mut(),
    );

    // Progress bar
    PROGRESS_BAR = lv_bar_create(screen);
    lv_obj_set_size(PROGRESS_BAR, 200, 20);
    lv_bar_set_value(PROGRESS_BAR, 0, LV_ANIM_OFF);

    // Switch
    let _sw = lv_switch_create(screen);
}

/// LVGL event callback for the "Tap Me!" button.
///
/// # Safety
/// Called from LVGL's event dispatch.
unsafe extern "C" fn btn_click_cb(_e: *mut lv_event_t) {
    TAP_COUNT += 1;

    if !COUNTER_LABEL.is_null() {
        // Format the counter text into a small stack buffer
        let mut buf = [0u8; 32];
        let text = format_tap_count(TAP_COUNT, &mut buf);
        lv_label_set_text(COUNTER_LABEL, text.as_ptr() as *const _);
    }
}

/// Format "Taps: N\0" into the provided buffer. Returns a slice including the NUL.
fn format_tap_count(count: i32, buf: &mut [u8; 32]) -> &[u8] {
    // Simple integer-to-string without alloc
    let prefix = b"Taps: ";
    buf[..prefix.len()].copy_from_slice(prefix);
    let mut pos = prefix.len();

    if count == 0 {
        buf[pos] = b'0';
        pos += 1;
    } else {
        let mut n = count;
        let start = pos;
        while n > 0 {
            buf[pos] = b'0' + (n % 10) as u8;
            pos += 1;
            n /= 10;
        }
        // Reverse the digits
        buf[start..pos].reverse();
    }

    buf[pos] = 0; // NUL terminator
    &buf[..=pos]
}
