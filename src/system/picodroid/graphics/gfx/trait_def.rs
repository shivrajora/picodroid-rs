//! Backend-agnostic graphics trait.
//!
//! `Gfx` abstracts the engine lifecycle plus the cross-widget setters that
//! every widget calls. Per-widget operations live on the sibling `*Ops`
//! sub-traits in `gfx::widget_ops` and are reached via factory methods on
//! `Gfx`.
//!
//! Today's only impl is `LvglGfx` in `super::super::lvgl`. The trait surface
//! is intentionally backend-neutral: no `lv_obj_t` / `lv_event_t` / RGB565
//! assumptions cross this boundary.

use super::handle::Handle;

/// Visibility of a widget. Mirrors Android's `View.VISIBLE` / `INVISIBLE` /
/// `GONE` ordering (0/1/2).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Visibility {
    Visible,
    Invisible,
    Gone,
}

/// Backend-neutral event kinds delivered to widget listeners.
///
/// LVGL-specific constants (`LV_EVENT_*`) stay inside the LVGL impl; the
/// translation lives in exactly one file (`lvgl/events.rs`). See
/// [project_lvgl_ffi_constants.md](../../../../../../../.claude/projects/-home-shiv-projects-picodroid-rs/memory/project_lvgl_ffi_constants.md)
/// for why centralizing this matters across LVGL version bumps.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum EventKind {
    Click,
    Press,
    Release,
    ValueChanged,
    Focus,
    Blur,
    KeyDown,
}

/// Payload delivered to a `fn(EventPayload)` listener registered via
/// [`Gfx::add_event_listener`]. Plain data — no allocation, no closure.
#[derive(Copy, Clone, Debug)]
pub struct EventPayload {
    pub handle: Handle,
    pub kind: EventKind,
    /// Auxiliary integer (e.g. value-changed: new int value; key-down:
    /// keycode). Backend interprets per `kind`.
    pub aux: i32,
}

/// One pull-mode event drained from the backend's ring buffer. Identical
/// shape to [`EventPayload`] today; kept distinct so push/pull paths can
/// diverge later without breaking either.
#[derive(Copy, Clone, Debug)]
pub struct EventRecord {
    pub handle: Handle,
    pub kind: EventKind,
    pub aux: i32,
}

/// Listener function signature. Plain `fn` — no captures, no `Box<dyn Fn>`,
/// no allocation. Per-handle state is keyed in the backend's slot table.
pub type EventListener = fn(EventPayload);

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

    // ── events ──────────────────────────────────────────────────────────────

    /// Register a push-mode listener. Today's Java path uses
    /// [`Self::poll_event`] instead — see `lvgl/events.rs` for how the LVGL
    /// trampoline routes to one or the other based on registration.
    fn add_event_listener(&mut self, h: Handle, kind: EventKind, cb: EventListener);

    /// Drain one event from the backend's ring buffer. Returns `None` when
    /// the queue is empty.
    fn poll_event(&mut self) -> Option<EventRecord>;

    // ── widget factories ────────────────────────────────────────────────────
    //
    // Per-widget `*Ops` sub-traits and their factories (`fn label(...) ->
    // (Handle, &mut dyn LabelOps)`) are added to this trait per-widget as
    // each widget is migrated in step 7 of the plan. Keeping the trait
    // skeleton minimal here avoids dead trait methods before their impls
    // exist. See `gfx::widget_ops` for sub-trait stubs.
}
