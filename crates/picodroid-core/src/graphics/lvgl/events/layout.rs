// SPDX-License-Identifier: GPL-3.0-only
//! Layout requests from the renderer: a custom `ViewGroup` (one made with
//! `ViewGroup(Context)`) hears `LV_EVENT_SIZE_CHANGED` on its widget and
//! asks for its Java `onLayout` to run again. The callback only records the
//! handle — LVGL fires it inside `g.tick()`, where the JVM is unreachable —
//! and the framework loop drains the set once a frame
//! (`lifecycle::widget_events::dispatch_layout_requests`).

use super::*;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

/// How many groups may change size in one frame before a request is
/// dropped; a dropped one is made up for by the next size change, and the
/// Java side re-lays out on `addView` / `requestLayout` anyway.
pub(super) const LAYOUT_QUEUE_SIZE: usize = 16;

pub(super) struct Layout {
    pending: LocalRing<i32, LAYOUT_QUEUE_SIZE>,
}

// SAFETY: filled by LVGL's size-changed callback and drained by the
// framework loop, all on core-0 JVM tasks.
pub(super) static LAYOUT: Core0<Layout> = unsafe {
    Core0::new(Layout {
        pending: LocalRing::new(0),
    })
};

lv_event_cb! {
    fn size_changed_cb(e) {
        let view = super::super::handle_table::handle_of(e.target());
        if view == 0 || LAYOUT.pending.any(|h| h == view) {
            return;
        }
        LAYOUT.pending.push(view);
    }
}

/// `ViewGroup.nativeWatchSize`'s widget half: queue a layout request for
/// `id`'s widget whenever its size changes. Idempotent, and a no-op for a
/// released view.
pub fn attach_size_changed(id: i32) {
    let Some(raw_obj) = super::super::handle_table::get(id) else {
        return;
    };
    lv_obj_remove_event_cb(raw_obj, Some(size_changed_cb));
    add_event_cb(raw_obj, size_changed_cb, LV_EVENT_SIZE_CHANGED);
}

/// Pop one pending layout request (a widget handle), if any.
pub fn drain_layout_request() -> Option<i32> {
    LAYOUT.pending.pop()
}

pub fn reset_layout_requests() {
    LAYOUT.pending.clear();
}
