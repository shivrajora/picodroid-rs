// SPDX-License-Identifier: GPL-3.0-only
//! Java-binding shim for `picodroid.widget.FrameLayout`.
//!
//! `addView` lives on `ViewGroup` now and routes through
//! [`super::super::view_group`]; FrameLayout itself only owns its native
//! constructor.

use pico_jvm::types::{JvmError, Value};

use super::super::lvgl::widgets::frame_layout as lvgl_frame_layout;

pub fn frame_layout_native_create() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(lvgl_frame_layout::create())))
}
