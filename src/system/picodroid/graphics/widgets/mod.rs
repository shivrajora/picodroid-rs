//! Native method implementations for all widget classes:
//! `TextView`, `Button`, `LinearLayout`, `ProgressBar`, `Switch`, `ListView`, `ImageView`.

mod button;
mod image_view;
mod linear_layout;
mod list_view;
mod progress_bar;
mod switch;
mod text_view;

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
pub use switch::{switch_is_checked, switch_native_create, switch_set_checked};
pub use text_view::{text_view_native_create, text_view_set_text, text_view_set_text_color};
