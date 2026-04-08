//! FPS overlay — displays a moving-average frame rate counter on screen.
//!
//! Enabled via `Display.showFps()` from Java.  The LVGL label is created
//! lazily on the first `update()` call so that LVGL is guaranteed to be
//! initialised.

use crate::hal;
use crate::lvgl_ffi::*;

/// Number of frames in the sliding window.
const WINDOW_SIZE: usize = 10;

/// Whether the FPS overlay is enabled.
static mut ENABLED: bool = false;

/// Pointer to the LVGL label widget (null until first `update()`).
static mut FPS_LABEL: *mut lv_obj_t = core::ptr::null_mut();

/// Ring buffer of the last `WINDOW_SIZE` frame durations (microseconds).
static mut FRAME_US: [u64; WINDOW_SIZE] = [16_667; WINDOW_SIZE];

/// Current write position in the ring buffer.
static mut RING_IDX: usize = 0;

/// Number of samples collected so far (caps at `WINDOW_SIZE`).
static mut SAMPLES: usize = 0;

/// Timestamp of the previous frame (nanos).
static mut LAST_NANOS: i64 = 0;

/// Frame counter — used to throttle label updates.
static mut FRAME_COUNT: u32 = 0;

/// Mark the overlay as enabled.  The label is created lazily in `update()`.
pub fn enable() {
    unsafe {
        ENABLED = true;
    }
}

/// Called once per frame from the render loop.  No-op when disabled.
pub fn update() {
    unsafe {
        if !ENABLED {
            return;
        }

        let now = hal::system_clock::elapsed_realtime_nanos();

        // First frame — just record the timestamp; no delta yet.
        if LAST_NANOS == 0 {
            LAST_NANOS = now;
            create_label();
            return;
        }

        let delta_ns = now - LAST_NANOS;
        LAST_NANOS = now;

        let frame_us = (delta_ns / 1000) as u64;
        if frame_us == 0 {
            return;
        }

        // Store in ring buffer.
        FRAME_US[RING_IDX] = frame_us;
        RING_IDX = (RING_IDX + 1) % WINDOW_SIZE;
        if SAMPLES < WINDOW_SIZE {
            SAMPLES += 1;
        }

        FRAME_COUNT += 1;
        if FRAME_COUNT.is_multiple_of(WINDOW_SIZE as u32) {
            let avg_us = FRAME_US[..SAMPLES].iter().sum::<u64>() / SAMPLES as u64;
            let fps = if avg_us > 0 {
                (1_000_000u64 / avg_us) as u32
            } else {
                0
            };
            let mut buf = [0u8; 16];
            let text = format_fps(fps, &mut buf);
            lv_label_set_text(FPS_LABEL, text.as_ptr() as *const _);
        }
    }
}

/// Create the LVGL label in the top-right corner of the screen.
unsafe fn create_label() {
    let screen = lv_screen_active();
    FPS_LABEL = lv_label_create(screen);
    lv_label_set_text(FPS_LABEL, c"-- FPS".as_ptr());

    // Position in top-right corner (leave a small margin).
    lv_obj_set_pos(FPS_LABEL, (hal::display::WIDTH - 70) as i32, 2);

    // Green text on dark background.
    lv_obj_set_style_text_color(FPS_LABEL, lv_color_hex(0x00FF00), 0);
    lv_obj_set_style_bg_color(FPS_LABEL, lv_color_hex(0x000000), 0);
    lv_obj_set_style_bg_opa(FPS_LABEL, LV_OPA_COVER, 0);
}

/// Format `"NN FPS\0"` into `buf` without heap allocation.
fn format_fps(fps: u32, buf: &mut [u8; 16]) -> &[u8] {
    let mut pos = 0usize;

    if fps == 0 {
        buf[pos] = b'0';
        pos += 1;
    } else {
        let start = pos;
        let mut n = fps;
        while n > 0 {
            buf[pos] = b'0' + (n % 10) as u8;
            pos += 1;
            n /= 10;
        }
        buf[start..pos].reverse();
    }

    let suffix = b" FPS";
    buf[pos..pos + suffix.len()].copy_from_slice(suffix);
    pos += suffix.len();

    buf[pos] = 0; // NUL terminator
    &buf[..=pos]
}
