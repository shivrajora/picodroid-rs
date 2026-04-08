//! Hand-written FFI bindings for LVGL v9.2.2.
//!
//! Only the subset of functions needed by picodroid is declared here.
//! Opaque LVGL types (lv_display_t, lv_obj_t, etc.) are represented as
//! `core::ffi::c_void` behind raw pointers.

#![allow(non_camel_case_types, dead_code)]

use core::ffi::{c_char, c_void};

// ---------------------------------------------------------------------------
// Compiler-rt intrinsic required by LVGL's TLSF allocator on Cortex-M0+
// (thumbv6m has no CLZ instruction, so __builtin_ffs maps to __ffssi2).
// ---------------------------------------------------------------------------
#[cfg(target_arch = "arm")]
#[no_mangle]
pub extern "C" fn __ffssi2(mut x: i32) -> i32 {
    if x == 0 {
        return 0;
    }
    let mut bit = 1i32;
    while (x & 1) == 0 {
        x >>= 1;
        bit += 1;
    }
    bit
}

// ---------------------------------------------------------------------------
// Opaque pointer types
// ---------------------------------------------------------------------------
pub type lv_display_t = c_void;
pub type lv_indev_t = c_void;
pub type lv_obj_t = c_void;
pub type lv_event_t = c_void;
pub type lv_event_dsc_t = c_void;

// ---------------------------------------------------------------------------
// Concrete types
// ---------------------------------------------------------------------------

/// lv_color_t in LVGL v9 is always RGB888 (3 bytes), regardless of LV_COLOR_DEPTH.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct lv_color_t {
    pub blue: u8,
    pub green: u8,
    pub red: u8,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct lv_area_t {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct lv_point_t {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct lv_indev_data_t {
    pub point: lv_point_t,
    pub key: u32,
    pub btn_id: u32,
    pub enc_diff: i16,
    pub state: lv_indev_state_t,
    pub continue_reading: bool,
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

pub type lv_indev_state_t = u8;
pub const LV_INDEV_STATE_RELEASED: lv_indev_state_t = 0;
pub const LV_INDEV_STATE_PRESSED: lv_indev_state_t = 1;

pub type lv_indev_type_t = u8;
pub const LV_INDEV_TYPE_NONE: lv_indev_type_t = 0;
pub const LV_INDEV_TYPE_POINTER: lv_indev_type_t = 1;

pub type lv_display_render_mode_t = u32;
pub const LV_DISPLAY_RENDER_MODE_PARTIAL: lv_display_render_mode_t = 0;

pub type lv_anim_enable_t = u8;
pub const LV_ANIM_OFF: lv_anim_enable_t = 0;
pub const LV_ANIM_ON: lv_anim_enable_t = 1;

pub type lv_event_code_t = u32;
pub const LV_EVENT_ALL: lv_event_code_t = 0;
pub const LV_EVENT_PRESSED: lv_event_code_t = 1;
pub const LV_EVENT_CLICKED: lv_event_code_t = 7;
pub const LV_EVENT_VALUE_CHANGED: lv_event_code_t = 32; // LVGL 9.2.2: shifted +4 by ROTARY, HOVER_OVER, HOVER_LEAVE, DRAW_TASK_ADDED

pub type lv_flex_flow_t = u32;
pub const LV_FLEX_FLOW_ROW: lv_flex_flow_t = 0x00;
pub const LV_FLEX_FLOW_COLUMN: lv_flex_flow_t = 0x01;

pub type lv_flex_align_t = u32;
pub const LV_FLEX_ALIGN_START: lv_flex_align_t = 0;
pub const LV_FLEX_ALIGN_CENTER: lv_flex_align_t = 2;

pub type lv_style_selector_t = u32;

// Opacity constants
pub const LV_OPA_COVER: u8 = 255;

// Object flags (from lv_obj.h)
pub const LV_OBJ_FLAG_HIDDEN: u32 = 1 << 0;
pub const LV_OBJ_FLAG_CLICKABLE: u32 = 1 << 1;
pub const LV_OBJ_FLAG_CHECKABLE: u32 = 1 << 3;

// Object states (from lv_obj.h)
pub const LV_STATE_CHECKED: u32 = 0x0001;
pub const LV_STATE_DISABLED: u32 = 0x0080;

// ---------------------------------------------------------------------------
// Callback types
// ---------------------------------------------------------------------------

pub type lv_display_flush_cb_t =
    Option<unsafe extern "C" fn(disp: *mut lv_display_t, area: *const lv_area_t, px_map: *mut u8)>;

pub type lv_indev_read_cb_t =
    Option<unsafe extern "C" fn(indev: *mut lv_indev_t, data: *mut lv_indev_data_t)>;

pub type lv_event_cb_t = Option<unsafe extern "C" fn(e: *mut lv_event_t)>;

// ---------------------------------------------------------------------------
// Extern "C" function declarations
// ---------------------------------------------------------------------------

extern "C" {
    // Core
    pub fn lv_init();
    pub fn lv_tick_inc(tick_period: u32);
    pub fn lv_timer_handler() -> u32;

    // Display
    pub fn lv_display_create(hor_res: i32, ver_res: i32) -> *mut lv_display_t;
    pub fn lv_display_set_flush_cb(disp: *mut lv_display_t, flush_cb: lv_display_flush_cb_t);
    pub fn lv_display_set_buffers(
        disp: *mut lv_display_t,
        buf1: *mut c_void,
        buf2: *mut c_void,
        buf_size: u32,
        render_mode: lv_display_render_mode_t,
    );
    pub fn lv_display_flush_ready(disp: *mut lv_display_t);

    // Input device
    pub fn lv_indev_create() -> *mut lv_indev_t;
    pub fn lv_indev_set_type(indev: *mut lv_indev_t, indev_type: lv_indev_type_t);
    pub fn lv_indev_set_read_cb(indev: *mut lv_indev_t, read_cb: lv_indev_read_cb_t);
    pub fn lv_indev_set_scroll_limit(indev: *mut lv_indev_t, scroll_limit: u8);

    // Screen
    pub fn lv_screen_active() -> *mut lv_obj_t;

    // Objects
    pub fn lv_obj_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_obj_clean(obj: *mut lv_obj_t);
    pub fn lv_obj_set_pos(obj: *mut lv_obj_t, x: i32, y: i32);
    pub fn lv_obj_set_size(obj: *mut lv_obj_t, w: i32, h: i32);
    pub fn lv_obj_center(obj: *mut lv_obj_t);
    pub fn lv_obj_set_style_bg_color(
        obj: *mut lv_obj_t,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_add_event_cb(
        obj: *mut lv_obj_t,
        event_cb: lv_event_cb_t,
        filter: lv_event_code_t,
        user_data: *mut c_void,
    ) -> *mut lv_event_dsc_t;

    // Flex layout
    pub fn lv_obj_set_flex_flow(obj: *mut lv_obj_t, flow: lv_flex_flow_t);
    pub fn lv_obj_set_flex_align(
        obj: *mut lv_obj_t,
        main_place: lv_flex_align_t,
        cross_place: lv_flex_align_t,
        track_cross_place: lv_flex_align_t,
    );

    // Label widget
    pub fn lv_label_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_label_set_text(obj: *mut lv_obj_t, text: *const c_char);

    // Button widget
    pub fn lv_button_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;

    // Bar widget
    pub fn lv_bar_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_bar_set_value(obj: *mut lv_obj_t, value: i32, anim: lv_anim_enable_t);

    // Switch widget
    pub fn lv_switch_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;

    // Color
    pub fn lv_color_hex(c: u32) -> lv_color_t;

    // Events
    pub fn lv_event_get_code(e: *mut lv_event_t) -> lv_event_code_t;
    pub fn lv_event_get_target_obj(e: *mut lv_event_t) -> *mut lv_obj_t;

    // Object lifecycle
    pub fn lv_obj_delete(obj: *mut lv_obj_t);

    // Object parent / child
    pub fn lv_obj_set_parent(obj: *mut lv_obj_t, parent: *mut lv_obj_t);
    pub fn lv_obj_get_child(obj: *mut lv_obj_t, idx: i32) -> *mut lv_obj_t;

    // Object flags
    pub fn lv_obj_add_flag(obj: *mut lv_obj_t, f: u32);
    pub fn lv_obj_remove_flag(obj: *mut lv_obj_t, f: u32);

    // Object state
    pub fn lv_obj_has_state(obj: *mut lv_obj_t, state: u32) -> bool;
    pub fn lv_obj_add_state(obj: *mut lv_obj_t, state: u32);
    pub fn lv_obj_remove_state(obj: *mut lv_obj_t, state: u32);

    // Text style
    pub fn lv_obj_set_style_text_color(
        obj: *mut lv_obj_t,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );

    // Padding style
    pub fn lv_obj_set_style_pad_left(obj: *mut lv_obj_t, value: i32, selector: lv_style_selector_t);
    pub fn lv_obj_set_style_pad_right(
        obj: *mut lv_obj_t,
        value: i32,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_pad_top(obj: *mut lv_obj_t, value: i32, selector: lv_style_selector_t);
    pub fn lv_obj_set_style_pad_bottom(
        obj: *mut lv_obj_t,
        value: i32,
        selector: lv_style_selector_t,
    );

    // Opacity style
    pub fn lv_obj_set_style_opa(obj: *mut lv_obj_t, value: u8, selector: lv_style_selector_t);
    pub fn lv_obj_set_style_bg_opa(obj: *mut lv_obj_t, value: u8, selector: lv_style_selector_t);

    // List widget
    pub fn lv_list_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_list_add_text(list: *mut lv_obj_t, text: *const c_char) -> *mut lv_obj_t;
    pub fn lv_list_add_button(
        list: *mut lv_obj_t,
        icon: *const c_char,
        text: *const c_char,
    ) -> *mut lv_obj_t;

    // Image widget
    pub fn lv_image_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_image_set_src(obj: *mut lv_obj_t, src: *const c_void);

    // Slider widget
    pub fn lv_slider_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_slider_set_value(obj: *mut lv_obj_t, value: i32, anim: lv_anim_enable_t);
    pub fn lv_slider_set_range(obj: *mut lv_obj_t, min: i32, max: i32);
    pub fn lv_slider_get_value(obj: *const lv_obj_t) -> i32;

    // Checkbox widget
    pub fn lv_checkbox_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_checkbox_set_text(obj: *mut lv_obj_t, txt: *const c_char);

    // Dropdown widget
    pub fn lv_dropdown_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_dropdown_set_options(obj: *mut lv_obj_t, options: *const c_char);
    pub fn lv_dropdown_get_selected(obj: *const lv_obj_t) -> u32;

    // Textarea widget
    pub fn lv_textarea_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_textarea_set_text(obj: *mut lv_obj_t, txt: *const c_char);
    pub fn lv_textarea_get_text(obj: *const lv_obj_t) -> *const c_char;
    pub fn lv_textarea_set_placeholder_text(obj: *mut lv_obj_t, txt: *const c_char);
}
