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

pub(in crate::system::picodroid::graphics) fn create_indeterminate() -> i32 {
    let ptr = unsafe {
        let s = lv_spinner_create(lifecycle::screen_ptr());
        lv_spinner_set_anim_params(s, SPINNER_ANIM_DURATION_MS, SPINNER_ARC_SWEEP_DEG);
        s
    };
    register_spinner(ptr as usize);
    handle_table::register(ptr)
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
