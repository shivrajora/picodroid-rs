// SPDX-License-Identifier: GPL-3.0-only
//! The compat window — docs/designs/app-portability-2026-10.md D5 (as
//! built: amendment A4).
//!
//! An app that declares a design size in its manifest
//! (`<supports-screens design-width=".." design-height=".."/>`) runs at
//! exactly that logical size, whatever the panel. The window is an LVGL
//! object of the design size on the panel-sized screen: every view is
//! created in it and every content root is parented to it, so the app's
//! `match_parent` is the design's width, `Display.getWidth()` reports it,
//! and what the app lays out past its edge is clipped by the window as a
//! real screen edge would. On a larger panel the window is centred and the
//! screen shows through around it; on a smaller one the window starts at
//! the origin and overflows the screen, which pans to the rest exactly as
//! it does for any oversized root (`[layout] overflow`, display.rs): a drag
//! on a touch board, the focus on a four-key one. The display itself stays
//! the panel, so flushes, touches, overlays on the top layer and the panel
//! scroll are untouched. An app without a design size is resizeable and
//! its window is the screen — the default, and today's behaviour.

use core::cell::Cell;
use core::ptr::null_mut;

use crate::hal;
use crate::lvgl_ffi::*;
use crate::util::local::Core0;

/// Where the app's logical window sits on the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    /// Logical size: what the app lays out against and `Display` reports.
    pub width: u16,
    pub height: u16,
    /// The panel pixel the window's origin lands on: zero on an axis where
    /// the window is as large as the panel or larger.
    pub x: u16,
    pub y: u16,
}

impl Window {
    /// The window that is the panel itself.
    pub const fn panel(width: u16, height: u16) -> Self {
        Window {
            width,
            height,
            x: 0,
            y: 0,
        }
    }

    /// Whether this window is something other than the whole `panel`.
    pub fn differs_from(&self, panel: (u16, u16)) -> bool {
        self.x != 0 || self.y != 0 || (self.width, self.height) != panel
    }
}

/// Place a `design`-sized window on a `panel`: centred on an axis where the
/// panel is larger, at the origin where it is smaller (the screen pans).
pub fn fit(design: (u16, u16), panel: (u16, u16)) -> Window {
    let (dw, dh) = design;
    let (pw, ph) = panel;
    Window {
        width: dw,
        height: dh,
        x: pw.saturating_sub(dw) / 2,
        y: ph.saturating_sub(dh) / 2,
    }
}

// ── The live window ──────────────────────────────────────────────────────────

const PANEL: (u16, u16) = (hal::display::WIDTH, hal::display::HEIGHT);

/// The design size the running app declared, if any. Set by `run_app`
/// before the app's first frame, possibly before LVGL exists; applied by
/// [`sync`] once it does.
// SAFETY: widget-layer state, reached only from JVM tasks.
static DESIGN: Core0<Cell<Option<(u16, u16)>>> = unsafe { Core0::new(Cell::new(None)) };

/// The window in effect.
// SAFETY: widget-layer state, reached only from JVM tasks.
static CURRENT: Core0<Cell<Window>> =
    unsafe { Core0::new(Cell::new(Window::panel(PANEL.0, PANEL.1))) };

/// The window object on the screen, null while the window is the screen.
// SAFETY: widget-layer state, reached only from JVM tasks.
static CONTAINER: Core0<Cell<*mut lv_obj_t>> = unsafe { Core0::new(Cell::new(null_mut())) };

/// The app's window: the panel unless a design size applied.
pub fn current() -> Window {
    CURRENT.get()
}

/// The logical size the app lays out against: its design size as soon as
/// `run_app` has read the manifest (before LVGL exists on a cold boot, and
/// before the window object does), else the panel.
pub fn size() -> (u16, u16) {
    DESIGN.get().unwrap_or(PANEL)
}

/// Whether a window other than the panel is in effect.
pub fn active() -> bool {
    !CONTAINER.get().is_null()
}

/// Where new views and content roots belong: the window object, or the
/// screen when the window is the screen.
pub fn content_root() -> *mut lv_obj_t {
    let c = CONTAINER.get();
    if c.is_null() {
        // SAFETY: LVGL is initialised whenever a view is created; the active
        // screen exists for the program's life.
        unsafe { lv_screen_active() }
    } else {
        c
    }
}

/// Put the screen back at its origin. The screen pans to show an oversized
/// root or window (D6, A4), and the pan is the screen's, not the root's: it
/// would otherwise carry over to the next content root — an Activity pushed
/// from a panned screen opened scrolled, showing its blank lower half. Every
/// Activity starts at its own origin on Android, so `setContentView` and the
/// return to a parked root call this. A no-op before LVGL exists.
pub fn reset_pan() {
    if !super::is_initialized() {
        return;
    }
    // SAFETY: LVGL is initialised; the active screen exists for the
    // program's life, and `CONTAINER`, when set, is live until `sync`
    // deletes it. A scroll of zero is a no-op.
    unsafe {
        lv_obj_scroll_to(lv_screen_active(), 0, 0, LV_ANIM_OFF);
        let c = CONTAINER.get();
        if !c.is_null() {
            lv_obj_scroll_to(c, 0, 0, LV_ANIM_OFF);
        }
    }
}

/// What the app about to run declared. `None` is resizeable: the window is
/// the screen. Applied now when LVGL is up, else by [`sync`] when it comes
/// up (`lifecycle::init`).
pub fn set_design(design: Option<(u16, u16)>) {
    DESIGN.set(design);
    sync();
}

/// Make the window the one [`set_design`] asked for. A no-op before LVGL
/// exists and when the window is already right.
pub fn sync() {
    if !super::is_initialized() {
        return;
    }
    let want = match DESIGN.get() {
        Some(design) => fit(design, PANEL),
        None => Window::panel(PANEL.0, PANEL.1),
    };
    let windowed = want.differs_from(PANEL);
    if want == CURRENT.get() && windowed == active() {
        return;
    }
    // LVGL is initialised (checked above) and this runs on the UI task
    // between frames; the active screen exists for the program's life and
    // `CONTAINER`, when set, is an object this function created and nothing
    // else deletes before it does.
    // SAFETY: see above.
    unsafe {
        let scr = lv_screen_active();
        // Content goes to the bare screen first, so no handle names the
        // object about to go (its children, the previous app's views, go
        // with it).
        super::lifecycle::set_content_parent(scr, false);
        let old = CONTAINER.replace(null_mut());
        if !old.is_null() {
            lv_obj_delete(old);
        }
        if windowed {
            let c = lv_obj_create(scr);
            // A bare box: no theme padding, border or corner, nothing
            // painted — the app's root paints the window, the screen shows
            // through around it. Not scrollable, so a drag on it reaches the
            // screen, which is what pans when the window overflows the panel.
            lv_obj_set_style_pad_left(c, 0, 0);
            lv_obj_set_style_pad_right(c, 0, 0);
            lv_obj_set_style_pad_top(c, 0, 0);
            lv_obj_set_style_pad_bottom(c, 0, 0);
            lv_obj_set_style_border_width(c, 0, 0);
            lv_obj_set_style_radius(c, 0, 0);
            lv_obj_set_style_bg_opa(c, 0, 0);
            lv_obj_remove_flag(c, LV_OBJ_FLAG_SCROLLABLE);
            lv_obj_set_size(c, want.width as i32, want.height as i32);
            lv_obj_set_pos(c, want.x as i32, want.y as i32);
            CONTAINER.set(c);
            super::lifecycle::set_content_parent(c, true);
        }
        lv_obj_invalidate(scr);
    }
    CURRENT.set(want);
    if windowed {
        crate::pd_info!(
            "window {}x{} @ ({},{}) on the {}x{} panel",
            want.width,
            want.height,
            want.x,
            want.y,
            PANEL.0,
            PANEL.1
        );
    } else {
        crate::pd_info!("window: the {}x{} panel", PANEL.0, PANEL.1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_smaller_design_is_centred() {
        let w = fit((240, 240), (320, 480));
        assert_eq!(
            w,
            Window {
                width: 240,
                height: 240,
                x: 40,
                y: 120
            }
        );
        assert!(w.differs_from((320, 480)));
    }

    #[test]
    fn the_panel_size_is_the_panel() {
        let w = fit((320, 240), (320, 240));
        assert_eq!(w, Window::panel(320, 240));
        assert!(!w.differs_from((320, 240)));
    }

    #[test]
    fn a_larger_design_starts_at_the_origin() {
        let w = fit((320, 480), (320, 240));
        assert_eq!(w, Window::panel(320, 480));
        assert!(w.differs_from((320, 240)));
        // Mixed: wider than the panel, shorter than it.
        let w = fit((320, 200), (240, 240));
        assert_eq!((w.x, w.y), (0, 20));
    }
}
