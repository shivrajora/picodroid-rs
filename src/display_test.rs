//! Native Rust test app for LVGL display integration.
//!
//! Creates a simple UI with a label, a button (with click counter),
//! a progress bar, and a switch to validate the full display + touch stack.
//!
//! On real hardware a 4-point touch calibration runs first so that
//! subsequent tap/toggle interactions map correctly to screen coordinates.

use crate::lvgl_ffi::*;
use crate::system::picodroid::graphics::engine;

/// Counter for button taps — updated from the LVGL event callback.
static mut TAP_COUNT: i32 = 0;

/// Pointer to the counter label — updated from the event callback.
static mut COUNTER_LABEL: *mut lv_obj_t = core::ptr::null_mut();

/// Pointer to the progress bar — updated from the tick loop.
static mut PROGRESS_BAR: *mut lv_obj_t = core::ptr::null_mut();

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
    engine::calibrate();

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
