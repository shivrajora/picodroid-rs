// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of the click pathway shared by all View subclasses.
//!
//! `Button::create()` constructs an `lv_button` + child `lv_label` for the
//! Button widget specifically. The click event plumbing — ring buffer,
//! handle→ObjectRef map, `LV_EVENT_CLICKED` trampoline, `performClick` —
//! lives here but is reached through `View.setOnClickListener` /
//! `View.performClick`, so any view (TextView, layout, ImageView) that
//! attaches a click listener becomes clickable. The LVGL CLICKABLE flag
//! and the event callback are both attached lazily on first listener
//! registration to avoid emitting events on widgets that don't care.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{warn_full, PtrMap, Upsert};
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;

// ── Click event queue (ring buffer) ─────────────────────────────────────────

const CLICK_QUEUE_SIZE: usize = 16;
struct State {
    click_queue: LocalRing<usize, CLICK_QUEUE_SIZE>,
    view_click_map: PtrMap<MAX_CLICK_VIEWS>,
    long_click_queue: LocalRing<usize, CLICK_QUEUE_SIZE>,
    view_long_click_map: PtrMap<MAX_CLICK_VIEWS>,
}

// SAFETY: filled by LVGL's click callbacks and by natives, drained by the
// framework loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        click_queue: LocalRing::new(0),
        view_click_map: PtrMap::new(),
        long_click_queue: LocalRing::new(0),
        view_long_click_map: PtrMap::new(),
    })
};

// ── Handle → Java object mapping (for click dispatch) ───────────────────────

const MAX_CLICK_VIEWS: usize = 32;

// ── Long-click pathway (LV_EVENT_LONG_PRESSED) ──────────────────────────────
//
// Parallel to the click pathway above. A view with an OnLongClickListener
// registers here; LVGL emits LV_EVENT_LONG_PRESSED ~400 ms into a press.

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
    /// changes.
    pub(super) fn watch(obj: Obj) {
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
        let obj = e.target();
        STATE.click_queue.push(obj.addr());
    }
}

lv_event_cb! {
    fn view_long_click_cb(e) {
        let obj = e.target();
        STATE.long_click_queue.push(obj.addr());
    }
}

lv_event_cb! {
    fn click_map_delete_cb(e) {
        let obj = e.target().addr();
        STATE.view_click_map.remove(obj)
    }
}

lv_event_cb! {
    fn long_click_map_delete_cb(e) {
        let obj = e.target().addr();
        STATE.view_long_click_map.remove(obj)
    }
}

// ── LVGL ops (plain-Rust; called from widgets/*.rs Java shims) ──────────────

/// Create `lv_button` with a centered child label set to `text`. Returns
/// the Java-side `nativeHandle`. Click events are not wired here — the
/// `LV_EVENT_CLICKED` trampoline is attached lazily by [`register_click_listener`]
/// on the first `View.setOnClickListener` call.
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

/// Register a Java `View` object as the click-listener target for the
/// given handle. On the first registration for a widget, attaches the
/// `LV_EVENT_CLICKED` trampoline and sets `LV_OBJ_FLAG_CLICKABLE` so any
/// view (not just Button) becomes clickable, plus the `LV_EVENT_DELETE`
/// unregistration hook. An update means the widget is still alive with its
/// callbacks intact — a recycled address always takes the insert path
/// because the old widget's delete callback removed its entry.
pub(in crate::graphics) fn register_click_listener(id: i32, obj_ref: u16) -> bool {
    let Some(obj) = handle_table::get(id) else {
        return true; // a released view: nothing to listen to, nothing full
    };
    match STATE.view_click_map.upsert(obj.addr(), obj_ref) {
        Upsert::Updated => {}
        Upsert::Full => {
            warn_full("view-click");
            return false;
        }
        Upsert::Inserted => {
            lv_obj_add_flag(obj, LV_OBJ_FLAG_CLICKABLE);
            #[cfg(has_touch)]
            touch_target::watch(obj);
            add_event_cb(obj, view_click_cb, LV_EVENT_CLICKED);
            add_event_cb(obj, click_map_delete_cb, LV_EVENT_DELETE);
        }
    }
    true
}

/// Register a Java `View` as the long-click target for `id`. Attaches the
/// `LV_EVENT_LONG_PRESSED` trampoline and flips CLICKABLE on first
/// registration, mirroring [`register_click_listener`].
pub(in crate::graphics) fn register_long_click_listener(id: i32, obj_ref: u16) {
    let Some(obj) = handle_table::get(id) else {
        return;
    };
    match STATE.view_long_click_map.upsert(obj.addr(), obj_ref) {
        Upsert::Updated => {}
        Upsert::Full => warn_full("view-long-click"),
        Upsert::Inserted => {
            lv_obj_add_flag(obj, LV_OBJ_FLAG_CLICKABLE);
            add_event_cb(obj, view_long_click_cb, LV_EVENT_LONG_PRESSED);
            add_event_cb(obj, long_click_map_delete_cb, LV_EVENT_DELETE);
        }
    }
}

/// Synthetically fire `LV_EVENT_LONG_PRESSED` on the underlying widget — the
/// real-input counterpart of `View.performLongClick` is pure Java, but this
/// drives the LVGL long-press path for scripted tests.
pub(in crate::graphics) fn perform_long_press(id: i32) {
    if let Some(o) = handle_table::get(id) {
        send_event(o, Notice::LongPressed);
    }
}

/// Drain one long-click event (raw `lv_obj_t*`) from the queue.
pub fn drain_long_click_queue() -> Option<usize> {
    STATE.long_click_queue.pop()
}

/// Look up the Java `View` object index for a long-clickable widget's pointer.
pub fn lookup_long_click_obj(handle: usize) -> Option<u16> {
    STATE.view_long_click_map.lookup(handle)
}

/// GC roots for the long-click map — same unrooted-View hazard as
/// [`visit_click_listener_roots`].
pub fn visit_long_click_listener_roots(visit: &mut dyn FnMut(u16)) {
    STATE.view_long_click_map.visit(visit)
}

/// Drain one click event (raw `lv_obj_t*` value) from the queue.
pub fn drain_click_queue() -> Option<usize> {
    STATE.click_queue.pop()
}

/// Look up the Java `View` object index for a clickable widget's raw LVGL pointer.
pub fn lookup_button_obj(handle: usize) -> Option<u16> {
    STATE.view_click_map.lookup(handle)
}

pub fn reset_button_state() {
    STATE.view_click_map.reset();
    STATE.click_queue.clear();
    STATE.view_long_click_map.reset();
    STATE.long_click_queue.clear();
}

/// Visit the Java `View` object ref of every view registered for an onClick
/// listener so the GC keeps it alive — a clickable view referenced only by
/// this native map (the app kept no field for it) would otherwise be swept,
/// after which its click silently stops dispatching. See
/// `events::visit_view_listener_roots`.
pub fn visit_click_listener_roots(visit: &mut dyn FnMut(u16)) {
    STATE.view_click_map.visit(visit)
}
