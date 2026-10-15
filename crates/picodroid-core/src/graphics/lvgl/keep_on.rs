// SPDX-License-Identifier: GPL-3.0-only
//! `View.setKeepScreenOn`: which live views hold the display on
//! (docs/designs/app-portability-2026-10.md K5). A registry keyed on the raw
//! `lv_obj_t*`, as the listener maps are, so a view deleted with its Activity
//! releases its hold through `LV_EVENT_DELETE` and an appliance that finishes
//! leaves the idle timer free to doze the panel again. `power.rs` counts the
//! holds; this module only keeps them matched to views.

use crate::lvgl_ffi::*;

use super::handle_table;
use super::listener_map::{warn_full, PtrMap, Upsert};
use crate::util::local::Core0;

/// How many views may hold the screen at once: an app flags a root, not a
/// tree.
const MAX_HOLDERS: usize = 8;
// SAFETY: a listener registry, reached only from JVM tasks.
static HOLDERS: Core0<PtrMap<MAX_HOLDERS>> = unsafe { Core0::new(PtrMap::new()) };

lv_event_cb! {
    fn holder_delete_cb(e) {
        release(e.target().addr());
    }
}

fn release(obj: usize) {
    if HOLDERS.lookup(obj).is_some() {
        HOLDERS.remove(obj);
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
    match HOLDERS.upsert(obj, 1) {
        Upsert::Updated => {}
        Upsert::Full => warn_full("keep-screen-on"),
        Upsert::Inserted => {
            crate::power::hold_screen(true);
            add_event_cb(raw, holder_delete_cb, LV_EVENT_DELETE);
        }
    }
}

/// Between app runs: the views are gone with their screen, and `power.rs`
/// has dropped the count.
pub fn reset() {
    HOLDERS.reset();
}
