// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.ScrollView`.
//!
//! `addView` lives on `ViewGroup` now and routes through
//! [`super::super::view_group`].

use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::scroll_view as lvgl_scroll_view;

pub fn scroll_view_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_scroll_view::create())))
}
