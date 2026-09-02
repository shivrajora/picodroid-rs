// SPDX-License-Identifier: GPL-3.0-only
//! LVGL-backed [`GraphicsBackend`] implementation.
//!
//! Each method matches on the Java method name and delegates to the leaf
//! handlers in `crate::graphics::{display, view, widgets}`,
//! which own the LVGL FFI calls and `handle_table` routing. This indirection
//! keeps LVGL-specific code isolated to the impl block.

use crate::shrink_names::m;
use pico_jvm::NativeContext;

use super::backend::{DispatchResult, GraphicsBackend};

use crate::graphics::{display, view, view_group, widgets};

pub struct LvglBackend;

impl GraphicsBackend for LvglBackend {
    fn dispatch_display(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::getInstance => Some(display::get_instance(ctx.objects)),
            m::setContentView => Some(display::set_content_view(ctx.args, ctx.objects)),
            m::update => Some(display::update()),
            _ => None,
        }
    }

    fn dispatch_display_debug(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        // Picodroid-only debug helpers; mirrors the existing dispatch_display
        // entries before they were moved off picodroid/graphics/Display in
        // the Tier 4 cleanup.
        match method {
            m::pollTouch => Some(display::poll_touch(ctx.objects)),
            m::calibrate => Some(display::calibrate()),
            m::showFps => Some(display::show_fps()),
            _ => None,
        }
    }

    fn dispatch_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::setPosition => Some(view::set_position(ctx.args, ctx.objects)),
            m::setSize => Some(view::set_size(ctx.args, ctx.objects)),
            m::setBackgroundColor => Some(view::set_bg_color(ctx.args, ctx.objects)),
            // setVisibility/setEnabled/setAlpha became Java wrappers (they
            // cache the value for the matching getter) around nativeSet*; the
            // bare names stay accepted so pre-rename PAPKs keep working.
            m::nativeSetVisibility | m::setVisibility => {
                Some(view::set_visibility(ctx.args, ctx.objects))
            }
            m::setPadding => Some(view::set_padding(ctx.args, ctx.objects)),
            m::nativeSetEnabled | m::setEnabled => Some(view::set_enabled(ctx.args, ctx.objects)),
            m::nativeSetAlpha | m::setAlpha => Some(view::set_alpha(ctx.args, ctx.objects)),
            m::getLeft => Some(view::get_left(ctx.args, ctx.objects)),
            m::getTop => Some(view::get_top(ctx.args, ctx.objects)),
            m::getWidth => Some(view::get_width(ctx.args, ctx.objects)),
            m::getHeight => Some(view::get_height(ctx.args, ctx.objects)),
            m::nativeSetProperty => Some(view::set_property(ctx.args, ctx.objects)),
            m::nativeGetProperty => Some(view::get_property(ctx.args, ctx.objects)),
            m::close => Some(view::close(ctx.args, ctx.objects)),
            m::performClick => Some(view::perform_click(ctx.args, ctx.objects)),
            m::nativeSetFlexGrow => Some(view::set_flex_grow(ctx.args, ctx.objects)),
            m::nativeRegisterClickListener => {
                Some(view::register_click_listener(ctx.args, ctx.objects))
            }
            m::nativeRegisterLongClickListener => {
                Some(view::register_long_click_listener(ctx.args, ctx.objects))
            }
            m::performLongClickNative => Some(view::perform_long_press(ctx.args, ctx.objects)),
            m::nativeRegisterKeyListener => {
                Some(view::register_key_listener(ctx.args, ctx.objects))
            }
            m::nativeSetFocusable => Some(view::set_focusable(ctx.args, ctx.objects)),
            m::nativeRequestFocus => Some(view::request_focus(ctx.args, ctx.objects)),
            m::nativeIsFocused => Some(view::is_focused(ctx.args, ctx.objects)),
            m::nativeRegisterFocusChangeListener => {
                Some(view::register_focus_change_listener(ctx.args, ctx.objects))
            }
            m::nativeRegisterTouchListener => {
                Some(view::register_touch_listener(ctx.args, ctx.objects))
            }
            m::nativeRegisterSwipeListener => {
                Some(view::register_swipe_listener(ctx.args, ctx.objects))
            }
            _ => None,
        }
    }

    fn dispatch_view_group(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::addView => Some(view_group::add_view(ctx.args, ctx.objects)),
            m::removeView => Some(view_group::remove_view(ctx.args, ctx.objects)),
            m::removeAllViews => Some(view_group::remove_all_views(ctx.args, ctx.objects)),
            m::getChildCount => Some(view_group::get_child_count(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_text_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::text_view_native_create()),
            m::setText => Some(widgets::text_view_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::setTextColor => Some(widgets::text_view_set_text_color(ctx.args, ctx.objects)),
            m::setIncludeFontPadding => Some(widgets::text_view_set_include_font_padding(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_button(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::button_native_create(ctx.args, ctx.strings)),
            m::setText => Some(widgets::button_set_text(ctx.args, ctx.strings, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_linear_layout(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::linear_layout_native_create()),
            m::setOrientation => Some(widgets::linear_layout_set_orientation(
                ctx.args,
                ctx.objects,
            )),
            m::setSpacing => Some(widgets::linear_layout_set_spacing(ctx.args, ctx.objects)),
            m::setGravity => Some(widgets::linear_layout_set_gravity(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_progress_bar(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::progress_bar_native_create()),
            m::nativeCreateIndeterminate => {
                Some(widgets::progress_bar_native_create_indeterminate(ctx.args))
            }
            m::nativeSetProgress => Some(widgets::progress_bar_set_progress(ctx.args, ctx.objects)),
            m::setTint => Some(widgets::progress_bar_set_tint(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_switch(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::switch_native_create()),
            m::isChecked => Some(widgets::switch_is_checked(ctx.args, ctx.objects)),
            m::setChecked => Some(widgets::switch_set_checked(ctx.args, ctx.objects)),
            m::toggle => Some(widgets::switch_toggle(ctx.args, ctx.objects)),
            m::nativeRegisterCheckedChangeListener => Some(
                widgets::switch_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            m::performCheckedChange => Some(widgets::switch_perform_checked_change(
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
            m::nativeCreate => Some(widgets::toggle_button_native_create()),
            m::nativeCreateWithText => Some(widgets::toggle_button_native_create_with_text(
                ctx.args,
                ctx.strings,
            )),
            m::isChecked => Some(widgets::toggle_button_is_checked(ctx.args, ctx.objects)),
            m::setChecked => Some(widgets::toggle_button_set_checked(ctx.args, ctx.objects)),
            m::toggle => Some(widgets::toggle_button_toggle(ctx.args, ctx.objects)),
            m::setTextOn => Some(widgets::toggle_button_set_text_on(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::setTextOff => Some(widgets::toggle_button_set_text_off(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::nativeRegisterCheckedChangeListener => Some(
                widgets::toggle_button_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            m::performCheckedChange => Some(widgets::toggle_button_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_number_picker(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::number_picker_native_create()),
            m::nativeSetText => Some(widgets::number_picker_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::nativeRegisterPicker => Some(widgets::number_picker_register_picker(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_list_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::list_view_native_create()),
            m::addItem => Some(widgets::list_view_add_item(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::nativeRegisterItemClickListener => Some(
                widgets::list_view_register_item_click_listener(ctx.args, ctx.objects),
            ),
            _ => None,
        }
    }

    fn dispatch_seek_bar(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::seek_bar_native_create()),
            m::nativeCreateWithMax => Some(widgets::seek_bar_native_create_with_max(ctx.args)),
            m::setMax => Some(widgets::seek_bar_set_max(ctx.args, ctx.objects)),
            m::setProgress => Some(widgets::seek_bar_set_progress(ctx.args, ctx.objects)),
            m::getProgress => Some(widgets::seek_bar_get_progress(ctx.args, ctx.objects)),
            m::nativeRegisterChangeListener => Some(widgets::seek_bar_register_change_listener(
                ctx.args,
                ctx.objects,
            )),
            m::performTrackingTouch => Some(widgets::seek_bar_perform_tracking_touch(
                ctx.args,
                ctx.objects,
            )),
            m::performProgressChange => Some(widgets::seek_bar_perform_progress_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_check_box(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::check_box_native_create()),
            m::setText => Some(widgets::check_box_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::isChecked => Some(widgets::check_box_is_checked(ctx.args, ctx.objects)),
            m::setChecked => Some(widgets::check_box_set_checked(ctx.args, ctx.objects)),
            m::nativeRegisterCheckedChangeListener => Some(
                widgets::check_box_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            m::performCheckedChange => Some(widgets::check_box_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_radio_button(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::radio_button_native_create()),
            m::setText => Some(widgets::radio_button_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::isChecked => Some(widgets::radio_button_is_checked(ctx.args, ctx.objects)),
            m::setChecked => Some(widgets::radio_button_set_checked(ctx.args, ctx.objects)),
            m::nativeRegisterCheckedChangeListener => Some(
                widgets::radio_button_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            m::performCheckedChange => Some(widgets::radio_button_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_image_view(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::image_view_native_create()),
            m::setImageSource => Some(widgets::image_view_set_src(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::setScaleType => Some(widgets::image_view_set_scale_type(ctx.args, ctx.objects)),
            m::setTint => Some(widgets::image_view_set_tint(ctx.args, ctx.objects)),
            m::setScale => Some(widgets::image_view_set_scale(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_scroll_view(
        &mut self,
        method: &str,
        _ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::scroll_view_native_create()),
            _ => None,
        }
    }

    fn dispatch_frame_layout(
        &mut self,
        method: &str,
        _ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::frame_layout_native_create()),
            _ => None,
        }
    }

    fn dispatch_date_picker(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::date_picker_native_create()),
            m::setDate => Some(widgets::date_picker_set_date(ctx.args, ctx.objects)),
            m::getYear => Some(widgets::date_picker_get_year(ctx.args, ctx.objects)),
            m::getMonth => Some(widgets::date_picker_get_month(ctx.args, ctx.objects)),
            m::getDay => Some(widgets::date_picker_get_day(ctx.args, ctx.objects)),
            m::nativeRegisterDateChangedListener => Some(widgets::date_picker_register_listener(
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
            m::nativeCreate => Some(widgets::time_picker_native_create()),
            m::setTime => Some(widgets::time_picker_set_time(ctx.args, ctx.objects)),
            m::getHour => Some(widgets::time_picker_get_hour(ctx.args, ctx.objects)),
            m::getMinute => Some(widgets::time_picker_get_minute(ctx.args, ctx.objects)),
            m::nativeRegisterTimeChangedListener => Some(widgets::time_picker_register_listener(
                ctx.args,
                ctx.objects,
            )),
            m::setIs24HourView => Some(widgets::time_picker_set_is_24hour(ctx.args, ctx.objects)),
            m::is24HourView => Some(widgets::time_picker_is_24hour(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_spinner(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::spinner_native_create()),
            m::setItems => Some(widgets::spinner_set_items(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::getSelectedItemPosition => {
                Some(widgets::spinner_get_selected(ctx.args, ctx.objects))
            }
            m::nativeRegisterItemSelectedListener => Some(
                widgets::spinner_register_item_selected_listener(ctx.args, ctx.objects),
            ),
            m::performItemSelected => Some(widgets::spinner_perform_item_selected(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_edit_text(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::edit_text_native_create()),
            m::nativeRegisterTextChangedListener => Some(
                widgets::edit_text_register_text_changed_listener(ctx.args, ctx.objects),
            ),
            m::setText => Some(widgets::edit_text_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::getText => Some(widgets::edit_text_get_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::setHint => Some(widgets::edit_text_set_hint(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            m::setShowKeyboardOnTouch => Some(widgets::edit_text_set_show_keyboard_on_touch(
                ctx.args,
                ctx.objects,
            )),
            m::setInputType => Some(widgets::edit_text_set_input_type(ctx.args, ctx.objects)),
            m::nativeRegisterEditorActionListener => Some(
                widgets::edit_text_register_editor_action_listener(ctx.args, ctx.objects),
            ),
            _ => None,
        }
    }

    fn dispatch_toast(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::toast_native_create(ctx.args, ctx.strings)),
            m::nativeShow => Some(widgets::toast_native_show(ctx.args)),
            m::nativeCancel => Some(widgets::toast_native_cancel(ctx.args)),
            m::nativeSetDuration => Some(widgets::toast_native_set_duration(ctx.args)),
            _ => None,
        }
    }

    fn dispatch_snackbar(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::snackbar_native_create(ctx.args, ctx.strings)),
            m::nativeShow => Some(widgets::snackbar_native_show(ctx.args)),
            m::nativeDismiss => Some(widgets::snackbar_native_dismiss(ctx.args)),
            m::nativeSetAction => Some(widgets::snackbar_native_set_action(ctx.args, ctx.strings)),
            m::nativeRegisterActionClickListener => Some(
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
            m::nativeCreate => Some(widgets::alert_dialog_native_create(ctx.args, ctx.strings)),
            m::nativeCreateWithList => Some(widgets::alert_dialog_native_create_with_list(
                ctx.args,
                ctx.strings,
            )),
            m::nativeShow => Some(widgets::alert_dialog_native_show(ctx.args)),
            m::nativePerformItemClick => {
                Some(widgets::alert_dialog_native_perform_item_click(ctx.args))
            }
            m::nativeDismiss => Some(widgets::alert_dialog_native_dismiss(ctx.args)),
            m::nativeRegisterButtonClickListener => Some(
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
            m::nativeStart => Some(widgets::animator_native_start(ctx.args)),
            m::nativeSetEndAction => Some(widgets::animator_native_set_end_action(ctx.args)),
            m::nativeCancel => Some(widgets::animator_native_cancel(ctx.args)),
            _ => None,
        }
    }

    fn dispatch_gradient_drawable(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            m::nativeApply => Some(widgets::gradient_drawable_apply(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_swipe_refresh_layout(
        &mut self,
        method: &str,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::swipe_refresh_native_create()),
            m::setRefreshing => Some(widgets::swipe_refresh_set_refreshing(ctx.args, ctx.objects)),
            m::nativeRegisterRefreshListener => Some(widgets::swipe_refresh_register_listener(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_keyboard(&mut self, method: &str, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            m::nativeCreate => Some(widgets::keyboard_native_create()),
            m::nativeSetTextarea => Some(widgets::keyboard_set_textarea(ctx.args, ctx.objects)),
            m::nativeSetMode => Some(widgets::keyboard_set_mode(ctx.args, ctx.objects)),
            m::nativeRegisterReadyListener => Some(widgets::keyboard_register_ready_listener(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }
}
