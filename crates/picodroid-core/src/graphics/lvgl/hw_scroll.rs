// SPDX-License-Identifier: GPL-3.0-only
//! Scrolling with the panel's own frame memory instead of a repaint — S4 of
//! docs/designs/scroll-performance-2026-09.md.
//!
//! LVGL's model of a scroll is "move the children, repaint the scroller",
//! and on the touch board a repaint of the 436-row scroller is 60 ms of
//! render and 36 ms of SPI whatever the finger did. The ST7796 can instead
//! rotate a band of its frame memory (`VSCRDEF`/`VSCRSADD`): moving content
//! 47 rows then costs rendering the 47 rows that scrolled in, and nothing
//! else. This module teaches the framework that a scroll can be that.
//!
//! How it fits into LVGL, with none of LVGL patched:
//!
//! * `lv_obj_scroll_by_raw` moves the children, sends `LV_EVENT_SCROLL`, and
//!   only *then* invalidates the whole scroller. [`watch`] hooks the event on
//!   every ScrollView. The handler asks `lvgl/hw_vscroll.c` whether this
//!   scroller may be moved by the panel — full width, nothing on it that
//!   would move wrongly — and, when it may, advances the panel's origin,
//!   stretches every area already queued for redraw inside the band by the
//!   same distance (their pixels moved too), queues repaints for the static
//!   things drawn on top of the band (the scrollbar thumb, an overlay), and
//!   arms one narrowing: the scroller's own invalidation that follows the
//!   event is rewritten, in the display's `LV_EVENT_INVALIDATE_AREA` hook,
//!   to just the rows that scrolled in.
//! * The panel is told at `LV_EVENT_RENDER_START`, right before the first
//!   band of the refresh that draws the strip, so the picture shifts and the
//!   stale rows are overwritten within the same few milliseconds.
//! * Every flush translates display rows to memory lines through the
//!   current rotation ([`flush`]), so a repaint of anything — the header, a
//!   pressed button, a dialog, an entire new screen — lands where the panel
//!   will show it. The rotation therefore never has to be undone; it is
//!   simply the panel's state. It returns to the identity for free whenever
//!   the whole screen is invalidated (a screen change, a wake), because that
//!   repaint overwrites every line anyway.
//! * A step the panel cannot take — sideways motion, a second scroller with
//!   a different band while the first still holds a rotation, a scroller
//!   with a border line inside the band — falls back to what LVGL was going
//!   to do: repaint. The picture stays right either way; only the cost
//!   differs.
//!
//! The sums — where a run of rows lives in memory, which rows a step
//! exposed — are in [`super::hw_scroll_math`], where `cargo test` can reach
//! them. The simulator emulates the panel's registers (`hal/sim/display.rs`),
//! so a wrong wrap is a wrong picture in the sim window, not a bench-only bug.
//!
//! Single-threaded by construction: every entry point runs on the UI task,
//! inside LVGL's own callbacks or its flush.

use core::ffi::c_void;

use crate::hal;
use crate::lvgl_ffi::*;

use super::hw_scroll_math::{advance, exposed_rows, segments, Region};

/// How many static things drawn on top of the band a step will repaint
/// before repainting the band instead. Each one is a small extra render.
const MAX_OVERLAYS: usize = 4;

const EMPTY: lv_area_t = lv_area_t {
    x1: 0,
    y1: 0,
    x2: -1,
    y2: -1,
};

struct State {
    enabled: bool,
    /// The band LVGL's queued invalidations assume, and its rotation.
    region: Option<Region>,
    origin: i32,
    /// What the panel was last told. `None` after a wake: re-sent.
    panel_region: Option<Region>,
    panel_origin: i32,
    /// Armed by a scroll step: the next whole-scroller invalidation that
    /// covers `.0` is narrowed to rows `.1`.
    pending: Option<(Region, (i32, i32))>,
    /// The scroller of the last step and where it stood, for the delta.
    last_obj: *mut lv_obj_t,
    last_scroll: (i32, i32),
    /// Where its vertical scrollbar thumb was drawn, to repaint the ghost.
    last_bar: lv_area_t,
    refused_logged: bool,
}

static mut STATE: State = State {
    enabled: false,
    region: None,
    origin: 0,
    panel_region: None,
    panel_origin: 0,
    pending: None,
    last_obj: core::ptr::null_mut(),
    last_scroll: (0, 0),
    last_bar: EMPTY,
    refused_logged: false,
};

fn state() -> &'static mut State {
    // SAFETY: UI task only; see the module docs.
    unsafe { &mut *core::ptr::addr_of_mut!(STATE) }
}

/// `PICODROID_SIM_HW_VSCROLL=0` runs the simulator with the panel scroll off,
/// for an A/B against the same build; the board's `hw_vscroll = false` is
/// the device's switch.
#[cfg(feature = "sim")]
fn disabled_by_env() -> bool {
    std::env::var("PICODROID_SIM_HW_VSCROLL")
        .map(|v| v == "0")
        .unwrap_or(false)
}
#[cfg(not(feature = "sim"))]
fn disabled_by_env() -> bool {
    false
}

/// Hook the display. Called once from [`super::lifecycle::init`] after the
/// display exists.
pub(super) fn install(disp: *mut lv_display_t) {
    let st = state();
    st.enabled = !disabled_by_env();
    if !st.enabled {
        crate::pd_info!("[hw_vscroll] off (PICODROID_SIM_HW_VSCROLL=0)");
        return;
    }
    let none = core::ptr::null_mut::<c_void>();
    unsafe {
        lv_display_add_event_cb(disp, Some(invalidate_cb), LV_EVENT_INVALIDATE_AREA, none);
        lv_display_add_event_cb(disp, Some(refr_start_cb), LV_EVENT_REFR_START, none);
        lv_display_add_event_cb(disp, Some(render_start_cb), LV_EVENT_RENDER_START, none);
    }
}

/// Let `obj`'s scroll steps use the panel. Called at widget creation for
/// every vertical scroller; whether a given step qualifies is decided per
/// step, so watching a scroller that never will costs one event callback.
pub(super) fn watch(obj: *mut lv_obj_t) {
    let st = state();
    if !st.enabled {
        return;
    }
    unsafe {
        lv_obj_add_event_cb(obj, Some(scroll_cb), LV_EVENT_SCROLL, core::ptr::null_mut());
        lv_obj_add_event_cb(obj, Some(delete_cb), LV_EVENT_DELETE, core::ptr::null_mut());
        // A new scroller stands at the origin, so its very first step has a
        // delta and can take the panel path.
        st.last_obj = obj;
        st.last_scroll = (lv_obj_get_scroll_x(obj), lv_obj_get_scroll_y(obj));
        st.last_bar = EMPTY;
    }
}

/// `LV_EVENT_DELETE` on a watched scroller: its address may be reused by
/// the next one, whose first step must not inherit this one's position.
unsafe extern "C" fn delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    let st = state();
    if st.last_obj == obj {
        st.last_obj = core::ptr::null_mut();
    }
}

/// The panel has been through sleep and wake: forget what it was told, so
/// the next render re-sends the band and the origin before it flushes.
pub(super) fn panel_reset() {
    let st = state();
    st.panel_region = None;
    st.panel_origin = 0;
}

/// Send the panel whatever it has not been told yet. Cheap when nothing
/// changed, which is every flush but the first of a step.
fn sync_panel() {
    let st = state();
    let Some(region) = st.region else { return };
    if st.panel_region != Some(region) {
        hal::display::set_vertical_scroll_area(region.top as u16, region.rows as u16);
        st.panel_region = Some(region);
        st.panel_origin = -1;
    }
    if st.panel_origin != st.origin {
        hal::display::set_vertical_scroll_start((region.top + st.origin) as u16);
        st.panel_origin = st.origin;
    }
}

/// Write one flushed area, display rows `y1..=y2`, to the memory lines the
/// panel shows them at. Replaces `set_window` + `write_pixels` in the flush
/// callback; identical to them while the panel holds no rotation.
pub(super) fn flush(x1: u16, y1: u16, x2: u16, y2: u16, data: &[u8]) {
    sync_panel();
    let st = state();
    let row_bytes = (x2 - x1 + 1) as usize * 2;
    let (segs, n) = segments(y1 as i32, y2 as i32, st.region, st.origin);
    for seg in &segs[..n] {
        let h = (seg.y2 - seg.y1 + 1) as usize;
        hal::display::set_window(
            x1,
            seg.mem_y1 as u16,
            x2,
            (seg.mem_y1 + h as i32 - 1) as u16,
        );
        let start = (seg.y1 - y1 as i32) as usize * row_bytes;
        hal::display::write_pixels(&data[start..start + h * row_bytes]);
    }
}

fn is_valid(a: &lv_area_t) -> bool {
    a.x2 >= a.x1 && a.y2 >= a.y1
}

/// Repaint a static rectangle inside the band after the content under it
/// moved by `dy`: where it is, and where its old pixels went, as one box.
unsafe fn repaint_static(obj: *mut lv_obj_t, a: &lv_area_t, dy: i32, band: &lv_area_t) {
    let mut box_ = *a;
    box_.y1 = (a.y1 + dy.min(0)).max(band.y1);
    box_.y2 = (a.y2 + dy.max(0)).min(band.y2);
    if is_valid(&box_) {
        unsafe { lv_obj_invalidate_area(obj, &box_) };
    }
}

/// Repaint the vertical scrollbar thumb after the content under it moved by
/// `dy`: its old pixels went to `prev + dy`, the new one is drawn at `bar`.
///
/// A thumb that is one flat opaque colour needs only its two ends redone —
/// wherever the old and new rectangles overlap, the pixels are already that
/// colour — so two short boxes replace the thumb-length one, each reaching
/// `radius` rows past the ends to cover the rounded caps. Anything else (a
/// translucent thumb, a bordered one) is repainted end to end.
unsafe fn repaint_thumb(
    obj: *mut lv_obj_t,
    prev: &lv_area_t,
    bar: &lv_area_t,
    dy: i32,
    band: &lv_area_t,
) {
    let radius = unsafe { picodroid_hw_vscroll_thumb_flat_radius(obj) };
    let flat =
        radius >= 0 && is_valid(prev) && is_valid(bar) && prev.x1 == bar.x1 && prev.x2 == bar.x2;
    if !flat {
        if is_valid(prev) {
            unsafe { repaint_static(obj, prev, dy, band) };
        }
        if is_valid(bar) {
            unsafe { lv_obj_invalidate_area(obj, bar) };
        }
        return;
    }
    let cap = radius.min((bar.x2 - bar.x1 + 1) / 2).max(0);
    let (o1, o2) = (prev.y1 + dy, prev.y2 + dy);
    let mut ends = [*bar, *bar];
    ends[0].y1 = o1.min(bar.y1);
    ends[0].y2 = o1.max(bar.y1) + cap;
    ends[1].y1 = o2.min(bar.y2) - cap;
    ends[1].y2 = o2.max(bar.y2);
    for e in &mut ends {
        e.y1 = e.y1.max(band.y1);
        e.y2 = e.y2.min(band.y2);
        if is_valid(e) {
            unsafe { lv_obj_invalidate_area(obj, e) };
        }
    }
}

/// `LV_EVENT_SCROLL` on a watched scroller: the children have moved, the
/// scroller is about to invalidate itself.
unsafe extern "C" fn scroll_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    let st = state();

    let now = unsafe { (lv_obj_get_scroll_x(obj), lv_obj_get_scroll_y(obj)) };
    let same = st.last_obj == obj;
    let prev = st.last_scroll;
    st.last_obj = obj;
    st.last_scroll = now;

    let mut hor_bar = EMPTY;
    let mut bar = EMPTY;
    unsafe { lv_obj_get_scrollbar_area(obj, &mut hor_bar, &mut bar) };
    let prev_bar = core::mem::replace(&mut st.last_bar, bar);

    // A scroller seen for the first time gives no delta; sideways motion the
    // panel cannot do. Both repaint, as LVGL was about to.
    if !same || now.0 != prev.0 {
        return;
    }
    // `lv_obj_get_scroll_y` is the distance scrolled; the children moved the
    // other way by the same amount.
    let dy = prev.1 - now.1;
    if dy == 0 {
        return;
    }

    let mut band = EMPTY;
    let mut overlays = [EMPTY; MAX_OVERLAYS];
    let n = unsafe {
        picodroid_hw_vscroll_region(obj, &mut band, overlays.as_mut_ptr(), MAX_OVERLAYS as i32)
    };
    if n < 0 {
        if !st.refused_logged {
            st.refused_logged = true;
            crate::pd_info!(
                "[hw_vscroll] scroller repaints in full (reason {}: -1 shape, -2 decoration, -3 overlays, -4 transform)",
                n
            );
        }
        return;
    }
    let region = Region {
        top: band.y1,
        rows: band.y2 - band.y1 + 1,
    };
    // Nothing of the old picture survives a step this large.
    if dy.abs() >= region.rows {
        return;
    }

    if st.region != Some(region) {
        if st.origin != 0 {
            // The panel still holds another band's rotation. Back to the
            // identity, which repainting the whole screen through it makes
            // true; this step is that repaint.
            st.origin = 0;
            st.region = Some(region);
            unsafe { lv_obj_invalidate(lv_screen_active()) };
            return;
        }
        st.region = Some(region);
    }

    let disp = unsafe { lv_display_get_default() };
    unsafe { picodroid_hw_vscroll_shift_pending(disp, &band, dy) };
    st.origin = advance(st.origin, region.rows, dy);

    // What did not move: the scrollbar thumb, then and now, and whatever is
    // drawn over the band.
    let n = n as usize;
    for a in &overlays[..n] {
        unsafe { repaint_static(obj, a, dy, &band) };
    }
    if is_valid(&hor_bar) {
        unsafe { repaint_static(obj, &hor_bar, dy, &band) };
    }
    unsafe { repaint_thumb(obj, &prev_bar, &bar, dy, &band) };

    // And the rows that scrolled in: the scroller's own invalidation, which
    // arrives next, becomes just these.
    st.pending = Some((region, exposed_rows(region, dy)));
}

/// `LV_EVENT_INVALIDATE_AREA` on the display, with the area LVGL is about to
/// queue, before it is queued.
unsafe extern "C" fn invalidate_cb(e: *mut lv_event_t) {
    let area = unsafe { &mut *(lv_event_get_param(e) as *mut lv_area_t) };
    let st = state();
    let (w, h) = (hal::display::WIDTH as i32, hal::display::HEIGHT as i32);
    let full_width = area.x1 <= 0 && area.x2 >= w - 1;

    if full_width && area.y1 <= 0 && area.y2 >= h - 1 {
        // Everything repaints, through the identity: the panel ends this
        // frame unrotated at no extra cost.
        st.origin = 0;
        st.pending = None;
        return;
    }
    if let Some((expect, (y1, y2))) = st.pending {
        let covers_band = area.y1 <= expect.top && area.y2 >= expect.top + expect.rows - 1;
        if full_width && covers_band {
            area.y1 = y1;
            area.y2 = y2;
            st.pending = None;
        }
    }
}

/// `LV_EVENT_REFR_START`: a narrowing that was never consumed must not
/// outlive the frame that armed it.
unsafe extern "C" fn refr_start_cb(_e: *mut lv_event_t) {
    state().pending = None;
}

/// `LV_EVENT_RENDER_START`: the panel shifts now, and the rows that shift
/// exposes are the first thing the render that follows overwrites.
unsafe extern "C" fn render_start_cb(_e: *mut lv_event_t) {
    sync_panel();
}
