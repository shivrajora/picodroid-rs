// SPDX-License-Identifier: GPL-3.0-only
//! Top-level native dispatch for `picodroid.graphics.*`, `picodroid.view.*`,
//! and `picodroid.widget.*`. Delegates to a [`GraphicsBackend`] so the LVGL
//! implementation can be swapped for a test fake in the future.

#![deny(clippy::undocumented_unsafe_blocks)]

use crate::shrink_names::{c, ch};
use pico_jvm::NativeKey;

mod backend;
mod lvgl_backend;

use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub use backend::GraphicsBackend;
use lvgl_backend::LvglBackend;

/// Returns `true` if `class_name` is `picodroid/view/View` or any of its
/// widget subclasses. Used to route inherited `View` native methods
/// (setSize, setPosition, …) when `invokevirtual` passes the runtime
/// subclass name.
fn is_view(class: u32) -> bool {
    matches!(
        class,
        ch::picodroid_view_View
            | ch::picodroid_view_ViewGroup
            | ch::picodroid_widget_TextView
            | ch::picodroid_widget_Button
            | ch::picodroid_widget_CompoundButton
            | ch::picodroid_widget_AdapterView
            | ch::picodroid_widget_LinearLayout
            | ch::picodroid_widget_ProgressBar
            | ch::picodroid_widget_CircularProgressIndicator
            | ch::picodroid_widget_Switch
            | ch::picodroid_widget_ToggleButton
            | ch::picodroid_widget_ListView
            | ch::picodroid_widget_ImageView
            | ch::picodroid_widget_SeekBar
            | ch::picodroid_widget_CheckBox
            | ch::picodroid_widget_RadioButton
            | ch::picodroid_widget_RadioGroup
            | ch::picodroid_widget_ScrollView
            | ch::picodroid_widget_FrameLayout
            | ch::picodroid_widget_GridLayout
            | ch::picodroid_widget_Spinner
            | ch::picodroid_widget_DatePicker
            | ch::picodroid_widget_TimePicker
            | ch::picodroid_widget_SwipeRefreshLayout
            | ch::picodroid_widget_EditText
            | ch::picodroid_widget_Keyboard
            | ch::picodroid_widget_NumberPicker
    )
}

/// Returns `true` if `class_name` is `picodroid/view/ViewGroup` or any of its
/// concrete subclasses. Routes ViewGroup-inherited natives (addView,
/// removeView, getChildCount, …) before the View fallthrough.
fn is_view_group(class: u32) -> bool {
    matches!(
        class,
        ch::picodroid_view_ViewGroup
            | ch::picodroid_widget_AdapterView
            | ch::picodroid_widget_LinearLayout
            | ch::picodroid_widget_FrameLayout
            | ch::picodroid_widget_GridLayout
            | ch::picodroid_widget_ScrollView
            | ch::picodroid_widget_SwipeRefreshLayout
            | ch::picodroid_widget_Spinner
            | ch::picodroid_widget_RadioGroup
            | ch::picodroid_widget_ListView
    )
}

pub fn dispatch(
    key: NativeKey,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let result = dispatch_with(&mut LvglBackend, key, ctx);
    // Every View/Display native passes through here — the one place to
    // notice a touch from off the UI thread (see `crate::ui_thread`). Only
    // once the class is known to be ours: every native the sub-handlers
    // before this one decline is offered here too, and a warning ahead of
    // the match called a worker's `Thread.currentThread()` a View touch.
    if result.is_some() {
        crate::ui_thread::warn_if_off_ui_thread();
    }
    result
}

/// Why the receiver (`args[0]`) has no widget behind it, if it has none.
enum Dead {
    /// Handle 0: `ViewGroup.removeView` (or a close) freed it.
    Released,
    /// A non-zero handle the table no longer holds: the widget went with
    /// its Activity's screen, and the Java object outlived it.
    Stale,
}

fn dead_receiver(ctx: &NativeContext<'_>) -> Option<Dead> {
    let Some(Value::ObjectRef(idx)) = ctx.args.first() else {
        return None;
    };
    match ctx
        .objects
        .get_field(*idx, crate::graphics::fields::view::NATIVE_HANDLE)
    {
        Some(Value::Int(0)) => Some(Dead::Released),
        Some(Value::Int(h)) if !crate::graphics::lvgl::handle_table::is_live(h) => {
            Some(Dead::Stale)
        }
        _ => None,
    }
}

/// Whether the receiver (`args[0]`) is a view whose handle is 0.
fn released_receiver(ctx: &NativeContext<'_>) -> bool {
    matches!(dead_receiver(ctx), Some(Dead::Released))
}

fn dispatch_with<B: GraphicsBackend>(
    be: &mut B,
    key: NativeKey,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    // A released view — handle 0, `ViewGroup.removeView` freed its widget —
    // has no widget for any View arm: an IllegalStateException the app can
    // read, never a null or dangling widget pointer for LVGL (on the boards
    // without a generational handle table the freed pointer was reused and
    // the touch kit hung re-parenting it; QA 2026-09-13). The same for a
    // stale handle: a finished Activity's Views keep non-zero handles the
    // table no longer resolves, and a main-executor task it left running
    // (`sw.setChecked`, `editText.setText`) handed LVGL the null lookup
    // unchecked — a sanitizer panic in the sim, a HardFault on device. The
    // View family's only static native, `nativeCreate`, has no receiver to
    // check.
    if is_view(key.class) {
        if released_receiver(ctx) {
            return Some(Err(super::throw_exception(
                ctx,
                c::java_lang_IllegalStateException,
                "this view was released by removeView; picodroid frees a removed view, create a new one",
            )));
        }
        if matches!(dead_receiver(ctx), Some(Dead::Stale)) {
            return Some(Err(super::throw_exception(
                ctx,
                c::java_lang_IllegalStateException,
                "this view's widget is gone: its Activity finished; drop the reference and build the view again",
            )));
        }
    }

    // Class-specific first — these take precedence over inherited View methods
    // so subclass-defined names don't collide with a future View-level setter.
    let class_hit = match key.class {
        ch::picodroid_graphics_Display => be.dispatch_display(key.method, ctx),
        ch::picodroid_debug_DisplayDebug => be.dispatch_display_debug(key.method, ctx),
        ch::picodroid_view_LayoutInflater => be.dispatch_layout_inflater(key.method, ctx),
        ch::picodroid_widget_TextView => be.dispatch_text_view(key.method, ctx),
        ch::picodroid_widget_Button => be.dispatch_button(key.method, ctx),
        ch::picodroid_widget_LinearLayout => be.dispatch_linear_layout(key.method, ctx),
        ch::picodroid_widget_ProgressBar => be.dispatch_progress_bar(key.method, ctx),
        ch::picodroid_widget_CircularProgressIndicator => {
            be.dispatch_circular_progress_indicator(key.method, ctx)
        }
        ch::picodroid_widget_Switch => be.dispatch_switch(key.method, ctx),
        ch::picodroid_widget_ToggleButton => be.dispatch_toggle_button(key.method, ctx),
        ch::picodroid_widget_ListView => be.dispatch_list_view(key.method, ctx),
        ch::picodroid_widget_NumberPicker => be.dispatch_number_picker(key.method, ctx),
        ch::picodroid_widget_SeekBar => be.dispatch_seek_bar(key.method, ctx),
        ch::picodroid_widget_CheckBox => be.dispatch_check_box(key.method, ctx),
        ch::picodroid_widget_RadioButton => be.dispatch_radio_button(key.method, ctx),
        ch::picodroid_widget_ImageView => be.dispatch_image_view(key.method, ctx),
        ch::picodroid_widget_ScrollView => be.dispatch_scroll_view(key.method, ctx),
        ch::picodroid_widget_FrameLayout => be.dispatch_frame_layout(key.method, ctx),
        ch::picodroid_widget_GridLayout => be.dispatch_grid_layout(key.method, ctx),
        ch::picodroid_widget_Spinner => be.dispatch_spinner(key.method, ctx),
        ch::picodroid_widget_DatePicker => be.dispatch_date_picker(key.method, ctx),
        ch::picodroid_widget_TimePicker => be.dispatch_time_picker(key.method, ctx),
        ch::picodroid_widget_EditText => be.dispatch_edit_text(key.method, ctx),
        ch::picodroid_widget_Toast => be.dispatch_toast(key.method, ctx),
        ch::picodroid_widget_Snackbar => be.dispatch_snackbar(key.method, ctx),
        ch::picodroid_widget_SwipeRefreshLayout => {
            be.dispatch_swipe_refresh_layout(key.method, ctx)
        }
        ch::picodroid_app_AlertDialog => be.dispatch_alert_dialog(key.method, ctx),
        ch::picodroid_widget_Keyboard => be.dispatch_keyboard(key.method, ctx),
        ch::picodroid_view_ViewPropertyAnimator => be.dispatch_view_animator(key.method, ctx),
        ch::picodroid_graphics_drawable_GradientDrawable => {
            be.dispatch_gradient_drawable(key.method, ctx)
        }
        #[cfg(has_canvas)]
        ch::picodroid_graphics_Canvas => be.dispatch_canvas(key.method, ctx),
        #[cfg(has_canvas)]
        ch::picodroid_graphics_Paint => be.dispatch_paint(key.method, ctx),
        #[cfg(has_multi_app)]
        ch::picodroid_graphics_drawable_BitmapDrawable => {
            be.dispatch_bitmap_drawable(key.method, ctx)
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
    if is_view_group(key.class) {
        let vg_hit = be.dispatch_view_group(key.method, ctx);
        if vg_hit.is_some() {
            return vg_hit;
        }
    }

    // Inherited View methods — match on any View subclass.
    if is_view(key.class) {
        return be.dispatch_view(key.method, ctx);
    }

    None
}
