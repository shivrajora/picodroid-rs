//! Hand-written FFI bindings for LVGL v9.5.0.
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
pub type lv_group_t = c_void;

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

/// LVGL gesture-type count (`LV_INDEV_GESTURE_CNT` in lv_indev.h). Must match
/// upstream — sized arrays inside `lv_indev_data_t` depend on this. v9.5.0
/// has 6 gesture types (NONE, PINCH, SWIPE, ROTATE, TWO_FINGERS_SWIPE,
/// SCROLL).
const LV_INDEV_GESTURE_CNT: usize = 6;

/// `lv_indev_gesture_type_t` is a C enum compiled with `-fshort-enums` on
/// both ARM and sim (build_support/lvgl.rs forces the flag on non-ARM), so
/// it is a single byte.
pub type lv_indev_gesture_type_t = u8;

/// Layout matches `lv_indev_data_t` in vendor/lvgl/src/indev/lv_indev.h.
/// The leading `gesture_type` / `gesture_data` arrays were added in v9.5.0
/// and `state` was reordered to precede `point`. We don't fill the gesture
/// fields ourselves — LVGL initialises them — so we just need the offsets
/// for `state`, `point`, `key`, and `continue_reading` to match.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct lv_indev_data_t {
    pub gesture_type: [lv_indev_gesture_type_t; LV_INDEV_GESTURE_CNT],
    pub gesture_data: [*mut c_void; LV_INDEV_GESTURE_CNT],
    pub state: lv_indev_state_t,
    pub point: lv_point_t,
    pub key: u32,
    pub btn_id: u32,
    pub enc_diff: i16,
    pub timestamp: u32,
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
pub const LV_INDEV_TYPE_KEYPAD: lv_indev_type_t = 2;

pub const LV_KEY_UP: u32 = 17;
pub const LV_KEY_DOWN: u32 = 18;
pub const LV_KEY_RIGHT: u32 = 19;
pub const LV_KEY_LEFT: u32 = 20;
pub const LV_KEY_ESC: u32 = 27;
pub const LV_KEY_ENTER: u32 = 10;
pub const LV_KEY_NEXT: u32 = 9;
pub const LV_KEY_PREV: u32 = 11;

pub type lv_display_render_mode_t = u32;
pub const LV_DISPLAY_RENDER_MODE_PARTIAL: lv_display_render_mode_t = 0;

pub type lv_anim_enable_t = u8;
pub const LV_ANIM_OFF: lv_anim_enable_t = 0;
pub const LV_ANIM_ON: lv_anim_enable_t = 1;

pub type lv_event_code_t = u32;
// Values verified against vendor/lvgl/src/misc/lv_event.h in LVGL 9.5.0.
// See project_lvgl_ffi_constants.md memory: wrong values silently route to
// wrong handlers and can cause infinite render loops.
//
// v9.5.0 inserted SINGLE_CLICKED / DOUBLE_CLICKED / TRIPLE_CLICKED before
// LONG_PRESSED, shifting all subsequent codes by +3 vs v9.2.2.
pub const LV_EVENT_ALL: lv_event_code_t = 0;
pub const LV_EVENT_PRESSED: lv_event_code_t = 1;
pub const LV_EVENT_PRESSING: lv_event_code_t = 2;
pub const LV_EVENT_LONG_PRESSED: lv_event_code_t = 8;
pub const LV_EVENT_CLICKED: lv_event_code_t = 10;
pub const LV_EVENT_RELEASED: lv_event_code_t = 11;
pub const LV_EVENT_VALUE_CHANGED: lv_event_code_t = 35;
/// Fired when a multi-step interaction "completes" — used by lv_keyboard
/// when the user taps the OK / Enter key. Position 38 in the v9.5.0
/// enum (VALUE_CHANGED + 3, accounting for INSERT, REFRESH).
pub const LV_EVENT_READY: lv_event_code_t = 38;

pub type lv_flex_flow_t = u32;
pub const LV_FLEX_FLOW_ROW: lv_flex_flow_t = 0x00;
pub const LV_FLEX_FLOW_COLUMN: lv_flex_flow_t = 0x01;

pub type lv_flex_align_t = u32;
pub const LV_FLEX_ALIGN_START: lv_flex_align_t = 0;
pub const LV_FLEX_ALIGN_CENTER: lv_flex_align_t = 2;

/// Linear-gradient direction. Values verified against
/// vendor/lvgl/src/misc/lv_style.h. v1 GradientDrawable only exposes
/// NONE / VER / HOR — LINEAR (arbitrary angle) and RADIAL are deferred.
pub type lv_grad_dir_t = u32;
pub const LV_GRAD_DIR_NONE: lv_grad_dir_t = 0;
pub const LV_GRAD_DIR_VER: lv_grad_dir_t = 1;
pub const LV_GRAD_DIR_HOR: lv_grad_dir_t = 2;

/// Soft-keyboard mode. Verified against
/// vendor/lvgl/src/widgets/keyboard/lv_keyboard.h. Only the four
/// "primary" modes are exposed; the various "user-defined" modes are
/// deferred until apps need custom layouts.
pub type lv_keyboard_mode_t = u32;
pub const LV_KEYBOARD_MODE_TEXT_LOWER: lv_keyboard_mode_t = 0;
pub const LV_KEYBOARD_MODE_TEXT_UPPER: lv_keyboard_mode_t = 1;
pub const LV_KEYBOARD_MODE_SPECIAL: lv_keyboard_mode_t = 2;
pub const LV_KEYBOARD_MODE_NUMBER: lv_keyboard_mode_t = 3;

pub type lv_style_selector_t = u32;

// Opacity constants
pub const LV_OPA_COVER: u8 = 255;

// Object flags (from lv_obj.h)
pub const LV_OBJ_FLAG_HIDDEN: u32 = 1 << 0;
pub const LV_OBJ_FLAG_CLICKABLE: u32 = 1 << 1;
pub const LV_OBJ_FLAG_CHECKABLE: u32 = 1 << 3;

// Object states (from lv_obj_style.h, v9.5.0).
// The state bits were renumbered in v9.5.0 to leave room for LV_STATE_ALT
// (1 << 0) and reserved slots, so values differ from v9.2.2.
pub const LV_STATE_CHECKED: u32 = 1 << 2;
pub const LV_STATE_DISABLED: u32 = 1 << 9;

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
    pub fn lv_indev_set_group(indev: *mut lv_indev_t, group: *mut lv_group_t);

    // Groups (keypad focus navigation)
    pub fn lv_group_create() -> *mut lv_group_t;
    pub fn lv_group_add_obj(group: *mut lv_group_t, obj: *mut lv_obj_t);
    pub fn lv_group_set_default(group: *mut lv_group_t);
    pub fn lv_group_get_default() -> *mut lv_group_t;
    pub fn lv_group_get_focused(group: *mut lv_group_t) -> *mut lv_obj_t;

    // Screen
    pub fn lv_screen_active() -> *mut lv_obj_t;

    // Objects
    pub fn lv_obj_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_obj_clean(obj: *mut lv_obj_t);
    pub fn lv_obj_set_pos(obj: *mut lv_obj_t, x: i32, y: i32);
    /// Set just the x coordinate (preserves y). Used by ViewPropertyAnimator
    /// to animate axes independently — `lv_obj_set_pos` would clobber the
    /// other axis if a y-anim were running concurrently.
    pub fn lv_obj_set_x(obj: *mut lv_obj_t, x: i32);
    /// See [`lv_obj_set_x`].
    pub fn lv_obj_set_y(obj: *mut lv_obj_t, y: i32);
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

    /// Get the indev (touch / keypad) that originated `e`. Used by the
    /// touch-event trampolines to fetch the current pointer position.
    pub fn lv_event_get_indev(e: *mut lv_event_t) -> *mut lv_indev_t;

    /// Fill `point` with the indev's current pointer position. Result is
    /// in display pixel coordinates. For non-pointer indevs (keypad) the
    /// behavior is undefined — only call from PRESSED/PRESSING/RELEASED
    /// touch callbacks.
    pub fn lv_indev_get_point(indev: *const lv_indev_t, point: *mut lv_point_t);
    /// Synchronously fire `event_code` on `obj`. Invokes every matching
    /// event callback on this object (so e.g. `LV_EVENT_CLICKED` on a
    /// Button goes through the same `button_click_cb` a real touch would
    /// trigger). Returns `lv_result_t` (0 = OK, 1 = invalid).
    pub fn lv_obj_send_event(
        obj: *mut lv_obj_t,
        event_code: lv_event_code_t,
        param: *mut c_void,
    ) -> u32;

    // Object lifecycle
    pub fn lv_obj_delete(obj: *mut lv_obj_t);

    // Mark the entire object as needing redraw on the next refresh.
    pub fn lv_obj_invalidate(obj: *mut lv_obj_t);

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

    // Drawable styles — used by GradientDrawable to apply a bundle of
    // visual properties at once.
    pub fn lv_obj_set_style_radius(obj: *mut lv_obj_t, value: i32, selector: lv_style_selector_t);
    pub fn lv_obj_set_style_border_width(
        obj: *mut lv_obj_t,
        value: i32,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_border_color(
        obj: *mut lv_obj_t,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_bg_grad_color(
        obj: *mut lv_obj_t,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_bg_grad_dir(
        obj: *mut lv_obj_t,
        value: lv_grad_dir_t,
        selector: lv_style_selector_t,
    );

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

    // Keyboard widget
    pub fn lv_keyboard_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_keyboard_set_textarea(kb: *mut lv_obj_t, ta: *mut lv_obj_t);
    pub fn lv_keyboard_set_mode(kb: *mut lv_obj_t, mode: lv_keyboard_mode_t);
}
