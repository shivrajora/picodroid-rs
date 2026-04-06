use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

/// Returns `true` if `class_name` is `picodroid/view/View` or any of its subclasses.
/// Used by match guards so that inherited native methods (setSize, setPosition, …)
/// dispatch correctly when invokevirtual passes the runtime class name.
fn is_view(class_name: &str) -> bool {
    matches!(
        class_name,
        "picodroid/view/View"
            | "picodroid/widget/TextView"
            | "picodroid/widget/Button"
            | "picodroid/widget/LinearLayout"
            | "picodroid/widget/ProgressBar"
            | "picodroid/widget/Switch"
            | "picodroid/widget/ListView"
            | "picodroid/widget/ImageView"
    )
}

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match (class_name, method_name) {
        // ── Display ───────────────────────────────────────────────────
        ("picodroid/graphics/Display", "getInstance") => Some(
            crate::system::picodroid::graphics::display::get_instance(ctx.objects),
        ),
        ("picodroid/graphics/Display", "setContentView") => Some(
            crate::system::picodroid::graphics::display::set_content_view(ctx.args, ctx.objects),
        ),
        ("picodroid/graphics/Display", "pollTouch") => Some(
            crate::system::picodroid::graphics::display::poll_touch(ctx.objects),
        ),
        ("picodroid/graphics/Display", "update") => {
            Some(crate::system::picodroid::graphics::display::update())
        }
        ("picodroid/graphics/Display", "calibrate") => {
            Some(crate::system::picodroid::graphics::display::calibrate())
        }

        // ── View (base class) ────────────────────────────────────────
        (c, "setPosition") if is_view(c) => Some(
            crate::system::picodroid::graphics::view::set_position(ctx.args, ctx.objects),
        ),
        (c, "setSize") if is_view(c) => Some(crate::system::picodroid::graphics::view::set_size(
            ctx.args,
            ctx.objects,
        )),
        (c, "setBackgroundColor") if is_view(c) => Some(
            crate::system::picodroid::graphics::view::set_bg_color(ctx.args, ctx.objects),
        ),
        (c, "setVisibility") if is_view(c) => Some(
            crate::system::picodroid::graphics::view::set_visibility(ctx.args, ctx.objects),
        ),
        (c, "close") if is_view(c) => Some(crate::system::picodroid::graphics::view::close(
            ctx.args,
            ctx.objects,
        )),

        // ── TextView ─────────────────────────────────────────────────
        ("picodroid/widget/TextView", "nativeCreate") => {
            Some(crate::system::picodroid::graphics::widgets::text_view_native_create())
        }
        ("picodroid/widget/TextView", "setText") => Some(
            crate::system::picodroid::graphics::widgets::text_view_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ),
        ),
        ("picodroid/widget/TextView", "setTextColor") => Some(
            crate::system::picodroid::graphics::widgets::text_view_set_text_color(
                ctx.args,
                ctx.objects,
            ),
        ),

        // ── Button ───────────────────────────────────────────────────
        ("picodroid/widget/Button", "nativeCreate") => Some(
            crate::system::picodroid::graphics::widgets::button_native_create(
                ctx.args,
                ctx.strings,
            ),
        ),
        ("picodroid/widget/Button", "setText") => Some(
            crate::system::picodroid::graphics::widgets::button_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ),
        ),
        ("picodroid/widget/Button", "wasClicked") => Some(
            crate::system::picodroid::graphics::widgets::button_was_clicked(ctx.args, ctx.objects),
        ),
        ("picodroid/widget/Button", "nativeRegisterClickListener") => Some(
            crate::system::picodroid::graphics::widgets::button_register_click_listener(
                ctx.args,
                ctx.objects,
            ),
        ),

        // ── LinearLayout ─────────────────────────────────────────────
        ("picodroid/widget/LinearLayout", "nativeCreate") => {
            Some(crate::system::picodroid::graphics::widgets::linear_layout_native_create())
        }
        ("picodroid/widget/LinearLayout", "addView") => Some(
            crate::system::picodroid::graphics::widgets::linear_layout_add_view(
                ctx.args,
                ctx.objects,
            ),
        ),
        ("picodroid/widget/LinearLayout", "setOrientation") => Some(
            crate::system::picodroid::graphics::widgets::linear_layout_set_orientation(
                ctx.args,
                ctx.objects,
            ),
        ),

        // ── ProgressBar ──────────────────────────────────────────────
        ("picodroid/widget/ProgressBar", "nativeCreate") => {
            Some(crate::system::picodroid::graphics::widgets::progress_bar_native_create())
        }
        ("picodroid/widget/ProgressBar", "setProgress") => Some(
            crate::system::picodroid::graphics::widgets::progress_bar_set_progress(
                ctx.args,
                ctx.objects,
            ),
        ),

        // ── Switch ───────────────────────────────────────────────────
        ("picodroid/widget/Switch", "nativeCreate") => {
            Some(crate::system::picodroid::graphics::widgets::switch_native_create())
        }
        ("picodroid/widget/Switch", "isChecked") => Some(
            crate::system::picodroid::graphics::widgets::switch_is_checked(ctx.args, ctx.objects),
        ),
        ("picodroid/widget/Switch", "setChecked") => Some(
            crate::system::picodroid::graphics::widgets::switch_set_checked(ctx.args, ctx.objects),
        ),

        // ── ListView ─────────────────────────────────────────────────
        ("picodroid/widget/ListView", "nativeCreate") => {
            Some(crate::system::picodroid::graphics::widgets::list_view_native_create())
        }
        ("picodroid/widget/ListView", "addItem") => Some(
            crate::system::picodroid::graphics::widgets::list_view_add_item(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ),
        ),

        // ── ImageView ────────────────────────────────────────────────
        ("picodroid/widget/ImageView", "nativeCreate") => {
            Some(crate::system::picodroid::graphics::widgets::image_view_native_create())
        }
        ("picodroid/widget/ImageView", "setImageSource") => Some(
            crate::system::picodroid::graphics::widgets::image_view_set_src(
                ctx.args,
                ctx.strings,
                ctx.objects,
            ),
        ),

        _ => None,
    }
}
