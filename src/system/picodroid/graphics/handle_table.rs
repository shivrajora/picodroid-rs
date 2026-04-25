//! Transitional re-export shim — implementation moved to
//! [`super::lvgl::handle_table`] in plan step 4. Removed when widgets
//! finish migrating to `with_gfx` (plan step 7).
#![allow(unused_imports)]
pub use super::lvgl::handle_table::*;
