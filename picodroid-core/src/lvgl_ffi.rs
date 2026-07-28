// SPDX-License-Identifier: GPL-3.0-only
//! Hand-written FFI bindings for LVGL v9.5.0.
//!
//! Only the subset of functions needed by picodroid is declared here.
//! Opaque LVGL types (lv_display_t, lv_obj_t, etc.) are represented as
//! `core::ffi::c_void` behind raw pointers.

#![allow(non_camel_case_types)]

use core::ffi::c_void;
// `c_char` is only referenced by the real LVGL `extern "C"` block below, which
// is `#[cfg(not(test))]`; gate the import to match so test builds don't warn.
#[cfg(not(test))]
use core::ffi::c_char;

// ---------------------------------------------------------------------------
// Compiler-rt intrinsic required by LVGL's TLSF allocator on Cortex-M0+
// (thumbv6m has no CLZ instruction, so __builtin_ffs maps to __ffssi2).
// Bare metal only: armv7-A (the 32-bit simulator lane) has CLZ, so nothing
// calls this there, and defining it would race libgcc's own copy.
// ---------------------------------------------------------------------------
#[cfg(all(target_arch = "arm", target_os = "none"))]
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
// No LV_INDEV_TYPE_NONE (0): we only ever register real input devices.
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
pub const LV_EVENT_GESTURE: lv_event_code_t = 16;
/// Fired when a Widget receives keypad/group focus, and when it loses it.
/// Positions 19/20 in the v9.5.0 enum (…KEY(17), ROTARY(18), FOCUSED(19),
/// DEFOCUSED(20), LEAVE…). Used to back `View.OnFocusChangeListener`.
pub const LV_EVENT_FOCUSED: lv_event_code_t = 19;
pub const LV_EVENT_DEFOCUSED: lv_event_code_t = 20;
pub const LV_EVENT_VALUE_CHANGED: lv_event_code_t = 35;
/// Fired when a multi-step interaction "completes" — used by lv_keyboard
/// when the user taps the OK / Enter key. Position 38 in the v9.5.0
/// enum (VALUE_CHANGED + 3, accounting for INSERT, REFRESH).
pub const LV_EVENT_READY: lv_event_code_t = 38;
/// Fired while an object is being deleted (also for each descendant during a
/// recursive `lv_obj_delete` / `lv_obj_clean` / screen switch). Position 42 in
/// the v9.5.0 enum: READY(38), CANCEL, STATE_CHANGED, CREATE, DELETE. Used by
/// the sim handle table to invalidate a deleted object's slot.
pub const LV_EVENT_DELETE: lv_event_code_t = 42;

pub type lv_flex_flow_t = u32;
pub const LV_FLEX_FLOW_ROW: lv_flex_flow_t = 0x00;
pub const LV_FLEX_FLOW_COLUMN: lv_flex_flow_t = 0x01;

pub type lv_flex_align_t = u32;
pub const LV_FLEX_ALIGN_START: lv_flex_align_t = 0;
pub const LV_FLEX_ALIGN_END: lv_flex_align_t = 1;
pub const LV_FLEX_ALIGN_CENTER: lv_flex_align_t = 2;

/// Linear-gradient direction. Values verified against
/// vendor/lvgl/src/misc/lv_style.h. v1 GradientDrawable only exposes
/// NONE / VER / HOR — LINEAR (arbitrary angle) and RADIAL are deferred.
pub type lv_grad_dir_t = u32;
pub const LV_GRAD_DIR_NONE: lv_grad_dir_t = 0;
pub const LV_GRAD_DIR_VER: lv_grad_dir_t = 1;
pub const LV_GRAD_DIR_HOR: lv_grad_dir_t = 2;

/// Direction bitmask used by `lv_indev_get_gesture_dir`. Verified against
/// vendor/lvgl/src/misc/lv_area.h:78-86. Only one of LEFT/RIGHT/TOP/BOTTOM
/// is set per gesture event; HOR / VER / ALL are convenience composites.
pub type lv_dir_t = u8;
pub const LV_DIR_NONE: lv_dir_t = 0x00;
pub const LV_DIR_LEFT: lv_dir_t = 1 << 0;
pub const LV_DIR_RIGHT: lv_dir_t = 1 << 1;
pub const LV_DIR_TOP: lv_dir_t = 1 << 2;
pub const LV_DIR_BOTTOM: lv_dir_t = 1 << 3;
pub const LV_DIR_HOR: lv_dir_t = LV_DIR_LEFT | LV_DIR_RIGHT;
pub const LV_DIR_VER: lv_dir_t = LV_DIR_TOP | LV_DIR_BOTTOM;

/// Scrollbar visibility mode. Values verified against
/// vendor/lvgl/src/core/lv_obj_scroll.h:31-36. Plain C enum → 1 byte under
/// `-fshort-enums` (see build_support/lvgl.rs), same as `lv_dir_t` above.
/// Only OFF is exposed for now: EditText hides scrollbars to match Android.
pub type lv_scrollbar_mode_t = u8;
pub const LV_SCROLLBAR_MODE_OFF: lv_scrollbar_mode_t = 0;

/// Roller scrolling mode (lv_roller.h:36-39). NORMAL stops at the ends;
/// INFINITE wraps around. Picodroid's TimePicker uses INFINITE so the
/// hour/minute lists feel continuous.
pub type lv_roller_mode_t = u32;
pub const LV_ROLLER_MODE_NORMAL: lv_roller_mode_t = 0;
pub const LV_ROLLER_MODE_INFINITE: lv_roller_mode_t = 1;

/// LVGL operation result (`lv_result_t`). Returned by `lv_calendar_get_pressed_date`.
pub type lv_result_t = u8;
pub const LV_RESULT_INVALID: lv_result_t = 0;
pub const LV_RESULT_OK: lv_result_t = 1;

/// `lv_calendar_date_t` (lv_calendar.h:31-35). Layout matters — picodroid
/// passes pointers to this struct into LVGL via getter/setter calls.
#[repr(C)]
#[derive(Copy, Clone, Default, Debug)]
pub struct lv_calendar_date_t {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

/// Soft-keyboard mode. Verified against
/// vendor/lvgl/src/widgets/keyboard/lv_keyboard.h. Only the four
/// "primary" modes are exposed; the various "user-defined" modes are
/// deferred until apps need custom layouts.
pub type lv_keyboard_mode_t = u32;
pub const LV_KEYBOARD_MODE_TEXT_LOWER: lv_keyboard_mode_t = 0;
// 1 = TEXT_UPPER and 2 = SPECIAL exist in LVGL but no caller selects them.
pub const LV_KEYBOARD_MODE_NUMBER: lv_keyboard_mode_t = 3;

pub type lv_style_selector_t = u32;

/// Part selectors for style setters. Verified against
/// `vendor/lvgl/src/core/lv_obj_style.h:61-63`. The selector u32 packs both
/// the part (high bits) and the state (low bits); passing `0` selects
/// `LV_PART_MAIN` in any state — matches LVGL's `lv_style_selector_default`.
pub const LV_PART_MAIN: lv_style_selector_t = 0x000000;
pub const LV_PART_INDICATOR: lv_style_selector_t = 0x020000;
/// `lv_draw_rect.h`: special radius value meaning "fully rounded" — the
/// standard recipe for a radio-style circular checkbox indicator.
pub const LV_RADIUS_CIRCLE: i32 = 0x7FFF;

// Button-matrix control flags (`lv_buttonmatrix.h`). Only the subset the
// AlertDialog choice lists need.
/// Toggle `LV_STATE_CHECKED` on the button when clicked.
pub const LV_BUTTONMATRIX_CTRL_CHECKABLE: u16 = 0x0080;
/// The button is currently checked.
pub const LV_BUTTONMATRIX_CTRL_CHECKED: u16 = 0x0100;
/// Returned by `lv_buttonmatrix_get_selected_button` when no button is selected.
pub const LV_BUTTONMATRIX_BUTTON_NONE: u32 = 0xFFFF;

/// Inner-alignment mode for `lv_image`. Verified against
/// `vendor/lvgl/src/widgets/image/lv_image.h:43-59`. The picodroid
/// `ImageView` exposes a subset that maps to Android's `ScaleType` enum;
/// `LV_IMAGE_ALIGN_CENTER` (the default after `setScaleType` is unset)
/// renders unscaled and centered.
pub type lv_image_align_t = u32;
pub const LV_IMAGE_ALIGN_CENTER: lv_image_align_t = 9;
pub const LV_IMAGE_ALIGN_STRETCH: lv_image_align_t = 11;
pub const LV_IMAGE_ALIGN_TILE: lv_image_align_t = 12;
pub const LV_IMAGE_ALIGN_CONTAIN: lv_image_align_t = 13;
pub const LV_IMAGE_ALIGN_COVER: lv_image_align_t = 14;

/// LVGL color format (`lv_color_format_t`). Values from
/// `vendor/lvgl/src/misc/lv_color.h`. Only the formats picodroid emits
/// from `papk-pack` are exposed here.
pub type lv_color_format_t = u8;
pub const LV_COLOR_FORMAT_RGB888: lv_color_format_t = 0x0F;
pub const LV_COLOR_FORMAT_ARGB8888: lv_color_format_t = 0x10;
pub const LV_COLOR_FORMAT_RGB565: lv_color_format_t = 0x12;

/// Magic byte at the top of `lv_image_header_t::magic` for a v9 image
/// descriptor. See `vendor/lvgl/src/draw/lv_image_dsc.h`.
pub const LV_IMAGE_HEADER_MAGIC: u8 = 0x19;

/// Mirror of `lv_image_header_t` (`vendor/lvgl/src/draw/lv_image_dsc.h:98-107`).
///
/// The C struct uses bitfields packed into 32-bit words. We encode the same
/// bits manually into `flags` (low byte = magic, next byte = cf, top 16 = flags)
/// so we don't depend on Rust's bitfield emulation. Layout is `#[repr(C)]`
/// with the same word order as the C struct, which matches LVGL's reads.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct lv_image_header_t {
    /// Bit 0..7: `LV_IMAGE_HEADER_MAGIC` (`0x19`).
    /// Bit 8..15: `lv_color_format_t`.
    /// Bit 16..31: image flags (`LV_IMAGE_FLAGS_*`; we leave at 0).
    pub magic_cf_flags: u32,
    pub w: u16,
    pub h: u16,
    pub stride: u16,
    pub reserved_2: u16,
}

impl lv_image_header_t {
    /// Build a header for an uncompressed, non-premultiplied RGB565/RGB888 image.
    #[inline]
    pub const fn new(cf: lv_color_format_t, w: u16, h: u16, stride: u16) -> Self {
        Self {
            magic_cf_flags: (LV_IMAGE_HEADER_MAGIC as u32) | ((cf as u32) << 8),
            w,
            h,
            stride,
            reserved_2: 0,
        }
    }
}

/// Mirror of `lv_image_dsc_t` (`vendor/lvgl/src/draw/lv_image_dsc.h:110-138`).
/// `data` points into XIP-mapped flash for bundled assets, so the descriptor
/// itself can live in RAM without needing to copy the pixel buffer.
#[repr(C)]
pub struct lv_image_dsc_t {
    pub header: lv_image_header_t,
    pub data_size: u32,
    pub data: *const u8,
    pub reserved: *const core::ffi::c_void,
    pub reserved_2: *const core::ffi::c_void,
}

// `lv_image_dsc_t` is shared with LVGL by raw pointer; the descriptor is
// immutable after init and the pixel data lives in `'static` flash, so it's
// safe to send/sync across the FreeRTOS tasks that use it.
unsafe impl Send for lv_image_dsc_t {}
unsafe impl Sync for lv_image_dsc_t {}

// Opacity constants
pub const LV_OPA_COVER: u8 = 255;

// Object flags (from lv_obj.h)
pub const LV_OBJ_FLAG_HIDDEN: u32 = 1 << 0;
pub const LV_OBJ_FLAG_CLICKABLE: u32 = 1 << 1;
pub const LV_OBJ_FLAG_CHECKABLE: u32 = 1 << 3;
pub const LV_OBJ_FLAG_SCROLLABLE: u32 = 1 << 4;

// Object states (from lv_obj_style.h, v9.5.0).
// The state bits were renumbered in v9.5.0 to leave room for LV_STATE_ALT
// (1 << 0) and reserved slots, so values differ from v9.2.2.
pub const LV_STATE_CHECKED: u32 = 1 << 2;
pub const LV_STATE_FOCUSED: u32 = 1 << 3;
// Set (in addition to LV_STATE_FOCUSED) when focus arrives via a keypad/encoder
// indev rather than a pointer. The default theme styles this state separately
// (blue), so widgets that override the focus highlight must cover it too.
pub const LV_STATE_FOCUS_KEY: u32 = 1 << 4;
// Set while a widget is being edited (keypad/encoder edit mode). The framework
// toggles it on NumberPicker during keypad edit mode for the theme-matching
// secondary outline; EDITED (1 << 5) outranks FOCUS_KEY in style specificity,
// so the edit outline wins while both states are set.
pub const LV_STATE_EDITED: u32 = 1 << 5;
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
// Excluded under `cfg(test)` so the constants + drift-check tests below
// compile on the host without linking against LVGL.

#[cfg(not(test))]
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
    /// The indev that triggered the currently-running event. Read inside an
    /// `LV_EVENT_GESTURE` callback to recover gesture parameters.
    pub fn lv_indev_active() -> *mut lv_indev_t;
    /// Direction of the most recent gesture detected on `indev`. Returns
    /// one of `LV_DIR_LEFT/RIGHT/TOP/BOTTOM` (or NONE if no gesture).
    pub fn lv_indev_get_gesture_dir(indev: *const lv_indev_t) -> lv_dir_t;
    pub fn lv_indev_set_group(indev: *mut lv_indev_t, group: *mut lv_group_t);

    // Groups (keypad focus navigation)
    pub fn lv_group_create() -> *mut lv_group_t;
    pub fn lv_group_add_obj(group: *mut lv_group_t, obj: *mut lv_obj_t);
    pub fn lv_group_set_default(group: *mut lv_group_t);
    pub fn lv_group_get_default() -> *mut lv_group_t;
    pub fn lv_group_get_focused(group: *mut lv_group_t) -> *mut lv_obj_t;
    /// Focus `obj` within its own group. No-op if `obj` belongs to no group,
    /// so it is safe to call on a non-focusable widget.
    pub fn lv_group_focus_obj(obj: *mut lv_obj_t);
    /// Remove `obj` from whatever group it belongs to. No-op if ungrouped.
    pub fn lv_group_remove_obj(obj: *mut lv_obj_t);
    /// Delete a focus group. Used to tear down a per-Activity group on pop.
    pub fn lv_group_delete(group: *mut lv_group_t);

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
    // Laid-out geometry readback for the View.getWidth/getHeight/getLeft/
    // getTop getters. Reads are only valid after a layout pass —
    // `lv_obj_update_layout` forces one (cheap no-op when nothing is dirty).
    pub fn lv_obj_get_x(obj: *const lv_obj_t) -> i32;
    pub fn lv_obj_get_y(obj: *const lv_obj_t) -> i32;
    pub fn lv_obj_get_width(obj: *const lv_obj_t) -> i32;
    pub fn lv_obj_get_height(obj: *const lv_obj_t) -> i32;
    /// Fill `coords` with the object's screen-absolute bounding box (x1/y1 =
    /// top-left). Unlike `lv_obj_get_x` (parent-relative), this gives the
    /// absolute origin needed to convert a screen touch point to
    /// view-relative MotionEvent coordinates.
    pub fn lv_obj_get_coords(obj: *const lv_obj_t, coords: *mut lv_area_t);
    pub fn lv_obj_update_layout(obj: *const lv_obj_t);
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
    /// Remove every callback descriptor on `obj` whose function matches
    /// `event_cb`. Returns the count removed. Used by the soft keyboard's
    /// press-outside dismiss to detach its screen-level press hook on hide.
    pub fn lv_obj_remove_event_cb(obj: *mut lv_obj_t, event_cb: lv_event_cb_t) -> u32;
    /// Walk one step up the widget tree. Returns null when `obj` is the
    /// active screen. Used by the keyboard outside-press hook to decide if
    /// a tap landed on the keyboard or one of its keys.
    pub fn lv_obj_get_parent(obj: *const lv_obj_t) -> *mut lv_obj_t;

    // Restrict the axes a scrollable object will scroll/over-pull on.
    // Default is LV_DIR_ALL; set to LV_DIR_VER on ScrollView so horizontal
    // drags don't trigger elastic over-pull or a transient scrollbar.
    pub fn lv_obj_set_scroll_dir(obj: *mut lv_obj_t, dir: lv_dir_t);

    // Control scrollbar drawing independently of scrollability. OFF on
    // EditText: the object stays scrollable (horizontal scroll-to-cursor
    // for long one-line text) but never draws a scrollbar, like Android.
    pub fn lv_obj_set_scrollbar_mode(obj: *mut lv_obj_t, mode: lv_scrollbar_mode_t);

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

    // Spinner widget — indeterminate counterpart of lv_bar; animates a
    // rotating arc whose duration and sweep are configurable. Used by the
    // indeterminate variant of `picodroid.widget.ProgressBar`.
    pub fn lv_spinner_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_spinner_set_anim_params(obj: *mut lv_obj_t, t_ms: u32, angle_deg: u32);

    // Switch widget
    pub fn lv_switch_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;

    // Color
    pub fn lv_color_hex(c: u32) -> lv_color_t;

    // Events
    pub fn lv_event_get_target_obj(e: *mut lv_event_t) -> *mut lv_obj_t;
    /// The `user_data` passed to `lv_obj_add_event_cb` for the descriptor whose
    /// callback is currently running. The handle table rides the slot id here so
    /// its delete callback can clear the right entry in O(1).
    pub fn lv_event_get_user_data(e: *mut lv_event_t) -> *mut c_void;
    /// The widget the firing handler is bound to. Differs from
    /// `lv_event_get_target_obj` when an event bubbles up from a child
    /// (e.g. lv_calendar's inner btnmatrix VALUE_CHANGED bubbles to the
    /// calendar root via LV_OBJ_FLAG_EVENT_BUBBLE).
    pub fn lv_event_get_current_target_obj(e: *mut lv_event_t) -> *mut lv_obj_t;

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
    pub fn lv_obj_get_child_count(obj: *const lv_obj_t) -> u32;

    // Flex layout — set_flex_flow / align live above; this declaration
    // backs `LinearLayout.LayoutParams.weight` so weighted children expand
    // along the flex axis. Maps directly to `lv_obj_set_flex_grow`.
    pub fn lv_obj_set_flex_grow(obj: *mut lv_obj_t, grow: u8);

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
    // Outline style — drawn outside the object's border. Used by NumberPicker
    // to replicate the theme's outline_primary/outline_secondary focus/edit
    // feedback, which the default theme applies to group-default widgets but
    // not to plain lv_obj containers.
    pub fn lv_obj_set_style_outline_width(
        obj: *mut lv_obj_t,
        value: i32,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_outline_pad(
        obj: *mut lv_obj_t,
        value: i32,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_outline_opa(
        obj: *mut lv_obj_t,
        value: u8,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_outline_color(
        obj: *mut lv_obj_t,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );

    // Active theme's primary/secondary accent colors (lv_theme.h). `obj` picks
    // the display whose theme is consulted; pass the widget being styled.
    pub fn lv_theme_get_color_primary(obj: *mut lv_obj_t) -> lv_color_t;
    pub fn lv_theme_get_color_secondary(obj: *mut lv_obj_t) -> lv_color_t;

    pub fn lv_obj_set_style_pad_row(obj: *mut lv_obj_t, value: i32, selector: lv_style_selector_t);
    pub fn lv_obj_set_style_pad_column(
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
    /// Set the inner alignment of the image inside its widget area. Used by
    /// `ImageView.setScaleType` to map Android's FIT_CENTER / CENTER_CROP /
    /// FIT_XY / TILE onto LVGL's `lv_image_align_t` enum.
    pub fn lv_image_set_inner_align(obj: *mut lv_obj_t, align: lv_image_align_t);
    /// Uniform image scale; `256` = 1.0×.
    pub fn lv_image_set_scale(obj: *mut lv_obj_t, zoom: u32);

    /// Image recolor (tint) — applies a tint color to the rendered image.
    /// Recolor is a *style* property on the lv_image widget; pair with
    /// `lv_obj_set_style_image_recolor_opa(..., 0..255, ...)` to control
    /// blend strength (0 = no tint, 255 = fully recolored).
    pub fn lv_obj_set_style_image_recolor(
        obj: *mut lv_obj_t,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_image_recolor_opa(
        obj: *mut lv_obj_t,
        value: u8,
        selector: lv_style_selector_t,
    );

    // Arc style — used by `lv_arc` and its subclass `lv_spinner`. Pair with
    // a part selector (e.g. `LV_PART_INDICATOR` for the moving sweep,
    // `LV_PART_MAIN` for the background ring).
    pub fn lv_obj_set_style_arc_color(
        obj: *mut lv_obj_t,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_arc_width(
        obj: *mut lv_obj_t,
        value: i32,
        selector: lv_style_selector_t,
    );
    pub fn lv_obj_set_style_arc_rounded(
        obj: *mut lv_obj_t,
        value: bool,
        selector: lv_style_selector_t,
    );

    // Slider widget
    pub fn lv_slider_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_slider_set_value(obj: *mut lv_obj_t, value: i32, anim: lv_anim_enable_t);
    pub fn lv_slider_set_range(obj: *mut lv_obj_t, min: i32, max: i32);
    pub fn lv_slider_get_value(obj: *const lv_obj_t) -> i32;

    // Checkbox widget
    pub fn lv_checkbox_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_checkbox_set_text(obj: *mut lv_obj_t, txt: *const c_char);

    // Button-matrix widget — one focusable lv_obj rendering a grid of
    // buttons from a NUL-terminated `map` of `*const c_char` (with `"\n"`
    // entries as row breaks and a final null entry). Backs AlertDialog's
    // item / choice lists: a single widget regardless of row count dodges
    // the >12-focusable-lv_list-rows renderer hang. The `map` array (and
    // the CStrings it points at) must outlive the widget — callers own that
    // storage. Selecting fires LV_EVENT_VALUE_CHANGED;
    // `get_selected_button` returns `LV_BUTTONMATRIX_BUTTON_NONE` (0xFFFF)
    // when nothing is selected.
    pub fn lv_buttonmatrix_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_buttonmatrix_set_map(obj: *mut lv_obj_t, map: *const *const c_char);
    pub fn lv_buttonmatrix_set_button_ctrl(obj: *mut lv_obj_t, btn_id: u32, ctrl: u16);
    pub fn lv_buttonmatrix_clear_button_ctrl(obj: *mut lv_obj_t, btn_id: u32, ctrl: u16);
    pub fn lv_buttonmatrix_set_one_checked(obj: *mut lv_obj_t, en: bool);
    pub fn lv_buttonmatrix_get_selected_button(obj: *const lv_obj_t) -> u32;
    pub fn lv_buttonmatrix_has_button_ctrl(obj: *mut lv_obj_t, btn_id: u32, ctrl: u16) -> bool;

    // Dropdown widget
    pub fn lv_dropdown_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_dropdown_set_options(obj: *mut lv_obj_t, options: *const c_char);
    pub fn lv_dropdown_get_selected(obj: *const lv_obj_t) -> u32;

    // Roller widget — vertically-scrolling option list. Used by TimePicker
    // (hour + minute) and could back a future date-roller variant.
    pub fn lv_roller_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_roller_set_options(
        obj: *mut lv_obj_t,
        options: *const c_char,
        mode: lv_roller_mode_t,
    );
    pub fn lv_roller_set_selected(obj: *mut lv_obj_t, sel_opt: u32, anim: lv_anim_enable_t);
    pub fn lv_roller_get_selected(obj: *const lv_obj_t) -> u32;
    pub fn lv_roller_set_visible_row_count(obj: *mut lv_obj_t, row_cnt: u32);

    // Calendar widget — backs DatePicker. `lv_calendar_get_pressed_date`
    // populates a caller-allocated `lv_calendar_date_t`.
    pub fn lv_calendar_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_calendar_set_today_date(obj: *mut lv_obj_t, year: u32, month: u32, day: u32);
    pub fn lv_calendar_set_month_shown(obj: *mut lv_obj_t, year: u32, month: u32);
    pub fn lv_calendar_get_pressed_date(
        obj: *const lv_obj_t,
        date: *mut lv_calendar_date_t,
    ) -> lv_result_t;

    // Textarea widget
    pub fn lv_textarea_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_textarea_set_text(obj: *mut lv_obj_t, txt: *const c_char);
    pub fn lv_textarea_get_text(obj: *const lv_obj_t) -> *const c_char;
    pub fn lv_textarea_set_placeholder_text(obj: *mut lv_obj_t, txt: *const c_char);
    pub fn lv_textarea_set_one_line(obj: *mut lv_obj_t, en: bool);

    // Keyboard widget
    pub fn lv_keyboard_create(parent: *mut lv_obj_t) -> *mut lv_obj_t;
    pub fn lv_keyboard_set_textarea(kb: *mut lv_obj_t, ta: *mut lv_obj_t);
    pub fn lv_keyboard_set_mode(kb: *mut lv_obj_t, mode: lv_keyboard_mode_t);
}

#[cfg(test)]
mod abi_tests {
    //! Cross-language check that C and Rust agree on `lv_indev_data_t`.
    //!
    //! Every enum in this file is typed as `u8`, which is only true while the
    //! C compiler applies `-fshort-enums`. A wider enum moves `state`, `point`
    //! and `key` to offsets LVGL never writes, so touch and key input would
    //! read garbage rather than fail loudly. `build_support/lvgl.rs` compiles
    //! `enum_abi_check.c` next to LVGL to export C's own `sizeof`; this test
    //! is the half that pins the Rust mirror to it. Both halves run on the
    //! x86 host suite and on the 32-bit `armv7` simulator lane, whose
    //! `arm-linux-gnueabihf-gcc` is the compiler that made the flag load-bearing.
    extern "C" {
        static pd_c_sizeof_lv_indev_data: core::ffi::c_uint;
    }

    #[test]
    fn lv_indev_data_layout_matches_c() {
        let c_size = unsafe { pd_c_sizeof_lv_indev_data } as usize;
        assert_eq!(
            c_size,
            core::mem::size_of::<super::lv_indev_data_t>(),
            "lv_indev_data_t size disagrees between C and lvgl_ffi.rs"
        );
    }
}

#[cfg(test)]
mod tests {
    //! Drift guard against the vendored LVGL `lv_event.h`. Per
    //! `project_lvgl_ffi_constants.md`: wrong event codes silently route
    //! to wrong handlers, which can cause infinite render loops. The
    //! enum is implicit-ordinal (no explicit `= N` after `LV_EVENT_ALL = 0`),
    //! so a single header edit shifts every code below it by one. This test
    //! parses the enum from the vendored header at compile time and asserts
    //! the Rust constants we depend on still match.
    use super::*;

    const LV_EVENT_HEADER: &str = include_str!("../../vendor/lvgl/src/misc/lv_event.h");

    /// Extract the ordinal of `name` from the `lv_event_code_t` enum body.
    /// Returns `None` if the enum or the name is missing.
    fn lookup_event_ordinal(name: &str) -> Option<u32> {
        // The enum opens with `typedef enum {` and `LV_EVENT_ALL = 0,`
        // explicitly anchors the count. Walk forward from there, counting
        // each `LV_EVENT_<NAME>` identifier at start-of-line (after
        // whitespace) until either `name` matches or `}` ends the enum.
        let start = LV_EVENT_HEADER.find("LV_EVENT_ALL = 0")?;
        let end = LV_EVENT_HEADER[start..].find("} lv_event_code_t")?;
        let body = &LV_EVENT_HEADER[start..start + end];
        let mut ordinal: u32 = 0;
        for line in body.lines() {
            let trimmed = line.trim_start();
            if let Some(ident_end) = trimmed.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            {
                let ident = &trimmed[..ident_end];
                if !ident.starts_with("LV_EVENT_") {
                    continue;
                }
                if ident == name {
                    return Some(ordinal);
                }
                ordinal += 1;
            }
        }
        None
    }

    #[test]
    fn lv_event_constants_match_vendored_header() {
        for (rust_const, name) in [
            (LV_EVENT_ALL, "LV_EVENT_ALL"),
            (LV_EVENT_PRESSED, "LV_EVENT_PRESSED"),
            (LV_EVENT_PRESSING, "LV_EVENT_PRESSING"),
            (LV_EVENT_LONG_PRESSED, "LV_EVENT_LONG_PRESSED"),
            (LV_EVENT_CLICKED, "LV_EVENT_CLICKED"),
            (LV_EVENT_RELEASED, "LV_EVENT_RELEASED"),
            (LV_EVENT_GESTURE, "LV_EVENT_GESTURE"),
            (LV_EVENT_VALUE_CHANGED, "LV_EVENT_VALUE_CHANGED"),
            (LV_EVENT_READY, "LV_EVENT_READY"),
            (LV_EVENT_FOCUSED, "LV_EVENT_FOCUSED"),
            (LV_EVENT_DEFOCUSED, "LV_EVENT_DEFOCUSED"),
            (LV_EVENT_DELETE, "LV_EVENT_DELETE"),
        ] {
            let header_ord = lookup_event_ordinal(name)
                .unwrap_or_else(|| panic!("{} not found in vendored lv_event.h", name));
            assert_eq!(
                rust_const, header_ord,
                "{}: Rust FFI ({}) drifted from vendored header ({}). \
                 The LVGL enum is implicit-ordinal — a single inserted variant \
                 shifts everything below it. Re-sync lvgl_ffi.rs to match the \
                 vendored lv_event.h before this slips into a runtime bug.",
                name, rust_const, header_ord
            );
        }
    }

    /// Anchor test — if the lookup helper itself silently returns wrong
    /// ordinals, every event assertion above passes vacuously. Pin the
    /// known-good first three to catch that.
    #[test]
    fn lookup_helper_yields_known_ordinals() {
        assert_eq!(lookup_event_ordinal("LV_EVENT_ALL"), Some(0));
        assert_eq!(lookup_event_ordinal("LV_EVENT_PRESSED"), Some(1));
        assert_eq!(lookup_event_ordinal("LV_EVENT_PRESSING"), Some(2));
        assert_eq!(lookup_event_ordinal("LV_EVENT_DOES_NOT_EXIST"), None);
    }

    // ── Drift guards for the other hand-maintained constant families ────────
    //
    // Same threat model as the event guard: the vendored headers are the only
    // source of truth, and a value shift (the v9.5.0 LV_STATE_* renumbering,
    // an inserted enum variant) silently becomes wrong callbacks, wrong style
    // bits, or corrupted papk assets. Deliberately unguarded, with rationale:
    //  - composite/alias values (LV_DIR_HOR/VER, LV_FLEX_FLOW_COLUMN
    //    = LV_FLEX_COLUMN, LV_COLOR_FORMAT_NATIVE): identifier RHS —
    //    `eval_c_const` refuses them by design; the Rust side derives them
    //    from guarded primitives or does not use them.
    //  - trivially-stable one-off families (LV_INDEV_TYPE_*,
    //    LV_KEYBOARD_MODE_*, LV_OPA_COVER, LV_DISPLAY_RENDER_MODE_*,
    //    LV_GRAD_DIR_*, LV_SCROLLBAR_MODE_*): tiny enums whose upstream
    //    reshuffling is near-inconceivable relative to parse maintenance.

    const LV_GROUP_HEADER: &str = include_str!("../../vendor/lvgl/src/core/lv_group.h");
    const LV_OBJ_STYLE_HEADER: &str = include_str!("../../vendor/lvgl/src/core/lv_obj_style.h");
    const LV_OBJ_HEADER: &str = include_str!("../../vendor/lvgl/src/core/lv_obj.h");
    const LV_COLOR_HEADER: &str = include_str!("../../vendor/lvgl/src/misc/lv_color.h");
    const LV_AREA_HEADER: &str = include_str!("../../vendor/lvgl/src/misc/lv_area.h");
    const LV_FLEX_HEADER: &str = include_str!("../../vendor/lvgl/src/layouts/flex/lv_flex.h");
    const LV_BTNMATRIX_HEADER: &str =
        include_str!("../../vendor/lvgl/src/widgets/buttonmatrix/lv_buttonmatrix.h");
    const LV_IMAGE_HEADER: &str = include_str!("../../vendor/lvgl/src/widgets/image/lv_image.h");
    const LV_IMAGE_DSC_HEADER: &str = include_str!("../../vendor/lvgl/src/draw/lv_image_dsc.h");
    const LV_DRAW_RECT_HEADER: &str = include_str!("../../vendor/lvgl/src/draw/lv_draw_rect.h");

    /// Slice one enum body out of a header that may contain several enums:
    /// find the closing anchor (e.g. `"} lv_key_t"`) and walk back to the
    /// nearest preceding `typedef enum`.
    fn enum_body<'a>(header: &'a str, close_anchor: &str) -> Option<&'a str> {
        let close = header.find(close_anchor)?;
        let open = header[..close].rfind("typedef enum")?;
        Some(&header[open..close])
    }

    /// Evaluate the C constant expressions these enums actually use: decimal,
    /// 0x/0X hex, `A << B` with optional u/U/l/L suffix on A, one optional
    /// layer of surrounding parens. Identifier RHS (`= LV_FOO`) and
    /// OR-composites return None — deliberately: none of the guarded
    /// constants need them, and a tiny evaluator stays auditable.
    fn eval_c_const(expr: &str) -> Option<u32> {
        let mut e = expr.trim();
        if let Some(inner) = e.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            e = inner.trim();
        }
        if let Some((lhs, rhs)) = e.split_once("<<") {
            let base: u32 = lhs
                .trim()
                .trim_end_matches(['u', 'U', 'l', 'L'])
                .parse()
                .ok()?;
            let shift: u32 = rhs.trim().parse().ok()?;
            return base.checked_shl(shift);
        }
        if let Some(hex) = e.strip_prefix("0x").or_else(|| e.strip_prefix("0X")) {
            return u32::from_str_radix(hex, 16).ok();
        }
        e.parse().ok()
    }

    /// Find `name` in an explicit-value enum body and evaluate its RHS.
    /// First match wins. Note two intentional conflations: "name absent" and
    /// "name found but RHS unparsable" both yield None (the guard tests panic
    /// on None either way), and a matched line without `=` aborts the scan
    /// via `?` rather than continuing — fine for every guarded name, which
    /// all carry explicit values.
    fn lookup_assigned_value(body: &str, name: &str) -> Option<u32> {
        for line in body.lines() {
            let trimmed = line.trim_start();
            let Some(ident_end) = trimmed.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            else {
                continue;
            };
            if &trimmed[..ident_end] != name {
                continue;
            }
            let rhs = trimmed[ident_end..].split_once('=')?.1;
            let rhs = rhs.split(',').next().unwrap_or(rhs);
            let rhs = rhs.split("/*").next().unwrap_or(rhs);
            return eval_c_const(rhs);
        }
        None
    }

    /// Ordinal lookup for implicit-value enums, generalizing
    /// `lookup_event_ordinal`: members matching `prefix` OR starting with
    /// `_` (private members like `_LV_IMAGE_ALIGN_AUTO_TRANSFORM` still
    /// consume an ordinal) are counted; an explicit parsable `= N` resets
    /// the counter to N, so mixed enums work too.
    fn lookup_ordinal(body: &str, prefix: &str, name: &str) -> Option<u32> {
        let mut ordinal: u32 = 0;
        for line in body.lines() {
            let trimmed = line.trim_start();
            let Some(ident_end) = trimmed.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            else {
                continue;
            };
            let ident = &trimmed[..ident_end];
            if ident.is_empty() || !(ident.starts_with(prefix) || ident.starts_with('_')) {
                continue;
            }
            if let Some((_, rhs)) = trimmed[ident_end..].split_once('=') {
                let rhs = rhs.split(',').next().unwrap_or(rhs);
                let rhs = rhs.split("/*").next().unwrap_or(rhs);
                if let Some(v) = eval_c_const(rhs) {
                    ordinal = v;
                }
            }
            if ident == name {
                return Some(ordinal);
            }
            ordinal += 1;
        }
        None
    }

    /// Look up a `#define NAME <value>` constant. Handles the parenthesized
    /// form `(0x19)` via `eval_c_const`'s paren stripping.
    fn lookup_define(header: &str, name: &str) -> Option<u32> {
        for line in header.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("#define ") else {
                continue;
            };
            let rest = rest.trim_start();
            let Some(after) = rest.strip_prefix(name) else {
                continue;
            };
            // Reject prefix matches (e.g. NAME_FOO when looking for NAME).
            if after
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                continue;
            }
            let value = after.split("/*").next().unwrap_or(after);
            let value = value.split("//").next().unwrap_or(value);
            return eval_c_const(value);
        }
        None
    }

    #[test]
    fn lv_key_constants_match_vendored_header() {
        let body = enum_body(LV_GROUP_HEADER, "} lv_key_t").expect("lv_key_t enum not found");
        for (rust_const, name) in [
            (LV_KEY_UP, "LV_KEY_UP"),
            (LV_KEY_DOWN, "LV_KEY_DOWN"),
            (LV_KEY_RIGHT, "LV_KEY_RIGHT"),
            (LV_KEY_LEFT, "LV_KEY_LEFT"),
            (LV_KEY_ESC, "LV_KEY_ESC"),
            (LV_KEY_ENTER, "LV_KEY_ENTER"),
            (LV_KEY_NEXT, "LV_KEY_NEXT"),
            (LV_KEY_PREV, "LV_KEY_PREV"),
        ] {
            let header_val = lookup_assigned_value(body, name)
                .unwrap_or_else(|| panic!("{name} not found/parsable in vendored lv_group.h"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored lv_group.h — board button \
                 tables (build.rs BUTTONS from board.toml lv_key) and keypad nav \
                 route through these codes."
            );
        }
    }

    #[test]
    fn lv_state_constants_match_vendored_header() {
        let body =
            enum_body(LV_OBJ_STYLE_HEADER, "} lv_state_t").expect("lv_state_t enum not found");
        for (rust_const, name) in [
            (LV_STATE_CHECKED, "LV_STATE_CHECKED"),
            (LV_STATE_FOCUSED, "LV_STATE_FOCUSED"),
            (LV_STATE_FOCUS_KEY, "LV_STATE_FOCUS_KEY"),
            (LV_STATE_EDITED, "LV_STATE_EDITED"),
            (LV_STATE_DISABLED, "LV_STATE_DISABLED"),
        ] {
            let header_val = lookup_assigned_value(body, name)
                .unwrap_or_else(|| panic!("{name} not found/parsable in vendored lv_obj_style.h"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored lv_obj_style.h — these \
                 state bits were renumbered once already (v9.5.0), which is \
                 exactly the drift this guard exists to catch."
            );
        }
    }

    #[test]
    fn lv_part_constants_match_vendored_header() {
        let body = enum_body(LV_OBJ_STYLE_HEADER, "} lv_part_t").expect("lv_part_t enum not found");
        for (rust_const, name) in [
            (LV_PART_MAIN, "LV_PART_MAIN"),
            (LV_PART_INDICATOR, "LV_PART_INDICATOR"),
        ] {
            let header_val = lookup_assigned_value(body, name)
                .unwrap_or_else(|| panic!("{name} not found/parsable in vendored lv_obj_style.h"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored lv_obj_style.h — style \
                 setters would target the wrong widget part."
            );
        }
    }

    #[test]
    fn lv_obj_flag_constants_match_vendored_header() {
        let body = enum_body(LV_OBJ_HEADER, "} lv_obj_flag_t").expect("lv_obj_flag_t not found");
        for (rust_const, name) in [
            (LV_OBJ_FLAG_HIDDEN, "LV_OBJ_FLAG_HIDDEN"),
            (LV_OBJ_FLAG_CLICKABLE, "LV_OBJ_FLAG_CLICKABLE"),
            (LV_OBJ_FLAG_CHECKABLE, "LV_OBJ_FLAG_CHECKABLE"),
            (LV_OBJ_FLAG_SCROLLABLE, "LV_OBJ_FLAG_SCROLLABLE"),
        ] {
            let header_val = lookup_assigned_value(body, name)
                .unwrap_or_else(|| panic!("{name} not found/parsable in vendored lv_obj.h"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored lv_obj.h — visibility, \
                 clickability and scroll behavior flags would silently target \
                 the wrong bits."
            );
        }
    }

    #[test]
    fn lv_color_format_constants_match_vendored_header() {
        let body =
            enum_body(LV_COLOR_HEADER, "} lv_color_format_t").expect("lv_color_format_t not found");
        for (rust_const, name) in [
            (LV_COLOR_FORMAT_RGB888, "LV_COLOR_FORMAT_RGB888"),
            (LV_COLOR_FORMAT_ARGB8888, "LV_COLOR_FORMAT_ARGB8888"),
            (LV_COLOR_FORMAT_RGB565, "LV_COLOR_FORMAT_RGB565"),
        ] {
            let header_val = lookup_assigned_value(body, name)
                .unwrap_or_else(|| panic!("{name} not found/parsable in vendored lv_color.h"));
            assert_eq!(
                rust_const as u32, header_val,
                "{name}: Rust FFI drifted from vendored lv_color.h — papk-pack \
                 bakes this byte into every image asset; drift means silently \
                 corrupted rendering."
            );
        }
    }

    #[test]
    fn lv_image_align_constants_match_vendored_header() {
        let body =
            enum_body(LV_IMAGE_HEADER, "} lv_image_align_t").expect("lv_image_align_t not found");
        for (rust_const, name) in [
            (LV_IMAGE_ALIGN_CENTER, "LV_IMAGE_ALIGN_CENTER"),
            (LV_IMAGE_ALIGN_STRETCH, "LV_IMAGE_ALIGN_STRETCH"),
            (LV_IMAGE_ALIGN_TILE, "LV_IMAGE_ALIGN_TILE"),
            (LV_IMAGE_ALIGN_CONTAIN, "LV_IMAGE_ALIGN_CONTAIN"),
            (LV_IMAGE_ALIGN_COVER, "LV_IMAGE_ALIGN_COVER"),
        ] {
            let header_val = lookup_ordinal(body, "LV_IMAGE_ALIGN_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_image.h"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored lv_image.h — this enum \
                 is implicit-ordinal (with a private _AUTO_TRANSFORM member \
                 consuming ordinal 10), so one inserted variant shifts \
                 ImageView's ScaleType mapping."
            );
        }
    }

    #[test]
    fn lv_dir_constants_match_vendored_header() {
        let body = enum_body(LV_AREA_HEADER, "} lv_dir_t").expect("lv_dir_t enum not found");
        for (rust_const, name) in [
            (LV_DIR_NONE, "LV_DIR_NONE"),
            (LV_DIR_LEFT, "LV_DIR_LEFT"),
            (LV_DIR_RIGHT, "LV_DIR_RIGHT"),
            (LV_DIR_TOP, "LV_DIR_TOP"),
            (LV_DIR_BOTTOM, "LV_DIR_BOTTOM"),
        ] {
            let header_val = lookup_assigned_value(body, name)
                .unwrap_or_else(|| panic!("{name} not found/parsable in vendored lv_area.h"));
            assert_eq!(
                rust_const as u32, header_val,
                "{name}: Rust FFI drifted from vendored lv_area.h — gesture \
                 direction decoding (swipe events) reads these bits. HOR/VER \
                 composites are derived in Rust from these guarded primitives."
            );
        }
    }

    #[test]
    fn lv_flex_constants_match_vendored_header() {
        let align =
            enum_body(LV_FLEX_HEADER, "} lv_flex_align_t").expect("lv_flex_align_t not found");
        for (rust_const, name) in [
            (LV_FLEX_ALIGN_START, "LV_FLEX_ALIGN_START"),
            (LV_FLEX_ALIGN_END, "LV_FLEX_ALIGN_END"),
            (LV_FLEX_ALIGN_CENTER, "LV_FLEX_ALIGN_CENTER"),
        ] {
            let header_val = lookup_ordinal(align, "LV_FLEX_ALIGN_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_flex.h"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored lv_flex.h — \
                 LinearLayout gravity mapping routes through these."
            );
        }
        let flow = enum_body(LV_FLEX_HEADER, "} lv_flex_flow_t").expect("lv_flex_flow_t not found");
        let row = lookup_assigned_value(flow, "LV_FLEX_FLOW_ROW")
            .expect("LV_FLEX_FLOW_ROW not found/parsable in vendored lv_flex.h");
        assert_eq!(LV_FLEX_FLOW_ROW, row, "LV_FLEX_FLOW_ROW drifted");
        // LV_FLEX_FLOW_COLUMN is `= LV_FLEX_COLUMN` (macro alias) in the
        // header — unguardable by the deliberately-minimal evaluator. Its
        // Rust value (0x01) is protected indirectly: ROW is guarded, and a
        // COLUMN repack upstream would be a flex redesign, not a drift.
    }

    #[test]
    fn lv_buttonmatrix_constants_match_vendored_header() {
        let body = enum_body(LV_BTNMATRIX_HEADER, "} lv_buttonmatrix_ctrl_t")
            .expect("lv_buttonmatrix_ctrl_t not found");
        for (rust_const, name) in [
            (
                LV_BUTTONMATRIX_CTRL_CHECKABLE as u32,
                "LV_BUTTONMATRIX_CTRL_CHECKABLE",
            ),
            (
                LV_BUTTONMATRIX_CTRL_CHECKED as u32,
                "LV_BUTTONMATRIX_CTRL_CHECKED",
            ),
        ] {
            let header_val = lookup_assigned_value(body, name)
                .unwrap_or_else(|| panic!("{name} not found/parsable in lv_buttonmatrix.h"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored lv_buttonmatrix.h — \
                 AlertDialog choice lists set these control bits."
            );
        }
    }

    #[test]
    fn lv_define_constants_match_vendored_headers() {
        for (rust_const, name, header, header_name) in [
            (
                LV_BUTTONMATRIX_BUTTON_NONE,
                "LV_BUTTONMATRIX_BUTTON_NONE",
                LV_BTNMATRIX_HEADER,
                "lv_buttonmatrix.h",
            ),
            (
                LV_IMAGE_HEADER_MAGIC as u32,
                "LV_IMAGE_HEADER_MAGIC",
                LV_IMAGE_DSC_HEADER,
                "lv_image_dsc.h",
            ),
            (
                LV_RADIUS_CIRCLE as u32,
                "LV_RADIUS_CIRCLE",
                LV_DRAW_RECT_HEADER,
                "lv_draw_rect.h",
            ),
        ] {
            let header_val = lookup_define(header, name)
                .unwrap_or_else(|| panic!("{name} #define not found/parsable in {header_name}"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored {header_name}."
            );
        }
    }

    /// Anchor tests — pin values that exist ONLY in the headers (not in
    /// lvgl_ffi.rs) wherever possible, so a parser bug that echoed Rust-side
    /// values could never pass vacuously.
    #[test]
    fn assigned_lookup_yields_known_values() {
        let keys = enum_body(LV_GROUP_HEADER, "} lv_key_t").unwrap();
        assert_eq!(lookup_assigned_value(keys, "LV_KEY_BACKSPACE"), Some(8));
        assert_eq!(lookup_assigned_value(keys, "LV_KEY_DEL"), Some(127));
        assert_eq!(lookup_assigned_value(keys, "LV_KEY_DOES_NOT_EXIST"), None);

        let states = enum_body(LV_OBJ_STYLE_HEADER, "} lv_state_t").unwrap();
        assert_eq!(lookup_assigned_value(states, "LV_STATE_DEFAULT"), Some(0));
        assert_eq!(
            lookup_assigned_value(states, "LV_STATE_HOVERED"),
            Some(1 << 6)
        );
        assert_eq!(lookup_assigned_value(states, "LV_STATE_ANY"), Some(0xFFFF));

        let parts = enum_body(LV_OBJ_STYLE_HEADER, "} lv_part_t").unwrap();
        assert_eq!(
            lookup_assigned_value(parts, "LV_PART_SCROLLBAR"),
            Some(0x010000)
        );
        assert_eq!(lookup_assigned_value(parts, "LV_PART_KNOB"), Some(0x030000));

        let flags = enum_body(LV_OBJ_HEADER, "} lv_obj_flag_t").unwrap();
        assert_eq!(
            lookup_assigned_value(flags, "LV_OBJ_FLAG_CLICK_FOCUSABLE"),
            Some(1 << 2)
        );
        assert_eq!(
            lookup_assigned_value(flags, "LV_OBJ_FLAG_ADV_HITTEST"),
            Some(1 << 16)
        );
        // Composite RHS must refuse — documents the evaluator's limit.
        assert_eq!(
            lookup_assigned_value(flags, "LV_OBJ_FLAG_SCROLL_CHAIN"),
            None
        );

        let cfs = enum_body(LV_COLOR_HEADER, "} lv_color_format_t").unwrap();
        assert_eq!(
            lookup_assigned_value(cfs, "LV_COLOR_FORMAT_ARGB2222"),
            Some(0x18)
        );
        // Alias RHS (= LV_COLOR_FORMAT_YUV_START) must refuse.
        assert_eq!(lookup_assigned_value(cfs, "LV_COLOR_FORMAT_I420"), None);
    }

    #[test]
    fn ordinal_lookup_yields_known_values() {
        let aligns = enum_body(LV_IMAGE_HEADER, "} lv_image_align_t").unwrap();
        // Header-only names; RIGHT_MID=8 also proves counting crosses the
        // whole implicit run, and STRETCH=11 (guarded above) proves the
        // underscore-prefixed _AUTO_TRANSFORM consumed ordinal 10.
        assert_eq!(
            lookup_ordinal(aligns, "LV_IMAGE_ALIGN_", "LV_IMAGE_ALIGN_TOP_LEFT"),
            Some(1)
        );
        assert_eq!(
            lookup_ordinal(aligns, "LV_IMAGE_ALIGN_", "LV_IMAGE_ALIGN_RIGHT_MID"),
            Some(8)
        );
        assert_eq!(
            lookup_ordinal(aligns, "LV_IMAGE_ALIGN_", "LV_IMAGE_ALIGN_NOPE"),
            None
        );

        let flex = enum_body(LV_FLEX_HEADER, "} lv_flex_align_t").unwrap();
        assert_eq!(
            lookup_ordinal(flex, "LV_FLEX_ALIGN_", "LV_FLEX_ALIGN_SPACE_EVENLY"),
            Some(3)
        );
    }

    #[test]
    fn define_lookup_yields_known_values() {
        // Parenthesized form (0x19) and plain forms both parse; prefix
        // matches (a longer #define starting with the same name) refuse.
        assert_eq!(
            lookup_define(LV_IMAGE_DSC_HEADER, "LV_IMAGE_HEADER_MAGIC"),
            Some(0x19)
        );
        assert_eq!(
            lookup_define(LV_DRAW_RECT_HEADER, "LV_RADIUS_CIRCLE"),
            Some(0x7FFF)
        );
        assert_eq!(lookup_define(LV_DRAW_RECT_HEADER, "LV_RADIUS_NOPE"), None);
    }

    #[test]
    fn eval_c_const_handles_all_header_forms() {
        assert_eq!(eval_c_const("17"), Some(17));
        assert_eq!(eval_c_const("0x0F"), Some(0x0F));
        assert_eq!(eval_c_const("0X18"), Some(0x18));
        assert_eq!(eval_c_const("1 << 9"), Some(512));
        assert_eq!(eval_c_const("(1u << 20)"), Some(1 << 20));
        assert_eq!(eval_c_const("LV_OBJ_FLAG_LAYOUT_1"), None);
        assert_eq!(eval_c_const("(A | B)"), None);
    }
}
