//! Native method implementations for all widget classes:
//! `TextView`, `Button`, `LinearLayout`, `ProgressBar`, `Switch`, `ToggleButton`, `ListView`,
//! `ImageView`.

mod button;
mod image_view;
mod linear_layout;
mod list_view;
mod progress_bar;
mod switch;
mod text_view;
mod toggle_button;

pub use button::{
    button_native_create, button_register_click_listener, button_set_text, button_was_clicked,
    reset_button_state,
};
#[cfg_attr(feature = "sim", allow(unused_imports))]
pub use button::{drain_click_queue, lookup_button_obj};
pub use image_view::{image_view_native_create, image_view_set_src};
pub use linear_layout::{
    linear_layout_add_view, linear_layout_native_create, linear_layout_set_orientation,
};
pub use list_view::{list_view_add_item, list_view_native_create};
pub use progress_bar::{progress_bar_native_create, progress_bar_set_progress};
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
