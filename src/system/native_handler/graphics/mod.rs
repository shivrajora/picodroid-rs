//! Top-level native dispatch for `picodroid.graphics.*`, `picodroid.view.*`,
//! and `picodroid.widget.*`. Delegates to a [`GraphicsBackend`] so the LVGL
//! implementation can be swapped for a test fake in the future.

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
fn is_view(class_name: &str) -> bool {
    matches!(
        class_name,
        "picodroid/view/View"
            | "picodroid/widget/TextView"
            | "picodroid/widget/Button"
            | "picodroid/widget/LinearLayout"
            | "picodroid/widget/ProgressBar"
            | "picodroid/widget/Switch"
            | "picodroid/widget/ToggleButton"
            | "picodroid/widget/ListView"
            | "picodroid/widget/ImageView"
            | "picodroid/widget/SeekBar"
            | "picodroid/widget/CheckBox"
            | "picodroid/widget/ScrollView"
            | "picodroid/widget/FrameLayout"
            | "picodroid/widget/Spinner"
            | "picodroid/widget/EditText"
    )
}

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    dispatch_with(&mut LvglBackend, class_name, method_name, ctx)
}

fn dispatch_with<B: GraphicsBackend>(
    be: &mut B,
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    // Class-specific first — these take precedence over inherited View methods
    // so subclass-defined names don't collide with a future View-level setter.
    let class_hit = match class_name {
        "picodroid/graphics/Display" => be.dispatch_display(method_name, ctx),
        "picodroid/widget/TextView" => be.dispatch_text_view(method_name, ctx),
        "picodroid/widget/Button" => be.dispatch_button(method_name, ctx),
        "picodroid/widget/LinearLayout" => be.dispatch_linear_layout(method_name, ctx),
        "picodroid/widget/ProgressBar" => be.dispatch_progress_bar(method_name, ctx),
        "picodroid/widget/Switch" => be.dispatch_switch(method_name, ctx),
        "picodroid/widget/ToggleButton" => be.dispatch_toggle_button(method_name, ctx),
        "picodroid/widget/ListView" => be.dispatch_list_view(method_name, ctx),
        "picodroid/widget/SeekBar" => be.dispatch_seek_bar(method_name, ctx),
        "picodroid/widget/CheckBox" => be.dispatch_check_box(method_name, ctx),
        "picodroid/widget/ImageView" => be.dispatch_image_view(method_name, ctx),
        "picodroid/widget/ScrollView" => be.dispatch_scroll_view(method_name, ctx),
        "picodroid/widget/FrameLayout" => be.dispatch_frame_layout(method_name, ctx),
        "picodroid/widget/Spinner" => be.dispatch_spinner(method_name, ctx),
        "picodroid/widget/EditText" => be.dispatch_edit_text(method_name, ctx),
        _ => None,
    };
    if class_hit.is_some() {
        return class_hit;
    }

    // Inherited View methods — match on any View subclass.
    if is_view(class_name) {
        return be.dispatch_view(method_name, ctx);
    }

    None
}
