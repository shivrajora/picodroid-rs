//! LVGL display engine — manages the LVGL lifecycle, display, and touch input.

use crate::hal;
use crate::lvgl_ffi::*;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

/// Band buffer: 320 pixels wide x 20 rows x 2 bytes (RGB565) = 12,800 bytes.
/// LVGL renders into this buffer band-by-band (partial render mode).
const BAND_HEIGHT: usize = 20;
const BAND_BUF_SIZE: usize = hal::display::WIDTH as usize * BAND_HEIGHT * 2;

/// Use a wrapper to get a raw pointer without creating a mutable reference.
#[allow(dead_code)]
struct BandBuf([u8; BAND_BUF_SIZE]);
static mut BAND_BUF: BandBuf = BandBuf([0u8; BAND_BUF_SIZE]);

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize LVGL, the ST7789 display, and the XPT2046 touch controller.
///
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);

    // Initialize hardware
    hal::display::init();
    hal::touch::init();
    hal::display::set_backlight(true);

    unsafe {
        // Initialize LVGL core
        lv_init();

        // Create display (320x240)
        let disp = lv_display_create(hal::display::WIDTH as i32, hal::display::HEIGHT as i32);
        lv_display_set_flush_cb(disp, Some(flush_cb));
        lv_display_set_buffers(
            disp,
            core::ptr::addr_of_mut!(BAND_BUF).cast::<u8>() as *mut c_void,
            core::ptr::null_mut(), // single buffer (no double-buffering)
            BAND_BUF_SIZE as u32,
            LV_DISPLAY_RENDER_MODE_PARTIAL,
        );

        // Create touch input device
        let indev = lv_indev_create();
        lv_indev_set_type(indev, LV_INDEV_TYPE_POINTER);
        lv_indev_set_read_cb(indev, Some(touch_read_cb));
    }
}

/// Advance LVGL's internal tick counter and process pending timers/rendering.
///
/// Call this periodically (e.g. every 16 ms for ~60 fps).
pub fn tick(ms: u32) {
    unsafe {
        lv_tick_inc(ms);
        lv_timer_handler();
    }
}

/// Get the active screen object.
pub fn screen() -> *mut lv_obj_t {
    unsafe { lv_screen_active() }
}

// ---------------------------------------------------------------------------
// 4-point touch calibration
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

/// Run interactive 4-point touch calibration.
///
/// Displays a "+" target at each screen corner and waits for a debounced touch.
/// After all four points are collected, calibration constants are computed and
/// applied to the touch driver.  The screen is cleared afterwards.
///
/// No-op in sim mode (no touch hardware).
#[cfg(not(feature = "sim"))]
pub fn calibrate() {
    unsafe { calibrate_inner() }
}

#[cfg(feature = "sim")]
pub fn calibrate() {}

#[cfg(not(feature = "sim"))]
unsafe fn calibrate_inner() {
    let scr = screen();

    let instr = lv_label_create(scr);
    lv_label_set_text(instr, c"Touch each + target".as_ptr());
    lv_obj_set_pos(instr, 60, 10);

    let cross = lv_label_create(scr);
    lv_label_set_text(cross, c"+".as_ptr());

    let step_lbl = lv_label_create(scr);

    let mut raw_pts: [(u16, u16); 4] = [(0, 0); 4];

    for (i, &(tx, ty)) in CAL_TARGETS.iter().enumerate() {
        let mut buf = [0u8; 16];
        buf[0] = b'1' + i as u8;
        buf[1..6].copy_from_slice(b" / 4\0");
        lv_label_set_text(step_lbl, buf.as_ptr() as *const _);
        lv_obj_set_pos(step_lbl, 130, 110);

        lv_obj_set_pos(cross, tx - GLYPH_HALF_W, ty - GLYPH_HALF_H);
        tick(16);

        wait_for_release();
        raw_pts[i] = wait_for_debounced_touch();

        // Brief visual feedback
        lv_obj_set_pos(cross, -50, -50);
        tick(16);
        hal::system_clock::sleep(200);
    }

    apply_calibration(&raw_pts);

    lv_obj_clean(scr);
    tick(16);
}

#[cfg(not(feature = "sim"))]
fn wait_for_release() {
    let mut quiet: usize = 0;
    loop {
        tick(16);
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
        tick(16);
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

/// LVGL display flush callback — called by LVGL when a region is ready to send.
///
/// # Safety
/// Called from LVGL's internal rendering pipeline.
unsafe extern "C" fn flush_cb(disp: *mut lv_display_t, area: *const lv_area_t, px_map: *mut u8) {
    let area = &*area;
    let x1 = area.x1 as u16;
    let y1 = area.y1 as u16;
    let x2 = area.x2 as u16;
    let y2 = area.y2 as u16;

    // Set the display window and stream the pixel data
    hal::display::set_window(x1, y1, x2, y2);

    let w = (x2 - x1 + 1) as usize;
    let h = (y2 - y1 + 1) as usize;
    let byte_count = w * h * 2; // RGB565 = 2 bytes per pixel
    let data = core::slice::from_raw_parts(px_map, byte_count);
    hal::display::write_pixels(data);

    // Tell LVGL the flush is complete
    lv_display_flush_ready(disp);
}

/// LVGL input device read callback — called by LVGL to poll touch state.
///
/// # Safety
/// Called from LVGL's internal input processing pipeline.
unsafe extern "C" fn touch_read_cb(_indev: *mut lv_indev_t, data: *mut lv_indev_data_t) {
    let data = &mut *data;
    match hal::touch::read_point() {
        Some((x, y)) => {
            data.point.x = x as i32;
            data.point.y = y as i32;
            data.state = LV_INDEV_STATE_PRESSED;
        }
        None => {
            data.state = LV_INDEV_STATE_RELEASED;
        }
    }
    data.continue_reading = false;
}
