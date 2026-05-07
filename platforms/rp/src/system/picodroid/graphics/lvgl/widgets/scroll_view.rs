// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `ScrollView` (`lv_obj` — scrolls when content exceeds bounds).

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let o = lv_obj_create(lifecycle::screen_ptr());
        // Clear theme padding so the scroll container is transparent;
        // padding is controlled explicitly via setPadding().
        lv_obj_set_style_pad_left(o, 0, 0);
        lv_obj_set_style_pad_right(o, 0, 0);
        lv_obj_set_style_pad_top(o, 0, 0);
        lv_obj_set_style_pad_bottom(o, 0, 0);
        // ScrollView is conceptually vertical-only (matches Android, where
        // HorizontalScrollView is a separate class). Without this, LVGL's
        // default elastic over-pull lets users drag horizontally even when
        // content fits, briefly showing a horizontal scrollbar.
        lv_obj_set_scroll_dir(o, LV_DIR_VER);
        o
    };
    handle_table::register(ptr)
}
