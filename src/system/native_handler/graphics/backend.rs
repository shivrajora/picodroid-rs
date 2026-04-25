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

    /// View methods are inherited by every widget subclass; the caller uses
    /// [`super::is_view`] to decide when to route here. Class name is not
    /// needed by the implementation.
    fn dispatch_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;

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
    fn dispatch_seek_bar(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_check_box(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_image_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_scroll_view(&mut self, method: &str, ctx: &mut NativeContext<'_>)
        -> DispatchResult;
    fn dispatch_frame_layout(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult;
    fn dispatch_spinner(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_edit_text(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
    fn dispatch_toast(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult;
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
}
