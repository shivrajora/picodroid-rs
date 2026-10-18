// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of the click pathway shared by all View subclasses.
//!
//! `Button::create()` constructs an `lv_button` + child `lv_label` for the
//! Button widget specifically. The click event plumbing — ring buffer,
//! `LV_EVENT_CLICKED` trampoline, `performClick` — lives here but is
//! reached through `View.setOnClickListener` / `View.performClick`, so any
//! view (TextView, layout, ImageView) that attaches a click listener
//! becomes clickable. The LVGL CLICKABLE flag and the event callback are
//! attached when a listener is set, so widgets that don't care emit nothing.
//!
//! A queued click names its widget by handle; the drain resolves the Java
//! `View` through the handle table's binding (`handle_table::bound_java`),
//! which the registration made. There is no per-kind map and no cap.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

// ── Click event queues (ring buffers of handles) ────────────────────────────

const CLICK_QUEUE_SIZE: usize = 16;
struct State {
    click_queue: LocalRing<i32, CLICK_QUEUE_SIZE>,
    long_click_queue: LocalRing<i32, CLICK_QUEUE_SIZE>,
}

// SAFETY: filled by LVGL's click callbacks and by natives, drained by the
// framework loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        click_queue: LocalRing::new(0),
        long_click_queue: LocalRing::new(0),
    })
};

// ── Long-click pathway (LV_EVENT_LONG_PRESSED) ──────────────────────────────
//
// Parallel to the click pathway. A view with an OnLongClickListener gets
// the trampoline; LVGL emits LV_EVENT_LONG_PRESSED ~400 ms into a press.

// ── Minimum touch target (touch boards) ─────────────────────────────────────
//
// A clickable view smaller than a fingertip gets its press area grown to the
// board's minimum target, without moving a pixel of layout: Android's 48 dp
// rule, here 7 mm at the panel's real pitch (`[display] dpi`), so a layout
// tuned on a dense panel stays tappable on a coarse one
// (docs/designs/app-portability-2026-10.md D7). The growth is capped so two
// small neighbours' areas do not swallow each other.

#[cfg(has_touch)]
mod touch_target {
    use crate::lvgl_ffi::*;

    /// Seven millimetres in panel pixels, rounded: `dpi * 7 / 25.4`.
    const TARGET_PX: i32 = (crate::board_cfg::display::PHYSICAL_DPI as i32 * 70 + 127) / 254;

    /// Size `obj`'s extended click area now and again whenever its layout
    /// changes. Idempotent: a listener set twice watches once.
    pub(super) fn watch(obj: Obj) {
        lv_obj_remove_event_cb(obj, Some(size_changed_cb));
        add_event_cb(obj, size_changed_cb, LV_EVENT_SIZE_CHANGED);
        apply(obj);
    }

    lv_event_cb! {
        fn size_changed_cb(e) {
            apply(e.target());
        }
    }

    /// Half the shortfall of the shorter edge against the target, on every
    /// side, at most a quarter of the target.
    pub(super) fn growth(width: i32, height: i32) -> i32 {
        let short = width.min(height);
        if short <= 0 {
            return 0;
        }
        ((TARGET_PX - short) / 2).clamp(0, TARGET_PX / 4)
    }

    fn apply(obj: Obj) {
        let grow = growth(lv_obj_get_width(obj), lv_obj_get_height(obj));
        lv_obj_set_ext_click_area(obj, grow);
    }
}

// ── LVGL trampoline ─────────────────────────────────────────────────────────

lv_event_cb! {
    fn view_click_cb(e) {
        let handle = handle_table::handle_of(e.target());
        if handle != 0 {
            STATE.click_queue.push(handle);
        }
    }
}

lv_event_cb! {
    fn view_long_click_cb(e) {
        let handle = handle_table::handle_of(e.target());
        if handle != 0 {
            STATE.long_click_queue.push(handle);
        }
    }
}

// ── LVGL ops (plain-Rust; called from widgets/*.rs Java shims) ──────────────

/// Create `lv_button` with a centered child label set to `text`. Returns
/// the Java-side `nativeHandle`. Click events are not wired here — the
/// `LV_EVENT_CLICKED` trampoline is attached by [`attach_click`] when
/// `View.setOnClickListener` is called.
pub(in crate::graphics) fn create(text: &str) -> i32 {
    lifecycle::screen_ptr().map_or(0, |screen| create_under(screen, text))
}

/// [`create`], under `parent` instead of the screen (the inflater's path).
#[inline(never)]
pub(in crate::graphics) fn create_under(parent: Obj, text: &str) -> i32 {
    let Some(btn) = lv_button_create(parent) else {
        return 0;
    };
    if let Some(label) = lv_label_create(btn) {
        // SAFETY: `label` was just made by `lv_label_create`; the text is
        // NUL-terminated and LVGL copies it.
        super::text_view::with_cstr(text, |p| unsafe { lv_label_set_text(label, p.as_ptr()) });
        lv_obj_center(label);
    }
    handle_table::register(btn)
}

/// Synthetically fire `LV_EVENT_CLICKED` on the underlying widget.
/// No-op if no click listener has been registered (no trampoline attached).
pub(in crate::graphics) fn perform_click(id: i32) {
    if let Some(o) = handle_table::get(id) {
        send_event(o, Notice::Clicked);
    }
}

/// `View.setOnClickListener`'s widget half: set `LV_OBJ_FLAG_CLICKABLE` so
/// any view (not just Button) becomes clickable, and route the widget's
/// `LV_EVENT_CLICKED` to the click queue. Idempotent — a listener set twice
/// attaches one trampoline — and a no-op for a released view. The Java
/// receiver is the handle table's binding, made by the caller.
pub(in crate::graphics) fn attach_click(id: i32) {
    let Some(obj) = handle_table::get(id) else {
        return;
    };
    lv_obj_add_flag(obj, LV_OBJ_FLAG_CLICKABLE);
    #[cfg(has_touch)]
    touch_target::watch(obj);
    lv_obj_remove_event_cb(obj, Some(view_click_cb));
    add_event_cb(obj, view_click_cb, LV_EVENT_CLICKED);
}

/// [`attach_click`] for `View.setOnLongClickListener`: the
/// `LV_EVENT_LONG_PRESSED` trampoline, and CLICKABLE so a press reaches it.
pub(in crate::graphics) fn attach_long_click(id: i32) {
    let Some(obj) = handle_table::get(id) else {
        return;
    };
    lv_obj_add_flag(obj, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_remove_event_cb(obj, Some(view_long_click_cb));
    add_event_cb(obj, view_long_click_cb, LV_EVENT_LONG_PRESSED);
}

/// Synthetically fire `LV_EVENT_LONG_PRESSED` on the underlying widget — the
/// real-input counterpart of `View.performLongClick` is pure Java, but this
/// drives the LVGL long-press path for scripted tests.
pub(in crate::graphics) fn perform_long_press(id: i32) {
    if let Some(o) = handle_table::get(id) {
        send_event(o, Notice::LongPressed);
    }
}

/// Drain one long-click event (the widget's handle) from the queue.
pub fn drain_long_click_queue() -> Option<i32> {
    STATE.long_click_queue.pop()
}

/// Drain one click event (the widget's handle) from the queue.
pub fn drain_click_queue() -> Option<i32> {
    STATE.click_queue.pop()
}

pub fn reset_button_state() {
    STATE.click_queue.clear();
    STATE.long_click_queue.clear();
}
