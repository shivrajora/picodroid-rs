// SPDX-License-Identifier: GPL-3.0-only
//! LVGL-backed [`GraphicsBackend`] implementation.
//!
//! Each method matches on the Java method name and delegates to the leaf
//! handlers in `crate::graphics::{display, view, widgets}`,
//! which own the LVGL FFI calls and `handle_table` routing. This indirection
//! keeps LVGL-specific code isolated to the impl block.
use crate::shrink_names::mh;

use pico_jvm::NativeContext;

use super::backend::{DispatchResult, GraphicsBackend};

use crate::graphics::{display, view, view_group, widgets};

pub struct LvglBackend;

impl GraphicsBackend for LvglBackend {
    fn dispatch_display(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::getInstance => Some(display::get_instance(ctx.objects)),
            mh::setContentView => Some(display::set_content_view(ctx.args, ctx.objects)),
            mh::update => Some(display::update()),
            _ => None,
        }
    }

    fn dispatch_display_debug(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        // Picodroid-only debug helpers; mirrors the existing dispatch_display
        // entries before they were moved off picodroid/graphics/Display in
        // the Tier 4 cleanup.
        match method {
            mh::pollTouch => Some(display::poll_touch(ctx.objects)),
            mh::calibrate => Some(display::calibrate()),
            mh::showFps => Some(display::show_fps()),
            _ => None,
        }
    }

    fn dispatch_view(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::setPosition => Some(view::set_position(ctx.args, ctx.objects)),
            mh::nativeSetSize => Some(view::set_size(ctx.args, ctx.objects)),
            mh::nativeSetBackgroundColor => Some(view::set_bg_color(ctx.args, ctx.objects)),
            mh::nativeSetBackgroundTint => Some(view::set_bg_tint(ctx.args, ctx.objects)),
            // setVisibility/setEnabled/setAlpha became Java wrappers (they
            // cache the value for the matching getter) around nativeSet*; the
            // bare names stay accepted so pre-rename PAPKs keep working.
            mh::nativeSetVisibility | mh::setVisibility => {
                Some(view::set_visibility(ctx.args, ctx.objects))
            }
            mh::setPadding => Some(view::set_padding(ctx.args, ctx.objects)),
            mh::nativeSetEnabled | mh::setEnabled => Some(view::set_enabled(ctx.args, ctx.objects)),
            mh::nativeSetKeepScreenOn => Some(view::set_keep_screen_on(ctx.args, ctx.objects)),
            mh::nativeSetMinimumSize => Some(view::set_minimum_size(ctx.args, ctx.objects)),
            mh::nativeSetMaxWidth => Some(view::set_max_width(ctx.args, ctx.objects)),
            mh::nativeSetAlpha | mh::setAlpha => Some(view::set_alpha(ctx.args, ctx.objects)),
            mh::getLeft => Some(view::get_left(ctx.args, ctx.objects)),
            mh::getTop => Some(view::get_top(ctx.args, ctx.objects)),
            mh::getWidth => Some(view::get_width(ctx.args, ctx.objects)),
            mh::isInTouchMode => Some(view::is_in_touch_mode()),
            mh::getHeight => Some(view::get_height(ctx.args, ctx.objects)),
            mh::nativeSetProperty => Some(view::set_property(ctx.args, ctx.objects)),
            mh::nativeGetProperty => Some(view::get_property(ctx.args, ctx.objects)),
            mh::nativeClose => Some(view::close(ctx.args, ctx.objects)),
            mh::performClick => Some(view::perform_click(ctx.args, ctx.objects)),
            mh::nativeSetWeight => Some(view::set_weight(ctx.args, ctx.objects)),
            mh::nativeSetLayoutGravity => Some(view::set_layout_gravity(ctx.args, ctx.objects)),
            mh::nativeSetMargins => Some(view::set_margins(ctx.args, ctx.objects)),
            mh::nativeSetFrameGravity => Some(view::set_frame_gravity(ctx.args, ctx.objects)),
            mh::nativeSetRelativeRules => {
                Some(view::set_relative_rules(ctx.args, ctx.arrays, ctx.objects))
            }
            mh::nativeRegisterClickListener => {
                Some(view::register_click_listener(ctx.args, ctx.objects))
            }
            mh::nativeRegisterLongClickListener => {
                Some(view::register_long_click_listener(ctx.args, ctx.objects))
            }
            mh::performLongClickNative => Some(view::perform_long_press(ctx.args, ctx.objects)),
            mh::nativeRegisterKeyListener => {
                Some(view::register_key_listener(ctx.args, ctx.objects))
            }
            mh::nativeSetFocusable => Some(view::set_focusable(ctx.args, ctx.objects)),
            mh::nativeRequestFocus => Some(view::request_focus(ctx.args, ctx.objects)),
            mh::nativeIsFocused => Some(view::is_focused(ctx.args, ctx.objects)),
            mh::nativeRegisterFocusChangeListener => {
                Some(view::register_focus_change_listener(ctx.args, ctx.objects))
            }
            // `setOnTouchListener` and the drawing View's constructor: the same binding and
            // LVGL press / move / release trampolines, with or without a listener object.
            mh::nativeRegisterTouchListener | mh::nativeEnableTouch => {
                Some(view::register_touch_listener(ctx.args, ctx.objects))
            }
            mh::nativeRegisterSwipeListener => {
                Some(view::register_swipe_listener(ctx.args, ctx.objects))
            }
            // The drawing View: `new View(Context)` and the bind around its `onDraw`. A board
            // without `has_canvas` ships no Canvas class, so these stay unclaimed there.
            #[cfg(has_canvas)]
            mh::nativeCreateView => Some(widgets::view_native_create_view()),
            #[cfg(has_canvas)]
            mh::nativeBeginDraw => Some(widgets::view_native_begin_draw(ctx.args, ctx.objects)),
            #[cfg(has_canvas)]
            mh::nativeEndDraw => Some(widgets::view_native_end_draw(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_view_group(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeAddView => Some(view_group::add_view(ctx.args, ctx.strings, ctx.objects)),
            mh::nativeRemoveView => Some(view_group::remove_view(ctx.args, ctx.objects)),
            mh::nativeRemoveAllViews => Some(view_group::remove_all_views(ctx.args, ctx.objects)),
            mh::nativeMoveChildTo => Some(view_group::move_child_to(ctx.args, ctx.objects)),
            mh::nativeGetChildCount => Some(view_group::get_child_count(ctx.args, ctx.objects)),
            mh::nativeCreateGroup => Some(view_group::create_group()),
            mh::nativeWatchSize => Some(view_group::watch_size(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_text_view(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::text_view_native_create()),
            mh::nativeSetText => Some(widgets::text_view_native_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::nativeSetTextColor => Some(widgets::text_view_native_set_text_color(
                ctx.args,
                ctx.objects,
            )),
            mh::nativeSetIncludeFontPadding => Some(
                widgets::text_view_native_set_include_font_padding(ctx.args, ctx.objects),
            ),
            mh::nativeSetLineMode => Some(widgets::text_view_native_set_line_mode(
                ctx.args,
                ctx.objects,
            )),
            mh::nativeSetTextSize => Some(widgets::text_view_native_set_text_size(
                ctx.args,
                ctx.objects,
            )),
            mh::nativeGetLineHeight => Some(widgets::text_view_native_get_line_height(
                ctx.args,
                ctx.objects,
            )),
            mh::nativeSetGravity => {
                Some(widgets::text_view_native_set_gravity(ctx.args, ctx.objects))
            }
            _ => None,
        }
    }

    fn dispatch_button(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::button_native_create(ctx.args, ctx.strings)),
            _ => None,
        }
    }

    fn dispatch_layout_inflater(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeInflateNode => Some(widgets::layout_inflater_inflate_node(ctx)),
            _ => None,
        }
    }

    fn dispatch_linear_layout(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::linear_layout_native_create()),
            mh::setOrientation => Some(widgets::linear_layout_set_orientation(
                ctx.args,
                ctx.objects,
            )),
            mh::setSpacing => Some(widgets::linear_layout_set_spacing(ctx.args, ctx.objects)),
            mh::setGravity => Some(widgets::linear_layout_set_gravity(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_progress_bar(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::progress_bar_native_create()),
            mh::nativeCreateIndeterminate => {
                Some(widgets::progress_bar_native_create_indeterminate(ctx.args))
            }
            mh::nativeSetProgress => {
                Some(widgets::progress_bar_set_progress(ctx.args, ctx.objects))
            }
            mh::nativeSetRange => Some(widgets::progress_bar_set_range(ctx.args, ctx.objects)),
            mh::nativeSetTint => Some(widgets::progress_bar_set_tint(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_circular_progress_indicator(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::circular_progress_indicator_native_create(ctx.args)),
            // Inherited from ProgressBar and routed here by the receiver's runtime
            // class, so the lv_bar setters never see the ring's lv_arc. The tint
            // lists are applied by the Java overrides through the ring's own
            // colour natives, so the bar's tint native has nothing left to do.
            mh::nativeSetProgress => Some(widgets::circular_progress_indicator_set_progress(
                ctx.args,
                ctx.objects,
            )),
            mh::nativeSetRange => Some(widgets::circular_progress_indicator_set_range(
                ctx.args,
                ctx.objects,
            )),
            mh::nativeSetTint => Some(Ok(None)),
            mh::nativeSetIndicatorColor => Some(
                widgets::circular_progress_indicator_set_indicator_color(ctx.args, ctx.objects),
            ),
            mh::nativeSetTrackColor => Some(widgets::circular_progress_indicator_set_track_color(
                ctx.args,
                ctx.objects,
            )),
            mh::nativeSetTrackThickness => Some(
                widgets::circular_progress_indicator_set_track_thickness(ctx.args, ctx.objects),
            ),
            mh::nativeSetIndicatorDirection => Some(
                widgets::circular_progress_indicator_set_indicator_direction(ctx.args, ctx.objects),
            ),
            mh::nativeSetTrackCornerRadius => Some(
                widgets::circular_progress_indicator_set_track_corner_radius(ctx.args, ctx.objects),
            ),
            mh::nativeSetAngles => Some(widgets::circular_progress_indicator_set_angles(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_switch(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::switch_native_create()),
            mh::isChecked => Some(widgets::switch_is_checked(ctx.args, ctx.objects)),
            mh::setChecked => Some(widgets::switch_set_checked(ctx.args, ctx.objects)),
            mh::toggle => Some(widgets::switch_toggle(ctx.args, ctx.objects)),
            mh::nativeRegisterCheckedChangeListener => Some(
                widgets::switch_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            mh::performCheckedChange => Some(widgets::switch_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_toggle_button(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::toggle_button_native_create()),
            mh::nativeCreateWithText => Some(widgets::toggle_button_native_create_with_text(
                ctx.args,
                ctx.strings,
            )),
            mh::isChecked => Some(widgets::toggle_button_is_checked(ctx.args, ctx.objects)),
            mh::setChecked => Some(widgets::toggle_button_set_checked(ctx.args, ctx.objects)),
            mh::toggle => Some(widgets::toggle_button_toggle(ctx.args, ctx.objects)),
            mh::setTextOn => Some(widgets::toggle_button_set_text_on(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::setTextOff => Some(widgets::toggle_button_set_text_off(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::nativeRegisterCheckedChangeListener => Some(
                widgets::toggle_button_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            mh::performCheckedChange => Some(widgets::toggle_button_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_number_picker(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::number_picker_native_create()),
            mh::nativeSetText => Some(widgets::number_picker_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::nativeRegisterPicker => Some(widgets::number_picker_register_picker(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_list_view(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::list_view_native_create()),
            mh::addItem => Some(widgets::list_view_add_item(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::nativeRegisterItemClickListener => Some(
                widgets::list_view_register_item_click_listener(ctx.args, ctx.objects),
            ),
            mh::nativeStyleRow => Some(widgets::list_view_style_row(ctx.args, ctx.objects)),
            mh::nativeWatchViewport => {
                Some(widgets::list_view_watch_viewport(ctx.args, ctx.objects))
            }
            mh::nativeScrollY => Some(widgets::list_view_scroll_y(ctx.args, ctx.objects)),
            mh::nativeScrollTo => Some(widgets::list_view_scroll_to(ctx.args, ctx.objects)),
            mh::nativeSetSpacers => Some(widgets::list_view_set_spacers(ctx.args, ctx.objects)),
            mh::nativeRecycleRow => Some(widgets::list_view_recycle_row(ctx.args, ctx.objects)),
            mh::nativeSyncFocusOrder => {
                Some(widgets::list_view_sync_focus_order(ctx.args, ctx.objects))
            }
            _ => None,
        }
    }

    fn dispatch_seek_bar(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::seek_bar_native_create()),
            mh::nativeCreateWithMax => Some(widgets::seek_bar_native_create_with_max(ctx.args)),
            mh::nativeSetMax => Some(widgets::seek_bar_set_max(ctx.args, ctx.objects)),
            mh::nativeSetProgress => Some(widgets::seek_bar_set_progress(ctx.args, ctx.objects)),
            mh::getProgress => Some(widgets::seek_bar_get_progress(ctx.args, ctx.objects)),
            mh::nativeRegisterChangeListener => Some(widgets::seek_bar_register_change_listener(
                ctx.args,
                ctx.objects,
            )),
            mh::performTrackingTouch => Some(widgets::seek_bar_perform_tracking_touch(
                ctx.args,
                ctx.objects,
            )),
            mh::performProgressChange => Some(widgets::seek_bar_perform_progress_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_check_box(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::check_box_native_create()),
            mh::setText => Some(widgets::check_box_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::isChecked => Some(widgets::check_box_is_checked(ctx.args, ctx.objects)),
            mh::setChecked => Some(widgets::check_box_set_checked(ctx.args, ctx.objects)),
            mh::nativeRegisterCheckedChangeListener => Some(
                widgets::check_box_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            mh::performCheckedChange => Some(widgets::check_box_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_radio_button(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::radio_button_native_create()),
            mh::setText => Some(widgets::radio_button_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::isChecked => Some(widgets::radio_button_is_checked(ctx.args, ctx.objects)),
            mh::nativeSetChecked => Some(widgets::radio_button_set_checked(ctx.args, ctx.objects)),
            mh::nativeRegisterCheckedChangeListener => Some(
                widgets::radio_button_register_checked_change_listener(ctx.args, ctx.objects),
            ),
            mh::performCheckedChange => Some(widgets::radio_button_perform_checked_change(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_image_view(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::image_view_native_create()),
            mh::setImageSource => Some(widgets::image_view_set_src(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::setImageResource => Some(widgets::image_view_set_resource(ctx)),
            mh::setScaleType => Some(widgets::image_view_set_scale_type(ctx.args, ctx.objects)),
            mh::setTint => Some(widgets::image_view_set_tint(ctx.args, ctx.objects)),
            mh::setScale => Some(widgets::image_view_set_scale(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_scroll_view(
        &mut self,
        method: u32,
        _ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::scroll_view_native_create()),
            _ => None,
        }
    }

    fn dispatch_frame_layout(
        &mut self,
        method: u32,
        _ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::frame_layout_native_create()),
            _ => None,
        }
    }

    fn dispatch_grid_layout(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::grid_layout_native_create()),
            mh::nativePlace => Some(widgets::grid_layout_place(ctx.args, ctx.objects)),
            mh::setSpacing => Some(widgets::grid_layout_set_spacing(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_relative_layout(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::relative_layout_native_create()),
            mh::nativeSetGravity => {
                Some(widgets::relative_layout_set_gravity(ctx.args, ctx.objects))
            }
            _ => None,
        }
    }

    fn dispatch_date_picker(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::date_picker_native_create()),
            mh::nativeSetDate => Some(widgets::date_picker_set_date(ctx.args, ctx.objects)),
            mh::nativeGetYear => Some(widgets::date_picker_get_year(ctx.args, ctx.objects)),
            mh::nativeGetMonth => Some(widgets::date_picker_get_month(ctx.args, ctx.objects)),
            mh::nativeGetDay => Some(widgets::date_picker_get_day(ctx.args, ctx.objects)),
            mh::nativeRegisterDateChangedListener => Some(widgets::date_picker_register_listener(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_time_picker(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::time_picker_native_create()),
            mh::setTime => Some(widgets::time_picker_set_time(ctx.args, ctx.objects)),
            mh::getHour => Some(widgets::time_picker_get_hour(ctx.args, ctx.objects)),
            mh::getMinute => Some(widgets::time_picker_get_minute(ctx.args, ctx.objects)),
            mh::nativeRegisterTimeChangedListener => Some(widgets::time_picker_register_listener(
                ctx.args,
                ctx.objects,
            )),
            mh::setIs24HourView => Some(widgets::time_picker_set_is_24hour(ctx.args, ctx.objects)),
            mh::is24HourView => Some(widgets::time_picker_is_24hour(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    fn dispatch_spinner(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::spinner_native_create()),
            mh::setItems => Some(widgets::spinner_set_items(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::getSelectedItemPosition => {
                Some(widgets::spinner_get_selected(ctx.args, ctx.objects))
            }
            mh::nativeRegisterItemSelectedListener => Some(
                widgets::spinner_register_item_selected_listener(ctx.args, ctx.objects),
            ),
            mh::performItemSelected => Some(widgets::spinner_perform_item_selected(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_edit_text(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::edit_text_native_create()),
            mh::nativeRegisterTextChangedListener => Some(
                widgets::edit_text_register_text_changed_listener(ctx.args, ctx.objects),
            ),
            mh::setText => Some(widgets::edit_text_set_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::getText => Some(widgets::edit_text_get_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::setHint => Some(widgets::edit_text_set_hint(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            mh::setShowKeyboardOnTouch => Some(widgets::edit_text_set_show_keyboard_on_touch(
                ctx.args,
                ctx.objects,
            )),
            mh::setInputType => Some(widgets::edit_text_set_input_type(ctx.args, ctx.objects)),
            mh::nativeRegisterEditorActionListener => Some(
                widgets::edit_text_register_editor_action_listener(ctx.args, ctx.objects),
            ),
            _ => None,
        }
    }

    fn dispatch_toast(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::toast_native_create(ctx.args, ctx.strings)),
            mh::nativeShow => Some(widgets::toast_native_show(ctx.args)),
            mh::nativeCancel => Some(widgets::toast_native_cancel(ctx.args)),
            mh::nativeSetDuration => Some(widgets::toast_native_set_duration(ctx.args)),
            _ => None,
        }
    }

    fn dispatch_snackbar(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::snackbar_native_create(ctx.args, ctx.strings)),
            mh::nativeShow => Some(widgets::snackbar_native_show(ctx.args)),
            mh::nativeDismiss => Some(widgets::snackbar_native_dismiss(ctx.args)),
            mh::nativeSetAction => Some(widgets::snackbar_native_set_action(ctx.args, ctx.strings)),
            mh::nativeRegisterActionClickListener => Some(
                widgets::snackbar_register_action_click_listener(ctx.args, ctx.objects),
            ),
            _ => None,
        }
    }

    fn dispatch_alert_dialog(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::alert_dialog_native_create(ctx.args, ctx.strings)),
            mh::nativeCreateWithList => Some(widgets::alert_dialog_native_create_with_list(
                ctx.args,
                ctx.strings,
            )),
            mh::nativeShow => Some(widgets::alert_dialog_native_show(ctx.args)),
            mh::nativePerformItemClick => {
                Some(widgets::alert_dialog_native_perform_item_click(ctx.args))
            }
            mh::nativeDismiss => Some(widgets::alert_dialog_native_dismiss(ctx.args)),
            mh::nativeRegisterButtonClickListener => Some(
                widgets::alert_dialog_register_button_click_listener(ctx.args, ctx.objects),
            ),
            _ => None,
        }
    }

    fn dispatch_view_animator(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeStart => Some(widgets::animator_native_start(ctx.args)),
            mh::nativeSetEndAction => Some(widgets::animator_native_set_end_action(ctx.args)),
            mh::nativeCancel => Some(widgets::animator_native_cancel(ctx.args)),
            _ => None,
        }
    }

    fn dispatch_gradient_drawable(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeApply => Some(widgets::gradient_drawable_apply(ctx.args, ctx.objects)),
            _ => None,
        }
    }

    #[cfg(has_canvas)]
    fn dispatch_canvas(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeDrawColor => Some(widgets::canvas_draw_color(ctx.args, ctx.objects)),
            mh::nativeDrawRect => Some(widgets::canvas_draw_rect(ctx.args, ctx.objects)),
            mh::nativeDrawLine => Some(widgets::canvas_draw_line(ctx.args, ctx.objects)),
            mh::nativeDrawArc => Some(widgets::canvas_draw_arc(ctx.args, ctx.objects)),
            mh::nativeDrawText => Some(widgets::canvas_draw_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            _ => None,
        }
    }

    #[cfg(has_canvas)]
    fn dispatch_paint(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::ascent => Some(widgets::paint_ascent(ctx.args, ctx.objects)),
            mh::descent => Some(widgets::paint_descent(ctx.args, ctx.objects)),
            mh::measureText => Some(widgets::paint_measure_text(
                ctx.args,
                ctx.strings,
                ctx.objects,
            )),
            _ => None,
        }
    }

    #[cfg(has_multi_app)]
    fn dispatch_bitmap_drawable(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeSetImageSrc => Some(widgets::bitmap_drawable_set_image_src(
                ctx.args,
                ctx.objects,
            )),
            mh::nativeSetBackground => Some(widgets::bitmap_drawable_set_background(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_swipe_refresh_layout(
        &mut self,
        method: u32,
        ctx: &mut NativeContext<'_>,
    ) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::swipe_refresh_native_create()),
            mh::setRefreshing => Some(widgets::swipe_refresh_set_refreshing(ctx.args, ctx.objects)),
            mh::nativeRegisterRefreshListener => Some(widgets::swipe_refresh_register_listener(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }

    fn dispatch_keyboard(&mut self, method: u32, ctx: &mut NativeContext<'_>) -> DispatchResult {
        match method {
            mh::nativeCreate => Some(widgets::keyboard_native_create()),
            mh::nativeSetTextarea => Some(widgets::keyboard_set_textarea(ctx.args, ctx.objects)),
            mh::nativeSetMode => Some(widgets::keyboard_set_mode(ctx.args, ctx.objects)),
            mh::nativeRegisterReadyListener => Some(widgets::keyboard_register_ready_listener(
                ctx.args,
                ctx.objects,
            )),
            _ => None,
        }
    }
}
