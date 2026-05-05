// SPDX-License-Identifier: GPL-3.0-only
//! LVGL-specific widget impls.
//!
//! Each module mirrors the corresponding `widgets/<name>.rs` Java-binding
//! shim and owns the `lv_<x>_create` + setter calls + event-callback
//! trampolines for that widget. A future second backend would mirror this
//! tree at e.g. `embedded_graphics/widgets/<name>.rs`.

pub mod alert_dialog;
pub mod button;
pub mod check_box;
pub mod date_picker;
pub mod edit_text;
pub mod frame_layout;
pub mod image_view;
pub mod keyboard;
pub mod linear_layout;
pub mod list_view;
pub mod progress_bar;
pub mod scroll_view;
pub mod seek_bar;
pub mod snackbar;
pub mod spinner;
pub mod swipe_refresh_layout;
pub mod switch;
pub mod text_view;
pub mod time_picker;
pub mod toast;
pub mod toggle_button;
