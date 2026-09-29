// SPDX-License-Identifier: GPL-3.0-only
//! FPS overlay — displays a moving-average frame rate counter on screen.
//!
//! Enabled via `Display.showFps()` from Java.  The LVGL label is created
//! lazily on the first `update()` call so that LVGL is guaranteed to be
//! initialised.

use crate::hal;
use crate::lvgl_ffi::*;
use crate::util::local::Core0;
use core::cell::Cell;

/// Number of frames in the sliding window.
const WINDOW_SIZE: usize = 10;

/// Whether the FPS overlay is enabled.
// SAFETY: widget-layer state, reached only from JVM tasks.
static ENABLED: Core0<Cell<bool>> = unsafe { Core0::new(Cell::new(false)) };

/// Pointer to the LVGL label widget (null until first `update()`).
// SAFETY: widget-layer state, reached only from JVM tasks.
static FPS_LABEL: Core0<Cell<*mut lv_obj_t>> =
    unsafe { Core0::new(Cell::new(core::ptr::null_mut())) };

/// Ring buffer of the last `WINDOW_SIZE` frame durations (microseconds).
static mut FRAME_US: [u64; WINDOW_SIZE] = [16_667; WINDOW_SIZE];

/// Current write position in the ring buffer.
// SAFETY: widget-layer state, reached only from JVM tasks.
static RING_IDX: Core0<Cell<usize>> = unsafe { Core0::new(Cell::new(0)) };

/// Number of samples collected so far (caps at `WINDOW_SIZE`).
// SAFETY: widget-layer state, reached only from JVM tasks.
static SAMPLES: Core0<Cell<usize>> = unsafe { Core0::new(Cell::new(0)) };

/// Timestamp of the previous frame (nanos).
// SAFETY: widget-layer state, reached only from JVM tasks.
static LAST_NANOS: Core0<Cell<i64>> = unsafe { Core0::new(Cell::new(0)) };

/// Frame counter — used to throttle label updates.
// SAFETY: widget-layer state, reached only from JVM tasks.
static FRAME_COUNT: Core0<Cell<u32>> = unsafe { Core0::new(Cell::new(0)) };

/// Mark the overlay as enabled.  The label is created lazily in `update()`.
pub fn enable() {
    ENABLED.set(true);
}

/// Called once per frame from the render loop.  No-op when disabled.
pub fn update() {
    unsafe {
        if !ENABLED.get() {
            return;
        }

        let now = hal::system_clock::elapsed_realtime_nanos();

        // First frame — just record the timestamp; no delta yet.
        if LAST_NANOS.get() == 0 {
            LAST_NANOS.set(now);
            create_label();
            return;
        }

        let delta_ns = now - LAST_NANOS.get();
        LAST_NANOS.set(now);

        let frame_us = (delta_ns / 1000) as u64;
        if frame_us == 0 {
            return;
        }

        // Store in ring buffer.
        FRAME_US[RING_IDX.get()] = frame_us;
        RING_IDX.set((RING_IDX.get() + 1) % WINDOW_SIZE);
        if SAMPLES.get() < WINDOW_SIZE {
            SAMPLES.set(SAMPLES.get() + 1);
        }

        FRAME_COUNT.set(FRAME_COUNT.get() + 1);
        if FRAME_COUNT.get().is_multiple_of(WINDOW_SIZE as u32) {
            let avg_us = FRAME_US[..SAMPLES.get()].iter().sum::<u64>() / SAMPLES.get() as u64;
            let fps = if avg_us > 0 {
                1_000_000u64.checked_div(avg_us).unwrap_or(0) as u32
            } else {
                0
            };
            let mut buf = [0u8; 16];
            let text = format_fps(fps, &mut buf);
            lv_label_set_text(FPS_LABEL.get(), text.as_ptr() as *const _);
        }
    }
}

/// Create the LVGL label in the top-right corner of the screen.
unsafe fn create_label() {
    let screen = lv_screen_active();
    FPS_LABEL.set(lv_label_create(screen));
    lv_label_set_text(FPS_LABEL.get(), c"-- FPS".as_ptr());

    // Position in top-right corner (leave a small margin).
    lv_obj_set_pos(FPS_LABEL.get(), (hal::display::WIDTH - 70) as i32, 2);

    // Green text on dark background.
    lv_obj_set_style_text_color(FPS_LABEL.get(), lv_color_hex(0x00FF00), 0);
    lv_obj_set_style_bg_color(FPS_LABEL.get(), lv_color_hex(0x000000), 0);
    lv_obj_set_style_bg_opa(FPS_LABEL.get(), LV_OPA_COVER, 0);
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
