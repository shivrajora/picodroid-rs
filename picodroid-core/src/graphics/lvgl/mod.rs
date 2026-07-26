// SPDX-License-Identifier: GPL-3.0-only
//! LVGL backend internals.
//!
//! The modules here are the parts with no dependency on the platform crate's
//! app/lifecycle state: the handle table, listener maps, key filtering and
//! debounce, edit-mode navigation, drawables, and the animation engine. The
//! rest of the LVGL layer (the `Gfx` impl, event pump, widgets) still lives
//! in the platform crate and reaches these through re-exports.
//!
//! Nothing outside `lvgl/` should reference `lv_obj_t` / `lv_event_t` /
//! `lv_color_t` directly.

// `lvgl_ffi`'s `extern "C"` block is `cfg(not(test))`, so its drift-check
// tests can run without linking LVGL. These two modules call LVGL functions
// directly and so follow the same gate. Neither has unit tests, so nothing
// is lost; both are covered end-to-end by the sim suite.
#[cfg(not(test))]
pub mod animations;
#[cfg(not(test))]
pub mod drawable;

// The rest are host-testable: they either touch only `LV_KEY_*` constants or
// stub the FFI under test (see `handle_table`'s opaque `lv_obj_t`). Their 38
// unit tests now run in-crate, which is why the platform crate's `#[path]`
// shims for them are gone.
pub mod edit_mode;
pub mod handle_table;
pub mod key_debounce;
pub mod key_filter;
pub mod listener_map;
