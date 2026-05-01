//! LVGL-backed [`GraphicsBackend`] implementation.
//!
//! Each method matches on the Java method name and delegates to the leaf
//! handlers in `crate::system::picodroid::graphics::{display, view, widgets}`,
//! which own the LVGL FFI calls and `handle_table` routing. This indirection
//! keeps LVGL-specific code isolated to the impl block.

use pico_jvm::NativeContext;

use super::backend::{DispatchResult, GraphicsBackend};

use crate::system::picodroid::graphics::{display, view, widgets};

pub struct LvglBackend;

impl GraphicsBackend for LvglBackend {
    fn dispatch_display(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "getInstance" => Some(display::get_instance(ctx.objects)),
            "setContentView" => Some(display::set_content_view(ctx.args, ctx.objects)),
            "pollTouch" => Some(display::poll_touch(ctx.objects)),
            "update" => Some(display::update()),
            "calibrate" => Some(display::calibrate()),
            "showFps" => Some(display::show_fps()),
            _ => None,
        }
    }

    fn dispatch_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "setPosition" => Some(view::set_position(ctx.args, ctx.objects)),
            "setSize" => Some(view::set_size(ctx.args, ctx.objects)),
            "setBackgroundColor" => Some(view::set_bg_color(ctx.args, ctx.objects)),
            "setVisibility" => Some(view::set_visibility(ctx.args, ctx.objects)),
            "setPadding" => Some(view::set_padding(ctx.args, ctx.objects)),
            "setEnabled" => Some(view::set_enabled(ctx.args, ctx.objects)),
            "setAlpha" => Some(view::set_alpha(ctx.args, ctx.objects)),
            "close" => Some(view::close(ctx.args, ctx.objects)),
            "nativeRegisterKeyListener" => Some(view::register_key_listener(ctx.args, ctx.objects)),
            "nativeRegisterTouchListener" => {
                Some(view::register_touch_listener(ctx.args, ctx.objects))
            }
            "nativeRegisterSwipeListener" => {
                Some(view::register_swipe_listener(ctx.args, ctx.objects))
            }
            _ => None,
        }
    }

    fn dispatch_text_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::text_view_native_create()),
            "setText" => Some(widgets::text_view_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            "setTextColor" => Some(widgets::text_view_set_text_color(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_button(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::button_native_create(ctx.args, ctx.strings)),
            "setText" => Some(widgets::button_set_text(ctx.args, ctx.strings, ctx.objects)),
            "wasClicked" => Some(widgets::button_was_clicked(ctx.args, ctx.objects)),
            "nativeRegisterClickListener" => Some(widgets::button_register_click_listener(
                ctx.args,
                ctx.objects,
            )),
            "performClick" => Some(widgets::button_perform_click(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_linear_layout(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::linear_layout_native_create()),
            "addView" => Some(widgets::linear_layout_add_view(ctx.args, ctx.objects)),
            "setOrientation" => Some(widgets::linear_layout_set_orientation(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_progress_bar(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::progress_bar_native_create()),
            "nativeCreateIndeterminate" => {
                Some(widgets::progress_bar_native_create_indeterminate())
            }
            "setProgress" => Some(widgets::progress_bar_set_progress(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_switch(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::switch_native_create()),
            "isChecked" => Some(widgets::switch_is_checked(ctx.args, ctx.objects)),
            "setChecked" => Some(widgets::switch_set_checked(ctx.args, ctx.objects)),
            "toggle" => Some(widgets::switch_toggle(ctx.args, ctx.objects)),
            "nativeRegisterCheckedChangeListener" => Some(
                widgets::switch_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            "performCheckedChange" => Some(widgets::switch_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_toggle_button(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::toggle_button_native_create()),
            "nativeCreateWithText" => Some(widgets::toggle_button_native_create_with_text(
                ctx.args,
                ctx.strings,
            )),
            "isChecked" => Some(widgets::toggle_button_is_checked(ctx.args, ctx.objects)),
            "setChecked" => Some(widgets::toggle_button_set_checked(ctx.args, ctx.objects)),
            "toggle" => Some(widgets::toggle_button_toggle(ctx.args, ctx.objects)),
            "setTextOn" => Some(widgets::toggle_button_set_text_on(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            "setTextOff" => Some(widgets::toggle_button_set_text_off(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            "nativeRegisterCheckedChangeListener" => Some(
                widgets::toggle_button_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            "performCheckedChange" => Some(widgets::toggle_button_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_list_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::list_view_native_create()),
            "addItem" => Some(widgets::list_view_add_item(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_seek_bar(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::seek_bar_native_create()),
            "nativeCreateWithMax" => Some(widgets::seek_bar_native_create_with_max(ctx.args)),
            "setMax" => Some(widgets::seek_bar_set_max(ctx.args, ctx.objects)),
            "setProgress" => Some(widgets::seek_bar_set_progress(ctx.args, ctx.objects)),
            "getProgress" => Some(widgets::seek_bar_get_progress(ctx.args, ctx.objects)),
            "nativeRegisterChangeListener" => Some(widgets::seek_bar_register_change_listener(
                ctx.args,
                ctx.objects,
            )),
            "performProgressChange" => Some(widgets::seek_bar_perform_progress_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_check_box(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::check_box_native_create()),
            "setText" => Some(widgets::check_box_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            "isChecked" => Some(widgets::check_box_is_checked(ctx.args, ctx.objects)),
            "setChecked" => Some(widgets::check_box_set_checked(ctx.args, ctx.objects)),
            "nativeRegisterCheckedChangeListener" => Some(
                widgets::check_box_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            "performCheckedChange" => Some(widgets::check_box_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_image_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::image_view_native_create()),
            "setImageSource" => Some(widgets::image_view_set_src(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            "setScaleType" => Some(widgets::image_view_set_scale_type(ctx.args, ctx.objects)),
            "setTint" => Some(widgets::image_view_set_tint(ctx.args, ctx.objects)),
            "setScale" => Some(widgets::image_view_set_scale(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_scroll_view(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::scroll_view_native_create()),
            "addView" => Some(widgets::scroll_view_add_view(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_frame_layout(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::frame_layout_native_create()),
            "addView" => Some(widgets::frame_layout_add_view(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_date_picker(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::date_picker_native_create()),
            "setDate" => Some(widgets::date_picker_set_date(ctx.args, ctx.objects)),
            "getYear" => Some(widgets::date_picker_get_year(ctx.args, ctx.objects)),
            "getMonth" => Some(widgets::date_picker_get_month(ctx.args, ctx.objects)),
            "getDay" => Some(widgets::date_picker_get_day(ctx.args, ctx.objects)),
            "nativeRegisterDateChangedListener" => Some(widgets::date_picker_register_listener(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_time_picker(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::time_picker_native_create()),
            "setTime" => Some(widgets::time_picker_set_time(ctx.args, ctx.objects)),
            "getHour" => Some(widgets::time_picker_get_hour(ctx.args, ctx.objects)),
            "getMinute" => Some(widgets::time_picker_get_minute(ctx.args, ctx.objects)),
            "nativeRegisterTimeChangedListener" => Some(widgets::time_picker_register_listener(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_spinner(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::spinner_native_create()),
            "setItems" => Some(widgets::spinner_set_items(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            "getSelectedItemPosition" => Some(widgets::spinner_get_selected(ctx.args, ctx.objects)),
            "nativeRegisterItemSelectedListener" => Some(
                widgets::spinner_register_item_selected_listener(ctx.args, ctx.objects),
            ),
            "performItemSelected" => Some(widgets::spinner_perform_item_selected(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_edit_text(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::edit_text_native_create()),
            "setText" => Some(widgets::edit_text_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            "getText" => Some(widgets::edit_text_get_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            "setHint" => Some(widgets::edit_text_set_hint(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            "setShowKeyboardOnTouch" => Some(widgets::edit_text_set_show_keyboard_on_touch(
                ctx.args,
                ctx.objects,
            )),
            "nativeRegisterEditorActionListener" => Some(
                widgets::edit_text_register_editor_action_listener(ctx.args, ctx.objects),
            ),
            _ => None,
        }
    }

    fn dispatch_toast(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::toast_native_create(ctx.args, ctx.strings)),
            "nativeShow" => Some(widgets::toast_native_show(ctx.args)),
            "nativeCancel" => Some(widgets::toast_native_cancel(ctx.args)),
            _ => None,
        }
    }

    fn dispatch_snackbar(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::snackbar_native_create(ctx.args, ctx.strings)),
            "nativeShow" => Some(widgets::snackbar_native_show(ctx.args)),
            "nativeDismiss" => Some(widgets::snackbar_native_dismiss(ctx.args)),
            "nativeSetAction" => Some(widgets::snackbar_native_set_action(ctx.args, ctx.strings)),
            "nativeRegisterActionClickListener" => Some(
                widgets::snackbar_register_action_click_listener(ctx.args, ctx.objects),
            ),
            _ => None,
        }
    }

    fn dispatch_alert_dialog(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::alert_dialog_native_create(ctx.args, ctx.strings)),
            "nativeShow" => Some(widgets::alert_dialog_native_show(ctx.args)),
            "nativeDismiss" => Some(widgets::alert_dialog_native_dismiss(ctx.args)),
            "nativeRegisterButtonClickListener" => Some(
                widgets::alert_dialog_register_button_click_listener(ctx.args, ctx.objects),
            ),
            _ => None,
        }
    }

    fn dispatch_view_animator(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeStart" => Some(widgets::animator_native_start(ctx.args)),
            "nativeCancel" => Some(widgets::animator_native_cancel(ctx.args)),
            _ => None,
        }
    }

    fn dispatch_gradient_drawable(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeApply" => Some(widgets::gradient_drawable_apply(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_swipe_refresh_layout(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::swipe_refresh_native_create()),
            "addView" => Some(widgets::swipe_refresh_add_view(ctx.args, ctx.objects)),
            "setRefreshing" => Some(widgets::swipe_refresh_set_refreshing(ctx.args, ctx.objects)),
            "nativeRegisterRefreshListener" => Some(widgets::swipe_refresh_register_listener(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_keyboard(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            "nativeCreate" => Some(widgets::keyboard_native_create()),
            "nativeSetTextarea" => Some(widgets::keyboard_set_textarea(ctx.args, ctx.objects)),
            "nativeSetMode" => Some(widgets::keyboard_set_mode(ctx.args, ctx.objects)),
            "nativeRegisterReadyListener" => Some(widgets::keyboard_register_ready_listener(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }
}
