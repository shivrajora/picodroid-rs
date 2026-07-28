// SPDX-License-Identifier: GPL-3.0-only
//! Backend-agnostic graphics trait.
//!
//! `Gfx` abstracts the engine lifecycle plus the cross-widget setters that
//! every widget calls.
//!
//! Today's only impl is `LvglGfx` in `super::super::lvgl`. The trait surface
//! is intentionally backend-neutral: no `lv_obj_t` / `lv_event_t` / RGB565
//! assumptions cross this boundary.
//!
//! Widget *events* do not come through here. An earlier design put a
//! push/pull event model on this trait (`EventKind`, `EventPayload`,
//! `add_event_listener`, `poll_event`); the LVGL backend never implemented
//! it, and the path that shipped instead is the `lv_event_t` trampoline in
//! `lvgl/events.rs` writing into the per-handle tables in
//! `lvgl/listener_map.rs`. The unbuilt surface was deleted rather than left
//! compiling — see the dead-code audit for the reasoning.

use super::handle::Handle;

/// Visibility of a widget. Mirrors Android's `View.VISIBLE` / `INVISIBLE` /
/// `GONE` states; the Java-int decode (0/4/8, Android's values) lives in
/// `graphics::view::set_visibility`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Visibility {
    Visible,
    Invisible,
    Gone,
}

/// Engine-level graphics trait. Handle type is the concrete [`Handle`]
/// newtype (no associated type / no generics) — call sites see a single
/// public type and `&mut dyn Gfx` works without pinning.
pub trait Gfx {
    // ── lifecycle ───────────────────────────────────────────────────────────

    /// Initialize the backend. The backend owns its own framebuffer scratch
    /// (the LVGL impl uses a static RGB565 band buffer sized at compile
    /// time from `hal::display` constants). A future backend with a
    /// different pixel format owns a separately-sized static.
    fn init(&mut self, width: u16, height: u16);

    /// Advance the backend's tick counter and process pending timers /
    /// rendering. Call periodically (~16 ms for 60 fps).
    fn tick(&mut self, ms: u32);

    /// Put the display panel into low-power sleep. Caller is responsible
    /// for stopping `tick()` until `wake()`.
    fn sleep(&mut self);

    /// Wake the display and force a full repaint on next `tick()`.
    fn wake(&mut self);

    /// The active screen / root container handle.
    fn screen(&self) -> Handle;

    // ── cross-widget view ops (every widget calls these) ────────────────────

    fn set_pos(&mut self, h: Handle, x: i32, y: i32);
    fn set_size(&mut self, h: Handle, w: i32, height: i32);
    /// `argb` is a packed `0xAARRGGBB` word; alpha is currently ignored by
    /// the LVGL backend (use [`Self::set_alpha`] for whole-widget opacity).
    fn set_bg_color(&mut self, h: Handle, argb: u32);
    fn set_padding(&mut self, h: Handle, left: i32, top: i32, right: i32, bottom: i32);
    fn set_visibility(&mut self, h: Handle, v: Visibility);
    fn set_enabled(&mut self, h: Handle, on: bool);
    /// `alpha` is 0..=255.
    fn set_alpha(&mut self, h: Handle, alpha: u8);
    fn set_parent(&mut self, h: Handle, parent: Handle);
    fn delete(&mut self, h: Handle);

    // ── ViewGroup ops ───────────────────────────────────────────────────────

    /// Number of children currently parented to `h`.
    ///
    /// There is deliberately no `child_at`: no reverse map from a raw
    /// backend object back to the Java `View` ObjectRef exists, so it could
    /// only ever return null. `ViewGroup.getChildAt` throws
    /// `UnsupportedOperationException` on the Java side instead of reaching
    /// native.
    fn child_count(&self, h: Handle) -> i32;

    /// Detach and delete `child`. The Java side calls this from {@code
    /// ViewGroup.removeView}; LVGL's parent-aware delete walks the tree so
    /// `parent` is informational on the trait surface.
    fn remove_child(&mut self, parent: Handle, child: Handle);

    /// Detach and delete every child of `h`. Maps to LVGL's `lv_obj_clean`.
    fn remove_all_children(&mut self, h: Handle);

    /// Apply a flex-grow factor to `h`. Used by
    /// {@code LinearLayout.LayoutParams.weight} so weighted children expand
    /// to fill remaining space along the layout's main axis.
    fn set_flex_grow(&mut self, h: Handle, weight: i32);

    /// Laid-out geometry (x, y, width, height) in parent-relative pixels,
    /// after forcing any pending layout pass. Backs View.getWidth/getHeight/
    /// getLeft/getTop.
    fn frame(&mut self, h: Handle) -> (i32, i32, i32, i32);

    // Per-widget operations are not on this trait. Each widget module under
    // `graphics/widgets/` calls its LVGL counterpart in
    // `graphics/lvgl/widgets/` directly; only the ops every widget shares
    // (above) are abstracted here.
}
