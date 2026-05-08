// SPDX-License-Identifier: GPL-3.0-only
//! Native method implementations for all widget classes:
//! `TextView`, `Button`, `LinearLayout`, `ProgressBar`, `Switch`, `ToggleButton`, `ListView`,
//! `ImageView`, `SeekBar`, `CheckBox`, `ScrollView`, `FrameLayout`, `Spinner`, `EditText`,
//! `Toast`, `AlertDialog`, `Snackbar`.

mod alert_dialog;
mod animator;
mod button;
mod check_box;
mod date_picker;
mod drawable;
mod edit_text;
mod frame_layout;
mod image_view;
mod keyboard;
mod linear_layout;
mod list_view;
mod progress_bar;
mod scroll_view;
mod seek_bar;
mod snackbar;
mod spinner;
mod swipe_refresh_layout;
mod switch;
mod text_view;
mod time_picker;
mod toast;
mod toggle_button;

pub use alert_dialog::{
    alert_dialog_native_create, alert_dialog_native_dismiss, alert_dialog_native_show,
    alert_dialog_register_button_click_listener, reset_alert_dialog_state,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use alert_dialog::{drain_click_queue as drain_dialog_click_queue, lookup_dialog_obj};
pub use animator::{animator_native_cancel, animator_native_start, reset_animation_state};
pub use button::{
    button_native_create, button_perform_click, button_register_click_listener, button_set_text,
    button_was_clicked, reset_button_state,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use button::{drain_click_queue, lookup_button_obj};
pub use check_box::{
    check_box_is_checked, check_box_native_create, check_box_perform_checked_change,
    check_box_register_checked_change_listener, check_box_set_checked, check_box_set_text,
    reset_check_box_state,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use check_box::{drain_cb_checked_change_queue, lookup_cb_checked_change_obj};
pub use date_picker::{
    date_picker_get_day, date_picker_get_month, date_picker_get_year, date_picker_native_create,
    date_picker_register_listener, date_picker_set_date, reset_date_picker_state,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use date_picker::{drain_date_picker_queue, lookup_date_picker_obj};
pub use drawable::gradient_drawable_apply;
pub use edit_text::{
    edit_text_get_text, edit_text_native_create, edit_text_register_editor_action_listener,
    edit_text_set_hint, edit_text_set_show_keyboard_on_touch, edit_text_set_text,
    reset_edit_text_state,
};
pub use frame_layout::{frame_layout_add_view, frame_layout_native_create};
pub use image_view::{
    image_view_native_create, image_view_set_scale, image_view_set_scale_type, image_view_set_src,
    image_view_set_tint,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use keyboard::{
    drain_editor_action, drain_ready_queue as drain_keyboard_ready_queue, lookup_keyboard_obj,
};
pub use keyboard::{
    keyboard_native_create, keyboard_register_ready_listener, keyboard_set_mode,
    keyboard_set_textarea, reset_keyboard_state,
};
pub use linear_layout::{
    linear_layout_add_view, linear_layout_native_create, linear_layout_set_orientation,
    linear_layout_set_spacing,
};
pub use list_view::{list_view_add_item, list_view_native_create};
pub use progress_bar::{
    progress_bar_native_create, progress_bar_native_create_indeterminate,
    progress_bar_set_progress, progress_bar_set_tint, reset_progress_bar_state,
};
pub use scroll_view::{scroll_view_add_view, scroll_view_native_create};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use seek_bar::{drain_seek_change_queue, lookup_seek_bar_obj};
pub use seek_bar::{
    reset_seek_bar_state, seek_bar_get_progress, seek_bar_native_create,
    seek_bar_native_create_with_max, seek_bar_perform_progress_change,
    seek_bar_register_change_listener, seek_bar_set_max, seek_bar_set_progress,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use snackbar::{drain_snackbar_click_queue, lookup_snackbar_obj};
pub use snackbar::{
    reset_snackbar_state, snackbar_native_create, snackbar_native_dismiss,
    snackbar_native_set_action, snackbar_native_show, snackbar_register_action_click_listener,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use spinner::{drain_spinner_change_queue, lookup_spinner_obj};
pub use spinner::{
    reset_spinner_state, spinner_get_selected, spinner_native_create,
    spinner_perform_item_selected, spinner_register_item_selected_listener, spinner_set_items,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use swipe_refresh_layout::{drain_refresh_queue, lookup_refresh_obj};
pub use swipe_refresh_layout::{
    reset_swipe_refresh_state, swipe_refresh_add_view, swipe_refresh_native_create,
    swipe_refresh_register_listener, swipe_refresh_set_refreshing,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use switch::{drain_sw_checked_change_queue, lookup_sw_checked_change_obj};
pub use switch::{
    reset_switch_state, switch_is_checked, switch_native_create, switch_perform_checked_change,
    switch_register_checked_change_listener, switch_set_checked, switch_toggle,
};
pub use text_view::{
    text_view_native_create, text_view_set_include_font_padding, text_view_set_text,
    text_view_set_text_color,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use time_picker::{drain_time_picker_queue, lookup_time_picker_obj};
pub use time_picker::{
    reset_time_picker_state, time_picker_get_hour, time_picker_get_minute, time_picker_is_24hour,
    time_picker_native_create, time_picker_register_listener, time_picker_set_is_24hour,
    time_picker_set_time,
};
pub use toast::{reset_toast_state, toast_native_cancel, toast_native_create, toast_native_show};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use toggle_button::{drain_checked_change_queue, lookup_checked_change_obj};
pub use toggle_button::{
    reset_toggle_button_state, toggle_button_is_checked, toggle_button_native_create,
    toggle_button_native_create_with_text, toggle_button_perform_checked_change,
    toggle_button_register_checked_change_listener, toggle_button_set_checked,
    toggle_button_set_text_off, toggle_button_set_text_on, toggle_button_toggle,
};
