// SPDX-License-Identifier: GPL-3.0-only
//! `View.setKeepScreenOn`: which live views hold the display on
//! (docs/designs/app-portability-2026-10.md K5). A set keyed on the raw
//! `lv_obj_t*`, so a view deleted with its Activity releases its hold
//! through `LV_EVENT_DELETE` and an appliance that finishes leaves the idle
//! timer free to doze the panel again. `power.rs` counts the holds; this
//! module only keeps them matched to views.

use crate::lvgl_ffi::*;

use super::handle_table;
use crate::util::local::Core0;
use crate::util::local_set::LocalSet;

/// How many views may hold the screen at once: an app flags a root, not a
/// tree.
const MAX_HOLDERS: usize = 8;
// SAFETY: a registry reached only from JVM tasks.
static HOLDERS: Core0<LocalSet<usize, MAX_HOLDERS>> = unsafe { Core0::new(LocalSet::new(0)) };

lv_event_cb! {
    fn holder_delete_cb(e) {
        release(e.target().addr());
    }
}

fn release(obj: usize) {
    if HOLDERS.remove(obj) {
        crate::power::hold_screen(false);
    }
}

/// `View.setKeepScreenOn(flag)` on the view behind `id`.
pub(in crate::graphics) fn set(id: i32, on: bool) {
    let Some(raw) = handle_table::get(id) else {
        return;
    };
    let obj = raw.addr();
    if !on {
        release(obj);
        // The delete hook stays attached: harmless, and the view may hold
        // again.
        return;
    }
    if HOLDERS.contains(obj) {
        return;
    }
    if !HOLDERS.insert(obj) {
        crate::pd_warn!(
            "keep-screen-on: {} views already hold the screen",
            MAX_HOLDERS
        );
        return;
    }
    crate::power::hold_screen(true);
    add_event_cb(raw, holder_delete_cb, LV_EVENT_DELETE);
}

/// Between app runs: the views are gone with their screen, and `power.rs`
/// has dropped the count.
pub fn reset() {
    HOLDERS.clear();
}
