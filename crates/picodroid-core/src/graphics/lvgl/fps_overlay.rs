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

struct State {
    /// Whether the FPS overlay is enabled.
    enabled: Cell<bool>,
    /// Pointer to the LVGL label widget (null until first `update()`).
    fps_label: Cell<*mut lv_obj_t>,
    /// Current write position in the ring buffer.
    ring_idx: Cell<usize>,
    /// Number of samples collected so far (caps at `WINDOW_SIZE`).
    samples: Cell<usize>,
    /// Timestamp of the previous frame (nanos).
    last_nanos: Cell<i64>,
    /// Frame counter — used to throttle label updates.
    frame_count: Cell<u32>,
    /// Ring buffer of the last `WINDOW_SIZE` frame durations (microseconds).
    frame_us: [Cell<u64>; WINDOW_SIZE],
}

// SAFETY: enabled by a native and updated by the frame loop, both on core-0
// JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        enabled: Cell::new(false),
        fps_label: Cell::new(core::ptr::null_mut()),
        ring_idx: Cell::new(0),
        samples: Cell::new(0),
        last_nanos: Cell::new(0),
        frame_count: Cell::new(0),
        // Only the first `samples` entries are averaged, each written first.
        frame_us: [const { Cell::new(0) }; WINDOW_SIZE],
    })
};

/// Mark the overlay as enabled.  The label is created lazily in `update()`.
pub fn enable() {
    STATE.enabled.set(true);
}

/// Called once per frame from the render loop.  No-op when disabled.
pub fn update() {
    unsafe {
        if !STATE.enabled.get() {
            return;
        }

        let now = hal::system_clock::elapsed_realtime_nanos();

        // First frame — just record the timestamp; no delta yet.
        if STATE.last_nanos.get() == 0 {
            STATE.last_nanos.set(now);
            create_label();
            return;
        }

        let delta_ns = now - STATE.last_nanos.get();
        STATE.last_nanos.set(now);

        let frame_us = (delta_ns / 1000) as u64;
        if frame_us == 0 {
            return;
        }

        // Store in ring buffer.
        STATE.frame_us[STATE.ring_idx.get()].set(frame_us);
        STATE.ring_idx.set((STATE.ring_idx.get() + 1) % WINDOW_SIZE);
        if STATE.samples.get() < WINDOW_SIZE {
            STATE.samples.set(STATE.samples.get() + 1);
        }

        STATE.frame_count.set(STATE.frame_count.get() + 1);
        if STATE.frame_count.get().is_multiple_of(WINDOW_SIZE as u32) {
            let samples = STATE.samples.get();
            let avg_us =
                STATE.frame_us[..samples].iter().map(Cell::get).sum::<u64>() / samples as u64;
            let fps = if avg_us > 0 {
                1_000_000u64.checked_div(avg_us).unwrap_or(0) as u32
            } else {
                0
            };
            let mut buf = [0u8; 16];
            let text = format_fps(fps, &mut buf);
            lv_label_set_text(STATE.fps_label.get(), text.as_ptr() as *const _);
        }
    }
}

/// Create the LVGL label in the top-right corner of the window, on the
/// overlay layer so it rides above every screen and never pans with one.
unsafe fn create_label() {
    let layer = lv_layer_top();
    STATE.fps_label.set(lv_label_create(layer));
    lv_label_set_text(STATE.fps_label.get(), c"-- FPS".as_ptr());

    // Position in top-right corner (leave a small margin).
    let win_w = lv_display_get_horizontal_resolution(lv_display_get_default());
    lv_obj_set_pos(STATE.fps_label.get(), win_w - 70, 2);

    // Green text on dark background.
    lv_obj_set_style_text_color(STATE.fps_label.get(), lv_color_hex(0x00FF00), 0);
    lv_obj_set_style_bg_color(STATE.fps_label.get(), lv_color_hex(0x000000), 0);
    lv_obj_set_style_bg_opa(STATE.fps_label.get(), LV_OPA_COVER, 0);
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
