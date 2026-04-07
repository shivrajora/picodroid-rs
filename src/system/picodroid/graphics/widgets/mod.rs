//! Native method implementations for all widget classes:
//! `TextView`, `Button`, `LinearLayout`, `ProgressBar`, `Switch`, `ToggleButton`, `ListView`,
//! `ImageView`, `SeekBar`, `CheckBox`, `ScrollView`, `FrameLayout`, `Spinner`, `EditText`.

mod button;
mod check_box;
mod edit_text;
mod frame_layout;
mod image_view;
mod linear_layout;
mod list_view;
mod progress_bar;
mod scroll_view;
mod seek_bar;
mod spinner;
mod switch;
mod text_view;
mod toggle_button;

pub use button::{
    button_native_create, button_register_click_listener, button_set_text, button_was_clicked,
    reset_button_state,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use button::{drain_click_queue, lookup_button_obj};
pub use check_box::{
    check_box_is_checked, check_box_native_create, check_box_register_checked_change_listener,
    check_box_set_checked, check_box_set_text, reset_check_box_state,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use check_box::{drain_cb_checked_change_queue, lookup_cb_checked_change_obj};
pub use edit_text::{
    edit_text_get_text, edit_text_native_create, edit_text_set_hint, edit_text_set_text,
};
pub use frame_layout::{frame_layout_add_view, frame_layout_native_create};
pub use image_view::{image_view_native_create, image_view_set_src};
pub use linear_layout::{
    linear_layout_add_view, linear_layout_native_create, linear_layout_set_orientation,
};
pub use list_view::{list_view_add_item, list_view_native_create};
pub use progress_bar::{progress_bar_native_create, progress_bar_set_progress};
pub use scroll_view::{scroll_view_add_view, scroll_view_native_create};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use seek_bar::{drain_seek_change_queue, lookup_seek_bar_obj};
pub use seek_bar::{
    reset_seek_bar_state, seek_bar_get_progress, seek_bar_native_create,
    seek_bar_native_create_with_max, seek_bar_register_change_listener, seek_bar_set_max,
    seek_bar_set_progress,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use spinner::{drain_spinner_change_queue, lookup_spinner_obj};
pub use spinner::{
    reset_spinner_state, spinner_get_selected, spinner_native_create,
    spinner_register_item_selected_listener, spinner_set_items,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use switch::{drain_sw_checked_change_queue, lookup_sw_checked_change_obj};
pub use switch::{
    reset_switch_state, switch_is_checked, switch_native_create,
    switch_register_checked_change_listener, switch_set_checked, switch_toggle,
};
pub use text_view::{text_view_native_create, text_view_set_text, text_view_set_text_color};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use toggle_button::{drain_checked_change_queue, lookup_checked_change_obj};
pub use toggle_button::{
    reset_toggle_button_state, toggle_button_is_checked, toggle_button_native_create,
    toggle_button_native_create_with_text, toggle_button_register_checked_change_listener,
    toggle_button_set_checked, toggle_button_set_text_off, toggle_button_set_text_on,
    toggle_button_toggle,
};
