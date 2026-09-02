// SPDX-License-Identifier: GPL-3.0-only
//! Top-level native dispatch for `picodroid.graphics.*`, `picodroid.view.*`,
//! and `picodroid.widget.*`. Delegates to a [`GraphicsBackend`] so the LVGL
//! implementation can be swapped for a test fake in the future.

mod backend;
mod lvgl_backend;

use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

use crate::shrink_names::c;
pub use backend::GraphicsBackend;
use lvgl_backend::LvglBackend;

/// Returns `true` if `class_name` is `picodroid/view/View` or any of its
/// widget subclasses. Used to route inherited `View` native methods
/// (setSize, setPosition, …) when `invokevirtual` passes the runtime
/// subclass name.
fn is_view(class_name: &str) -> bool {
    matches!(
        class_name,
        c::picodroid_view_View
            | c::picodroid_view_ViewGroup
            | c::picodroid_widget_TextView
            | c::picodroid_widget_Button
            | c::picodroid_widget_CompoundButton
            | c::picodroid_widget_AdapterView
            | c::picodroid_widget_LinearLayout
            | c::picodroid_widget_ProgressBar
            | c::picodroid_widget_Switch
            | c::picodroid_widget_ToggleButton
            | c::picodroid_widget_ListView
            | c::picodroid_widget_ImageView
            | c::picodroid_widget_SeekBar
            | c::picodroid_widget_CheckBox
            | c::picodroid_widget_RadioButton
            | c::picodroid_widget_RadioGroup
            | c::picodroid_widget_ScrollView
            | c::picodroid_widget_FrameLayout
            | c::picodroid_widget_Spinner
            | c::picodroid_widget_DatePicker
            | c::picodroid_widget_TimePicker
            | c::picodroid_widget_SwipeRefreshLayout
            | c::picodroid_widget_EditText
            | c::picodroid_widget_Keyboard
            | c::picodroid_widget_NumberPicker
    )
}

/// Returns `true` if `class_name` is `picodroid/view/ViewGroup` or any of its
/// concrete subclasses. Routes ViewGroup-inherited natives (addView,
/// removeView, getChildCount, …) before the View fallthrough.
fn is_view_group(class_name: &str) -> bool {
    matches!(
        class_name,
        c::picodroid_view_ViewGroup
            | c::picodroid_widget_AdapterView
            | c::picodroid_widget_LinearLayout
            | c::picodroid_widget_FrameLayout
            | c::picodroid_widget_ScrollView
            | c::picodroid_widget_SwipeRefreshLayout
            | c::picodroid_widget_Spinner
            | c::picodroid_widget_RadioGroup
            | c::picodroid_widget_ListView
    )
}

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    // Every View/Display native passes through here — the one place to
    // notice a touch from off the UI thread (see `crate::ui_thread`).
    crate::ui_thread::warn_if_off_ui_thread();
    dispatch_with(&mut LvglBackend, class_name, method_name, ctx)
}

fn dispatch_with<B: GraphicsBackend>(
    be: &mut B,
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    // Class-specific first — these take precedence over inherited View methods
    // so subclass-defined names don't collide with a future View-level setter.
    let class_hit = match class_name {
        c::picodroid_graphics_Display => be.dispatch_display(method_name, ctx),
        c::picodroid_debug_DisplayDebug => be.dispatch_display_debug(method_name, ctx),
        c::picodroid_widget_TextView => be.dispatch_text_view(method_name, ctx),
        c::picodroid_widget_Button => be.dispatch_button(method_name, ctx),
        c::picodroid_widget_LinearLayout => be.dispatch_linear_layout(method_name, ctx),
        c::picodroid_widget_ProgressBar => be.dispatch_progress_bar(method_name, ctx),
        c::picodroid_widget_Switch => be.dispatch_switch(method_name, ctx),
        c::picodroid_widget_ToggleButton => be.dispatch_toggle_button(method_name, ctx),
        c::picodroid_widget_ListView => be.dispatch_list_view(method_name, ctx),
        c::picodroid_widget_NumberPicker => be.dispatch_number_picker(method_name, ctx),
        c::picodroid_widget_SeekBar => be.dispatch_seek_bar(method_name, ctx),
        c::picodroid_widget_CheckBox => be.dispatch_check_box(method_name, ctx),
        c::picodroid_widget_RadioButton => be.dispatch_radio_button(method_name, ctx),
        c::picodroid_widget_ImageView => be.dispatch_image_view(method_name, ctx),
        c::picodroid_widget_ScrollView => be.dispatch_scroll_view(method_name, ctx),
        c::picodroid_widget_FrameLayout => be.dispatch_frame_layout(method_name, ctx),
        c::picodroid_widget_Spinner => be.dispatch_spinner(method_name, ctx),
        c::picodroid_widget_DatePicker => be.dispatch_date_picker(method_name, ctx),
        c::picodroid_widget_TimePicker => be.dispatch_time_picker(method_name, ctx),
        c::picodroid_widget_EditText => be.dispatch_edit_text(method_name, ctx),
        c::picodroid_widget_Toast => be.dispatch_toast(method_name, ctx),
        c::picodroid_widget_Snackbar => be.dispatch_snackbar(method_name, ctx),
        c::picodroid_widget_SwipeRefreshLayout => {
            be.dispatch_swipe_refresh_layout(method_name, ctx)
        }
        c::picodroid_app_AlertDialog => be.dispatch_alert_dialog(method_name, ctx),
        c::picodroid_widget_Keyboard => be.dispatch_keyboard(method_name, ctx),
        c::picodroid_view_ViewPropertyAnimator => be.dispatch_view_animator(method_name, ctx),
        c::picodroid_graphics_drawable_GradientDrawable => {
            be.dispatch_gradient_drawable(method_name, ctx)
        }
        _ => None,
    };
    if class_hit.is_some() {
        return class_hit;
    }

    // Inherited ViewGroup methods (addView, removeView, removeAllViews,
    // getChildCount). Checked between class-specific dispatch and the
    // View fallthrough so a layout's own setOrientation/setSpacing wins
    // first, then ViewGroup's parenting natives, and finally generic
    // View ops resolve.
    if is_view_group(class_name) {
        let vg_hit = be.dispatch_view_group(method_name, ctx);
        if vg_hit.is_some() {
            return vg_hit;
        }
    }

    // Inherited View methods — match on any View subclass.
    if is_view(class_name) {
        return be.dispatch_view(method_name, ctx);
    }

    None
}
