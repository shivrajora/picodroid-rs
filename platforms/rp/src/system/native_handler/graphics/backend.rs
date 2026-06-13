// SPDX-License-Identifier: GPL-3.0-only
//! `GraphicsBackend` trait — abstracts the Java-facing graphics surface.
//!
//! One method per widget class (matching the Android-style API split), each
//! internally routing on the method name. Keeping class-level granularity
//! lets a future sim-fake or test double stub individual widget families
//! without reimplementing the whole dispatch table.
//!
//! The only current impl is [`super::lvgl_backend::LvglBackend`]. LVGL event
//! constants and widget FFI calls stay inside that impl per
//! `project_lvgl_ffi_constants.md`.

use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub type DispatchResult = Option<Result<Option<Value>, JvmError>>;

pub trait GraphicsBackend {
    fn dispatch_display(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;

    /// `picodroid/debug/DisplayDebug` static helpers (calibrate, showFps,
    /// pollTouch). Split out of `dispatch_display` so the public Display
    /// API stays close to Android's surface.
    fn dispatch_display_debug(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;

    /// View methods are inherited by every widget subclass; the caller uses
    /// [`super::is_view`] to decide when to route here. Class name is not
    /// needed by the implementation.
    fn dispatch_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;

    /// ViewGroup methods (addView, removeView, removeAllViews, getChildCount)
    /// are inherited by every layout subclass; [`super::is_view_group`]
    /// gates routing here. Checked between class-specific dispatch and the
    /// View-level fallthrough.
    fn dispatch_view_group(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;

    fn dispatch_text_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_button(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_linear_layout(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_progress_bar(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_switch(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_toggle_button(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_list_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_number_picker(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_seek_bar(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_check_box(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;

    fn dispatch_radio_button(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_image_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_scroll_view(&mut self, method: &str, ctx: &mut NativeContext<'_>)
        -> DispatchResult;
    fn dispatch_frame_layout(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_spinner(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_date_picker(&mut self, method: &str, ctx: &mut NativeContext<'_>)
        -> DispatchResult;
    fn dispatch_time_picker(&mut self, method: &str, ctx: &mut NativeContext<'_>)
        -> DispatchResult;
    fn dispatch_edit_text(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_toast(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_snackbar(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_alert_dialog(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_view_animator(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_gradient_drawable(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_keyboard(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_swipe_refresh_layout(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
}
