// SPDX-License-Identifier: GPL-3.0-only
//! Hand-written FFI bindings for LVGL v9.6.0, plus the build of the vendored
//! LVGL C sources they bind (`build.rs`, `lvgl/lv_conf.h`).
//!
//! Only the subset of functions needed by picodroid is declared here.
//! Opaque LVGL types (lv_display_t, lv_obj_t, etc.) are represented as
//! `core::ffi::c_void`; the bindings take them as the typed handles [`Obj`],
//! [`Group`], [`Indev`] and [`Display`], whose docs carry the argument for
//! why a call that takes only those and scalars is `safe`.
//!
//! The crate declares `links = "lvgl"`: exactly one crate in a build may
//! compile LVGL, because a `cc` static lib reaches the final binary through
//! the dependency and a second builder means duplicate symbols.
//! picodroid-core re-exports this crate as `picodroid_core::lvgl_ffi`.

#![cfg_attr(not(test), no_std)]
#![allow(non_camel_case_types)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use core::ffi::c_void;
// `c_char` and `CStr` are only referenced by the real LVGL `extern "C"` block
// below and its safe forms, which are `#[cfg(not(test))]`; gate the imports
// to match so test builds don't warn.
#[cfg(not(test))]
use core::ffi::{c_char, CStr};
use core::marker::PhantomData;
use core::ptr::NonNull;

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
/// A widget class descriptor (`lv_obj_class_t`), opaque: only ever compared
/// by address against the `lv_*_class` statics.
pub type lv_obj_class_t = c_void;
/// Opaque `lv_font_t`; only ever handed back to LVGL (`lv_font_get_line_height`,
/// `lv_obj_set_style_text_font`).
pub type lv_font_t = c_void;

/// One row of the face table in `lvgl/pd_fonts.c`: a compiled face and its pixel size. The
/// pointer comes first so the layout is the same on ARM32 and x86_64.
#[repr(C)]
pub struct pd_font_t {
    pub font: *const lv_font_t,
    pub px: u8,
}

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

/// `lv_style_value_t` — what `lv_obj_get_style_prop` returns. Numeric props
/// (opacity, translate, transform_*) come back in `num`. Pointer-sized on
/// every target, so it is returned in a register exactly as C does.
#[repr(C)]
#[derive(Copy, Clone)]
pub union lv_style_value_t {
    pub num: i32,
    pub ptr: *const c_void,
    pub color: lv_color_t,
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
/// and v9.6.0 both have 6 gesture types (NONE, PINCH, SWIPE, ROTATE,
/// TWO_FINGERS_SWIPE, SCROLL).
const LV_INDEV_GESTURE_CNT: usize = 6;

/// `lv_indev_gesture_type_t` is a C enum compiled with `-fshort-enums` on
/// both ARM and sim (build_support/lvgl.rs forces the flag on non-ARM), so
/// it is a single byte.
pub type lv_indev_gesture_type_t = u8;

/// Layout matches `lv_indev_data_t` in third_party/lvgl/include/lvgl/indev/lv_indev.h.
/// The leading `gesture_type` / `gesture_data` arrays were added in v9.5.0
/// and `state` was reordered to precede `point`; v9.6.0 leaves the struct
/// untouched (88 bytes on x86_64, `state` at 56). We don't fill the gesture
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
// Values verified against third_party/lvgl/include/lvgl/core/lv_event.h in LVGL 9.6.0.
// See project_lvgl_ffi_constants.md memory: wrong values silently route to
// wrong handlers and can cause infinite render loops.
//
// The enum is reshuffled nearly every release. v9.5.0 inserted SINGLE_CLICKED /
// DOUBLE_CLICKED / TRIPLE_CLICKED before LONG_PRESSED (+3 vs v9.2.2); v9.6.0
// inserted GESTURE_UP / GESTURE_DOWN / GESTURE_LEFT / GESTURE_RIGHT after
// GESTURE (+4 from KEY onwards) and CHECKED / UNCHECKED after STATE_CHANGED
// (+2 more, so +6 from CREATE onwards). The `event_codes_match_header` test
// below re-derives every value from the header and is the real guard.
pub const LV_EVENT_ALL: lv_event_code_t = 0;
pub const LV_EVENT_PRESSED: lv_event_code_t = 1;
pub const LV_EVENT_PRESSING: lv_event_code_t = 2;
pub const LV_EVENT_LONG_PRESSED: lv_event_code_t = 8;
/// Sent every `long_press_repeat_time` after LONG_PRESSED while the press
/// goes on: the clock behind hold-for-HOME on the soft-nav control.
pub const LV_EVENT_LONG_PRESSED_REPEAT: lv_event_code_t = 9;
pub const LV_EVENT_CLICKED: lv_event_code_t = 10;
pub const LV_EVENT_RELEASED: lv_event_code_t = 11;
pub const LV_EVENT_GESTURE: lv_event_code_t = 16;
/// A key reached the focused widget (after the widget's class acted on it).
/// Position 21 in the v9.6.0 enum. What tells a TimePicker roller that the
/// keys changed it, since `lv_roller_set_selected` sends no VALUE_CHANGED.
pub const LV_EVENT_KEY: lv_event_code_t = 21;
/// Fired when a Widget receives keypad/group focus, and when it loses it.
/// Positions 23/24 in the v9.6.0 enum (…KEY(21), ROTARY(22), FOCUSED(23),
/// DEFOCUSED(24), LEAVE…). Used to back `View.OnFocusChangeListener`.
pub const LV_EVENT_FOCUSED: lv_event_code_t = 23;
pub const LV_EVENT_DEFOCUSED: lv_event_code_t = 24;
pub const LV_EVENT_VALUE_CHANGED: lv_event_code_t = 39;
/// Fired when a multi-step interaction "completes" — used by lv_keyboard
/// when the user taps the OK / Enter key. Position 42 in the v9.6.0
/// enum (VALUE_CHANGED + 3, accounting for INSERT, REFRESH).
pub const LV_EVENT_READY: lv_event_code_t = 42;
/// Fired while an object is being deleted (also for each descendant during a
/// recursive `lv_obj_delete` / `lv_obj_clean` / screen switch). Position 48 in
/// the v9.6.0 enum: READY(42), CANCEL, STATE_CHANGED, CHECKED, UNCHECKED,
/// CREATE, DELETE. Used by the sim handle table to invalidate a deleted
/// object's slot.
pub const LV_EVENT_DELETE: lv_event_code_t = 48;
/// Sent once an object's size has changed (`lv_obj_refr_size`). Position 56
/// in the v9.6.0 enum: DELETE(48), CHILD_CHANGED, CHILD_CREATED,
/// CHILD_DELETED, SCREEN_UNLOAD_START, SCREEN_LOAD_START, SCREEN_LOADED,
/// SCREEN_UNLOADED, SIZE_CHANGED. What sizes a small clickable view's
/// extended touch area once its layout is known.
pub const LV_EVENT_SIZE_CHANGED: lv_event_code_t = 56;
/// Sent by `lv_obj_scroll_by_raw` once the children have moved and just
/// before the scroller invalidates its whole area. Position 15:
/// SCROLL_BEGIN(12), SCROLL_THROW_BEGIN, SCROLL_END, SCROLL. What the
/// hardware-scroll path (`graphics/lvgl/hw_scroll.rs`) hooks.
pub const LV_EVENT_SCROLL: lv_event_code_t = 15;
/// Display events. `LV_EVENT_INVALIDATE_AREA` (60) carries the `lv_area_t`
/// about to join the display's redraw list as its parameter, and a handler
/// may rewrite it; `REFR_START` (64) opens every refresh cycle, even an
/// empty one; `RENDER_START` (66) precedes the first band of one that draws.
pub const LV_EVENT_INVALIDATE_AREA: lv_event_code_t = 60;
pub const LV_EVENT_REFR_START: lv_event_code_t = 64;
pub const LV_EVENT_RENDER_START: lv_event_code_t = 66;

pub type lv_flex_flow_t = u32;
pub const LV_FLEX_FLOW_ROW: lv_flex_flow_t = 0x00;
pub const LV_FLEX_FLOW_COLUMN: lv_flex_flow_t = 0x01;

pub type lv_flex_align_t = u32;
pub const LV_FLEX_ALIGN_START: lv_flex_align_t = 0;
pub const LV_FLEX_ALIGN_END: lv_flex_align_t = 1;
pub const LV_FLEX_ALIGN_CENTER: lv_flex_align_t = 2;

/// The layout an object's `LV_STYLE_LAYOUT` names (`lv_layout_t`,
/// lv_layout.h): none, LVGL's flex, LVGL's grid, then the ids
/// [`lv_layout_create`] hands out in order. Guarded by
/// `lv_layout_constants_match_vendored_header`.
pub type lv_layout_t = u32;
pub const LV_LAYOUT_NONE: lv_layout_t = 0;
pub const LV_LAYOUT_FLEX: lv_layout_t = 1;
pub const LV_LAYOUT_GRID: lv_layout_t = 2;

/// A custom layout's callbacks (`lv_layout_callbacks_t`). LVGL calls
/// `layout_update_cb` from `lv_obj_update_layout` for a container on that
/// layout whose layout is dirty, with the container; `get_min_size_cb` when
/// a content-sized container asks how big its content is on one axis
/// (`width` says which), and the answer goes in `req_size`; `false` leaves
/// LVGL to walk the children's coordinates instead.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct lv_layout_callbacks_t {
    pub layout_update_cb: Option<unsafe extern "C" fn(*mut lv_obj_t, *mut c_void)>,
    pub get_min_size_cb:
        Option<unsafe extern "C" fn(*mut lv_obj_t, *mut i32, bool, *mut c_void) -> bool>,
}

/// Grid cell alignment (lv_grid.h `lv_grid_align_t`). STRETCH sizes the item
/// to its cell, which is what Android's `GridLayout.FILL` means. Guarded by
/// `lv_grid_constants_match_vendored_header`.
pub type lv_grid_align_t = u32;
pub const LV_GRID_ALIGN_START: lv_grid_align_t = 0;
pub const LV_GRID_ALIGN_CENTER: lv_grid_align_t = 1;
pub const LV_GRID_ALIGN_END: lv_grid_align_t = 2;
pub const LV_GRID_ALIGN_STRETCH: lv_grid_align_t = 3;

/// A grid track sized to its content: `LV_GRID_CONTENT = LV_COORD_MAX - 101`
/// (lv_grid.h). Guarded with the FR macro below.
pub const LV_GRID_CONTENT: i32 = LV_COORD_MAX - 101;
/// Terminates a grid template array: `LV_GRID_TEMPLATE_LAST = LV_COORD_MAX`.
pub const LV_GRID_TEMPLATE_LAST: i32 = LV_COORD_MAX;
/// A track taking `x` shares of the free space: `LV_GRID_FR(x) =
/// LV_COORD_MAX - 100 + x`. `x` must be `1..=99`: `LV_GRID_FR(100)` is
/// `LV_GRID_TEMPLATE_LAST` and would end the template early.
pub const fn lv_grid_fr(x: i32) -> i32 {
    LV_COORD_MAX - 100 + x
}

/// Linear-gradient direction. Values verified against
/// third_party/lvgl/include/lvgl/core/lv_style.h. v1 GradientDrawable only exposes
/// NONE / VER / HOR — LINEAR (arbitrary angle) and RADIAL are deferred.
pub type lv_grad_dir_t = u32;
pub const LV_GRAD_DIR_NONE: lv_grad_dir_t = 0;
pub const LV_GRAD_DIR_VER: lv_grad_dir_t = 1;
pub const LV_GRAD_DIR_HOR: lv_grad_dir_t = 2;

/// Direction bitmask used by `lv_indev_get_gesture_dir`. Verified against
/// third_party/lvgl/include/lvgl/core/lv_area.h:78-86. Only one of LEFT/RIGHT/TOP/BOTTOM
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
/// third_party/lvgl/include/lvgl/core/lv_obj_scroll.h:31-36. Plain C enum → 1 byte under
/// `-fshort-enums` (see build_support/lvgl.rs), same as `lv_dir_t` above.
/// Only OFF is exposed for now: EditText hides scrollbars to match Android.
pub type lv_scrollbar_mode_t = u8;
pub const LV_SCROLLBAR_MODE_OFF: lv_scrollbar_mode_t = 0;

/// Label long-text mode (third_party/lvgl/include/lvgl/widgets/lv_label.h
/// `lv_label_long_mode_t`), implicit ordinals; plain C enum → 1 byte under
/// `-fshort-enums`. Guarded by `lv_label_long_mode_constants_match_vendored_header`.
/// `TextView.setSingleLine`/`setEllipsize`/`setMaxLines` map onto these.
pub type lv_label_long_mode_t = u8;
pub const LV_LABEL_LONG_MODE_WRAP: lv_label_long_mode_t = 0;
pub const LV_LABEL_LONG_MODE_DOTS: lv_label_long_mode_t = 1;
pub const LV_LABEL_LONG_MODE_SCROLL: lv_label_long_mode_t = 2;
pub const LV_LABEL_LONG_MODE_SCROLL_CIRCULAR: lv_label_long_mode_t = 3;
pub const LV_LABEL_LONG_MODE_CLIP: lv_label_long_mode_t = 4;

/// Text alignment inside a label's box (third_party/lvgl/include/lvgl/font/lv_text.h
/// `lv_text_align_t`), implicit ordinals; plain C enum → 1 byte under `-fshort-enums`.
/// Guarded by `lv_text_align_constants_match_vendored_header`. `TextView.setGravity`'s
/// horizontal field maps onto these; AUTO is what a fresh label starts with and draws as LEFT.
pub type lv_text_align_t = u8;
pub const LV_TEXT_ALIGN_AUTO: lv_text_align_t = 0;
pub const LV_TEXT_ALIGN_LEFT: lv_text_align_t = 1;
pub const LV_TEXT_ALIGN_CENTER: lv_text_align_t = 2;
pub const LV_TEXT_ALIGN_RIGHT: lv_text_align_t = 3;

/// Arc fill direction (third_party/lvgl/include/lvgl/widgets/lv_arc.h
/// `lv_arc_mode_t`), implicit ordinals; plain C enum → 1 byte under
/// `-fshort-enums`. Guarded by `lv_arc_mode_constants_match_vendored_header`.
/// `CircularProgressIndicator.setIndicatorDirection` maps CLOCKWISE onto NORMAL
/// and COUNTERCLOCKWISE onto REVERSE; SYMMETRICAL (1) has no Android meaning.
pub type lv_arc_mode_t = u8;
pub const LV_ARC_MODE_NORMAL: lv_arc_mode_t = 0;
pub const LV_ARC_MODE_REVERSE: lv_arc_mode_t = 2;

/// `LV_COORD_MAX = (1 << LV_COORD_TYPE_SHIFT) - 1` (lv_area.h): the default of the
/// `max_height` style, so setting it lifts a cap. Guarded by
/// `lv_coord_max_matches_vendored_header`.
pub const LV_COORD_MAX: i32 = (1 << 29) - 1;
/// `LV_SIZE_CONTENT = LV_COORD_SET_SPEC(LV_COORD_MAX)` (lv_area.h): a size
/// that follows the object's children. Guarded with `LV_COORD_MAX`.
pub const LV_SIZE_CONTENT: i32 = LV_COORD_MAX | (1 << 29);

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
/// third_party/lvgl/include/lvgl/widgets/lv_keyboard.h. Only the four
/// "primary" modes are exposed; the various "user-defined" modes are
/// deferred until apps need custom layouts.
pub type lv_keyboard_mode_t = u32;
pub const LV_KEYBOARD_MODE_TEXT_LOWER: lv_keyboard_mode_t = 0;
// 1 = TEXT_UPPER and 2 = SPECIAL exist in LVGL but no caller selects them.
pub const LV_KEYBOARD_MODE_NUMBER: lv_keyboard_mode_t = 3;

pub type lv_style_selector_t = u32;

/// Part selectors for style setters. Verified against
/// `third_party/lvgl/include/lvgl/core/lv_obj_style.h:88`. The selector u32 packs both
/// the part (high bits) and the state (low bits); passing `0` selects
/// `LV_PART_MAIN` in any state — matches LVGL's `lv_style_selector_default`.
pub const LV_PART_MAIN: lv_style_selector_t = 0x000000;
pub const LV_PART_SCROLLBAR: lv_style_selector_t = 0x010000;
pub const LV_PART_INDICATOR: lv_style_selector_t = 0x020000;
/// The draggable handle of `lv_arc`/`lv_slider`; `CircularProgressIndicator`
/// removes the theme's style for it to draw a plain ring.
pub const LV_PART_KNOB: lv_style_selector_t = 0x030000;
/// `LV_PART_ANY | LV_STATE_ANY`: the selector `lv_obj_remove_style(obj, NULL, …)` takes to
/// strip every style an object carries — what LVGL's inline `lv_obj_remove_style_all` passes.
/// Both halves are guarded by `lv_part_constants_match_vendored_header` and
/// `lv_state_constants_match_vendored_header`.
pub const LV_PART_ANY: lv_style_selector_t = 0x0F0000;
pub const LV_STATE_ANY: lv_style_selector_t = 0xFFFF;

/// A bare part id (no state bits), as `lv_obj_get_style_prop` takes it.
pub type lv_part_t = u32;
/// `lv_style_prop_t` is `uint8_t` in LVGL 9 (`lv_types.h`).
pub type lv_style_prop_t = u8;

/// Style property ids from `third_party/lvgl/include/lvgl/core/lv_style.h` — the
/// `lv_style_prop_t` enum mixes explicit `= N` anchors with implicit
/// ordinals, so these are pinned by `tests::lv_style_prop_constants_match_vendored_header`.
/// Only the props the animation engine reads back are declared.
pub const LV_STYLE_TRANSLATE_X: lv_style_prop_t = 12;
pub const LV_STYLE_TRANSLATE_Y: lv_style_prop_t = 13;
pub const LV_STYLE_OPA: lv_style_prop_t = 112;
pub const LV_STYLE_TRANSFORM_SCALE_X: lv_style_prop_t = 152;
pub const LV_STYLE_TRANSFORM_SCALE_Y: lv_style_prop_t = 153;
pub const LV_STYLE_TRANSFORM_ROTATION: lv_style_prop_t = 156;
// Read back by `TextView`'s line cap (graphics/lvgl/widgets/text_view.rs).
pub const LV_STYLE_MAX_HEIGHT: lv_style_prop_t = 11;
pub const LV_STYLE_PAD_TOP: lv_style_prop_t = 24;
pub const LV_STYLE_PAD_BOTTOM: lv_style_prop_t = 25;
pub const LV_STYLE_BORDER_WIDTH: lv_style_prop_t = 56;
pub const LV_STYLE_TEXT_FONT: lv_style_prop_t = 77;
pub const LV_STYLE_TEXT_LINE_SPACE: lv_style_prop_t = 103;
/// Read back by `LinearLayout.setGravity` to learn which axis the layout's flow
/// makes the main one; `lv_obj_set_flex_flow` writes it.
pub const LV_STYLE_FLEX_FLOW: lv_style_prop_t = 160;
/// Removed per part by `ProgressBar` when an instance tint is cleared.
pub const LV_STYLE_BG_OPA: lv_style_prop_t = 72;
pub const LV_STYLE_BG_COLOR: lv_style_prop_t = 73;
// Read back by picodroid-core's layout solvers (graphics/lvgl/layouts): a
// child's margins and the container's gaps, padding, orientation and
// gravity, all of which LVGL keeps as style properties.
pub const LV_STYLE_WIDTH: lv_style_prop_t = 1;
pub const LV_STYLE_HEIGHT: lv_style_prop_t = 2;
pub const LV_STYLE_PAD_LEFT: lv_style_prop_t = 26;
pub const LV_STYLE_PAD_RIGHT: lv_style_prop_t = 27;
pub const LV_STYLE_PAD_ROW: lv_style_prop_t = 29;
pub const LV_STYLE_PAD_COLUMN: lv_style_prop_t = 30;
pub const LV_STYLE_MARGIN_TOP: lv_style_prop_t = 32;
pub const LV_STYLE_MARGIN_BOTTOM: lv_style_prop_t = 33;
pub const LV_STYLE_MARGIN_LEFT: lv_style_prop_t = 34;
pub const LV_STYLE_MARGIN_RIGHT: lv_style_prop_t = 35;
pub const LV_STYLE_LAYOUT: lv_style_prop_t = 132;
pub const LV_STYLE_FLEX_MAIN_PLACE: lv_style_prop_t = 161;
pub const LV_STYLE_FLEX_CROSS_PLACE: lv_style_prop_t = 162;
pub const LV_STYLE_FLEX_GROW: lv_style_prop_t = 164;

/// What [`lv_obj_get_local_style_prop`] answers (`lv_style_res_t`, a short
/// enum). Guarded by `lv_style_res_constants_match_vendored_header`.
pub type lv_style_res_t = u8;
pub const LV_STYLE_RES_NOT_FOUND: lv_style_res_t = 0;
pub const LV_STYLE_RES_FOUND: lv_style_res_t = 1;

/// Behaviour flags of a style property, for [`lv_style_register_prop`]
/// (`LV_STYLE_PROP_FLAG_*`, lv_style.h). A write to a property carrying
/// `LAYOUT_UPDATE` re-lays the object out, `PARENT_LAYOUT_UPDATE` its parent.
pub const LV_STYLE_PROP_FLAG_LAYOUT_UPDATE: u8 = 1 << 2;
pub const LV_STYLE_PROP_FLAG_PARENT_LAYOUT_UPDATE: u8 = 1 << 3;
/// The id [`lv_style_register_prop`] returns when LVGL has no property id
/// left (`LV_STYLE_PROP_INV`).
pub const LV_STYLE_PROP_INV: lv_style_prop_t = 0;
/// `transform_scale_*` value meaning "unscaled" (`lv_style.h` `LV_SCALE_NONE`).
pub const LV_SCALE_NONE: i32 = 256;
/// `lv_draw_rect.h`: special radius value meaning "fully rounded" — the
/// standard recipe for a radio-style circular checkbox indicator.
pub const LV_RADIUS_CIRCLE: i32 = 0x7FFF;
/// `lvgl/pd_canvas.h`: the bytes one drawing view's display list may hold;
/// ops past it are dropped and counted (`pd_canvas_end`).
pub const PD_CANVAS_MAX_BYTES: usize = 2048;

// Button-matrix control flags (`lv_buttonmatrix.h`). Only the subset the
// AlertDialog choice lists need.
/// The button is disabled, as `LV_STATE_DISABLED` disables a widget: drawn
/// in that state, skipped by the keypad's walk, and sending no
/// `VALUE_CHANGED` for a tap or ENTER.
pub const LV_BUTTONMATRIX_CTRL_DISABLED: u16 = 0x0040;
/// Toggle `LV_STATE_CHECKED` on the button when clicked.
pub const LV_BUTTONMATRIX_CTRL_CHECKABLE: u16 = 0x0080;
/// The button is currently checked.
pub const LV_BUTTONMATRIX_CTRL_CHECKED: u16 = 0x0100;
/// Returned by `lv_buttonmatrix_get_selected_button` when no button is selected.
pub const LV_BUTTONMATRIX_BUTTON_NONE: u32 = 0xFFFF;

/// Inner-alignment mode for `lv_image`. Verified against
/// `third_party/lvgl/include/lvgl/widgets/lv_image.h:38-55`. The picodroid
/// `ImageView` exposes a subset that maps to Android's `ScaleType` enum;
/// `LV_IMAGE_ALIGN_CENTER` (the default after `setScaleType` is unset)
/// renders unscaled and centered.
pub type lv_image_align_t = u32;
pub const LV_IMAGE_ALIGN_CENTER: lv_image_align_t = 9;
pub const LV_IMAGE_ALIGN_STRETCH: lv_image_align_t = 11;
pub const LV_IMAGE_ALIGN_TILE: lv_image_align_t = 12;
pub const LV_IMAGE_ALIGN_CONTAIN: lv_image_align_t = 13;
/// 15, not 14: v9.6.0 inserted `LV_IMAGE_ALIGN_CONTAIN_DOWNSCALE`
/// between CONTAIN and COVER.
pub const LV_IMAGE_ALIGN_COVER: lv_image_align_t = 15;

/// LVGL color format (`lv_color_format_t`). Values from
/// `third_party/lvgl/include/lvgl/draw/lv_color.h`. Only the formats picodroid emits
/// from `papk-pack` are exposed here.
pub type lv_color_format_t = u8;
pub const LV_COLOR_FORMAT_RGB888: lv_color_format_t = 0x0F;
pub const LV_COLOR_FORMAT_ARGB8888: lv_color_format_t = 0x10;
pub const LV_COLOR_FORMAT_RGB565: lv_color_format_t = 0x12;

/// Magic byte at the top of `lv_image_header_t::magic` for a v9 image
/// descriptor. See `third_party/lvgl/include/lvgl/draw/lv_image_dsc.h`.
pub const LV_IMAGE_HEADER_MAGIC: u8 = 0x19;

/// Mirror of `lv_image_header_t` (`third_party/lvgl/include/lvgl/draw/lv_image_dsc.h:109-118`).
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

/// Mirror of `lv_image_dsc_t` (`third_party/lvgl/include/lvgl/draw/lv_image_dsc.h:143-149`).
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

// SAFETY: the raw pointers are the only thing keeping the auto traits off.
// A descriptor is written once, before LVGL is handed its address, and only
// read after; `data` points at pixels in `'static` flash or `.rodata`, which
// nothing writes. Sending or sharing one across tasks shares only reads.
unsafe impl Send for lv_image_dsc_t {}
// SAFETY: as for `Send`: every access after construction is a read.
unsafe impl Sync for lv_image_dsc_t {}

// Opacity constants
pub const LV_OPA_COVER: u8 = 255;

/// `lv_border_side_t` (lv_style.h): a C enum. Only ever passed by value in a
/// register, so the C side's enum width (int, or a byte under
/// `-fshort-enums`) does not matter to the call.
pub type lv_border_side_t = u32;
/// `LV_BORDER_SIDE_BOTTOM`: the separator line under a list row.
pub const LV_BORDER_SIDE_BOTTOM: lv_border_side_t = 0x01;

// Object flags (from lv_obj.h)
pub const LV_OBJ_FLAG_HIDDEN: u32 = 1 << 0;
pub const LV_OBJ_FLAG_CLICKABLE: u32 = 1 << 1;
/// Take the keypad focus when clicked. Cleared on the soft-nav control, which
/// must never sit in a focus ring.
pub const LV_OBJ_FLAG_CLICK_FOCUSABLE: u32 = 1 << 2;
pub const LV_OBJ_FLAG_CHECKABLE: u32 = 1 << 3;
pub const LV_OBJ_FLAG_SCROLLABLE: u32 = 1 << 4;
/// Neither scrolled with the parent nor laid out by it: the `[layout]`
/// check leaves such a child out, as LVGL's own scroll extent does.
/// Not positioned by the parent's layout.
pub const LV_OBJ_FLAG_IGNORE_LAYOUT: u32 = 1 << 17;
pub const LV_OBJ_FLAG_FLOATING: u32 = 1 << 18;
/// Scroll the object into view when it takes focus (a focused row in a
/// list taller than the screen, as Android's ScrollView does).
pub const LV_OBJ_FLAG_SCROLL_ON_FOCUS: u32 = 1 << 10;
/// Hand a gesture to the parent. LVGL sets it on every object that has a
/// parent, so a gesture reaches the screen unless the object that wants it
/// clears this — which is what registering a swipe listener does.
pub const LV_OBJ_FLAG_GESTURE_BUBBLE: u32 = 1 << 15;
/// LVGL's first free user flag. Marks a view that was clickable before
/// `View.INVISIBLE` took that away, so `VISIBLE` can give it back.
pub const LV_OBJ_FLAG_USER_1: u32 = 1 << 27;
/// LVGL's second free user flag. Marks a `NumberPicker`'s box, a plain
/// `lv_obj` the keypad's edit-mode filter could not otherwise tell from a
/// layout (`widgets::number_picker::is_number_picker`).
pub const LV_OBJ_FLAG_USER_2: u32 = 1 << 28;

// Object states (from lv_obj_style.h; renumbered in v9.5.0, unchanged in v9.6.0).
// The state bits were renumbered in v9.5.0 to leave room for LV_STATE_ALT
// (1 << 0) and reserved slots, so values differ from v9.2.2.
pub const LV_STATE_CHECKED: u32 = 1 << 2;
pub const LV_STATE_FOCUSED: u32 = 1 << 3;
// Set (in addition to LV_STATE_FOCUSED) when focus arrives via a keypad/encoder
// indev rather than a pointer. The default theme styles this state separately
// (blue), so widgets that override the focus highlight must cover it too.
pub const LV_STATE_FOCUS_KEY: u32 = 1 << 4;

/// `lv_align_t` (lv_area.h): `LV_ALIGN_TOP_LEFT`, the origin `lv_obj_set_pos`
/// offsets from once a widget's own alignment is undone.
pub const LV_ALIGN_TOP_LEFT: u8 = 1;
// The other eight in-parent alignments: where a FrameLayout child's gravity
// puts it (`widgets/gravity.rs::frame_align`).
pub const LV_ALIGN_TOP_MID: u8 = 2;
pub const LV_ALIGN_TOP_RIGHT: u8 = 3;
pub const LV_ALIGN_BOTTOM_LEFT: u8 = 4;
pub const LV_ALIGN_BOTTOM_MID: u8 = 5;
pub const LV_ALIGN_BOTTOM_RIGHT: u8 = 6;
pub const LV_ALIGN_LEFT_MID: u8 = 7;
pub const LV_ALIGN_RIGHT_MID: u8 = 8;
pub const LV_ALIGN_CENTER: u8 = 9;
// Set while a widget is being edited (keypad/encoder edit mode). The framework
// toggles it on NumberPicker during keypad edit mode for the theme-matching
// secondary outline; EDITED (1 << 5) outranks FOCUS_KEY in style specificity,
// so the edit outline wins while both states are set.
pub const LV_STATE_EDITED: u32 = 1 << 5;
// Set on a scroller while an indev or a fling is moving it. The default
// theme binds a scrollbar style to it; ScrollView removes that binding so
// a scroll changes no style (see widgets/scroll_view.rs).
pub const LV_STATE_SCROLLED: u32 = 1 << 8;
pub const LV_STATE_DISABLED: u32 = 1 << 9;

// ---------------------------------------------------------------------------
// Callback types
// ---------------------------------------------------------------------------

pub type lv_display_flush_cb_t =
    Option<unsafe extern "C" fn(disp: *mut lv_display_t, area: *const lv_area_t, px_map: *mut u8)>;

pub type lv_display_flush_wait_cb_t = Option<unsafe extern "C" fn(disp: *mut lv_display_t)>;

pub type lv_indev_read_cb_t =
    Option<unsafe extern "C" fn(indev: *mut lv_indev_t, data: *mut lv_indev_data_t)>;

pub type lv_event_cb_t = Option<unsafe extern "C" fn(e: *mut lv_event_t)>;

// ---------------------------------------------------------------------------
// Typed handles
// ---------------------------------------------------------------------------
//
// LVGL is built with `LV_USE_CHECK_ARG 0` (lv_conf.h): no public call checks
// its object argument, so a null or freed `lv_obj_t *` handed to any of them
// is a wild access in C. The types below carry that obligation in Rust's type
// system instead of at every call site: a function whose pointer arguments
// are all `Obj`/`Group`/`Indev`/`Display` (the rest scalars, enums or `Copy`
// structs by value or by reference) is declared `safe fn`, and the argument
// for why each such handle is valid is made once, here.

/// A live LVGL object: non-null, and not deleted.
///
/// # Where an `Obj` comes from, and why it is live
///
/// An `Obj` is only ever made by:
///
/// - a `lv_*_create` call (`Option<Obj>`, `None` when LVGL could not
///   allocate the object);
/// - a getter LVGL answers from its own tree — [`lv_screen_active`],
///   [`lv_layer_top`], [`lv_obj_get_parent`], [`lv_obj_get_child`],
///   [`lv_group_get_focused`] — each `Option<Obj>`;
/// - [`ObjEvent::target`] / [`ObjEvent::current_target`], inside an event
///   callback, where LVGL is dispatching an event to that object;
/// - picodroid-core's handle table (`handle_table::get`), which hands out an
///   object only while its generation-tagged slot is current; the slot is
///   invalidated by an `LV_EVENT_DELETE` hook that LVGL runs before it frees
///   the object;
/// - [`Obj::from_raw`], which is `unsafe`: its caller vouches for liveness
///   (the slot tables that keep raw addresses across natives clear them in
///   their own `LV_EVENT_DELETE` callbacks).
///
/// Each of those yields an object LVGL has not deleted at that moment. What
/// keeps it live afterwards is that, in this codebase, an object is deleted
/// only by the `unsafe` [`lv_obj_delete`] and [`lv_obj_clean`] (on the
/// object and its descendants): picodroid binds no `lv_obj_delete_async`,
/// loads no screen with auto-delete, and holds no `Obj` for a child a widget
/// creates and deletes internally. `lv_timer_handler` stays `unsafe` for the
/// same reason.
///
/// # The limit
///
/// `Obj` is `Copy`, and nothing ties a copy to the object's lifetime: a copy
/// taken before a delete names freed memory after it. That is why every
/// delete is `unsafe` and its caller must show that no `Obj` naming the
/// deleted subtree is used afterwards — in practice an `Obj` is a local of
/// one native call or one callback, and state that outlives a call stores
/// the address (`usize`) and goes back through `from_raw`. An `Obj` must not
/// be kept in a static across natives unless that object is never deleted
/// while it is stored, which the static's declaration must say.
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Obj(NonNull<lv_obj_t>);

impl Obj {
    /// An `Obj` for `ptr`, `None` for null.
    ///
    /// # Safety
    ///
    /// `ptr` must be null or point at an LVGL object that has not been
    /// deleted, and the result must not be used after that object is
    /// deleted (see the type's docs).
    #[inline(always)]
    pub unsafe fn from_raw(ptr: *mut lv_obj_t) -> Option<Obj> {
        NonNull::new(ptr).map(Obj)
    }

    /// The raw pointer, for a C call that takes it as data.
    #[inline(always)]
    pub fn as_ptr(self) -> *mut lv_obj_t {
        self.0.as_ptr()
    }

    /// The object's address: the key the widget layer's slot tables and
    /// listener maps store.
    #[inline(always)]
    pub fn addr(self) -> usize {
        self.0.as_ptr() as usize
    }
}

/// A live LVGL focus group. Created by [`lv_group_create`], deleted only by
/// the `unsafe` [`lv_group_delete`], whose caller must drop every copy.
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Group(NonNull<lv_group_t>);

impl Group {
    /// The raw pointer.
    #[inline(always)]
    pub fn as_ptr(self) -> *mut lv_group_t {
        self.0.as_ptr()
    }
}

/// An LVGL input device. Made only by [`lv_indev_create`] or answered by
/// LVGL ([`lv_indev_active`], [`ObjEvent::indev`]); picodroid never deletes
/// one (no `lv_indev_delete` is bound), so every `Indev` stays valid.
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Indev(NonNull<lv_indev_t>);

impl Indev {
    /// The raw pointer.
    #[inline(always)]
    pub fn as_ptr(self) -> *mut lv_indev_t {
        self.0.as_ptr()
    }
}

/// An LVGL display. Made only by [`lv_display_create`] or answered by
/// [`lv_display_get_default`]; picodroid never deletes one (no
/// `lv_display_delete` is bound), so every `Display` stays valid.
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Display(NonNull<lv_display_t>);

impl Display {
    /// The raw pointer.
    #[inline(always)]
    pub fn as_ptr(self) -> *mut lv_display_t {
        self.0.as_ptr()
    }
}

// ---------------------------------------------------------------------------
// Object events
// ---------------------------------------------------------------------------

/// The event an object's callback is running for, valid for that call.
///
/// Only [`lv_event_cb!`] makes one, from the pointer LVGL hands the callback;
/// the lifetime keeps it inside the callback's body.
pub struct ObjEvent<'a> {
    raw: NonNull<lv_event_t>,
    _call: PhantomData<&'a mut lv_event_t>,
}

impl ObjEvent<'_> {
    /// # Safety
    ///
    /// `raw` must be the event LVGL passed to a callback it is running from
    /// an object's event list (one registered with [`lv_obj_add_event_cb`]
    /// or [`add_event_cb`]), and the result must not outlive that call.
    #[inline(always)]
    pub unsafe fn from_raw(raw: *mut lv_event_t) -> Self {
        Self {
            // SAFETY: LVGL never runs a callback with a null event (the
            // caller's contract).
            raw: unsafe { NonNull::new_unchecked(raw) },
            _call: PhantomData,
        }
    }

    /// The raw event, for the accessors that stay `unsafe`
    /// ([`lv_event_get_param`]).
    #[inline(always)]
    pub fn as_ptr(&self) -> *mut lv_event_t {
        self.raw.as_ptr()
    }

    /// The object the event was sent to (for a bubbled event, the child it
    /// started at).
    #[cfg(not(test))]
    #[inline(always)]
    pub fn target(&self) -> Obj {
        // SAFETY: `raw` is the event LVGL is dispatching (`from_raw`). An
        // object event's target is the object `lv_obj_send_event` was called
        // on, never null, and LVGL is still holding it: even a
        // `LV_EVENT_DELETE` callback runs before the object is freed.
        unsafe {
            Obj(NonNull::new_unchecked(lv_event_get_target_obj(
                self.as_ptr(),
            )))
        }
    }

    /// The object whose callback this is: the target, or for a bubbled event
    /// the ancestor the event has climbed to.
    #[cfg(not(test))]
    #[inline(always)]
    pub fn current_target(&self) -> Obj {
        // SAFETY: as in `target`: LVGL sets the current target to the object
        // whose event list it is walking.
        unsafe {
            Obj(NonNull::new_unchecked(lv_event_get_current_target_obj(
                self.as_ptr(),
            )))
        }
    }

    /// The input device that caused the event, if one did.
    #[cfg(not(test))]
    #[inline(always)]
    pub fn indev(&self) -> Option<Indev> {
        // SAFETY: `raw` is the event LVGL is dispatching (`from_raw`).
        unsafe { lv_event_get_indev(self.as_ptr()) }
    }

    /// The `user_data` the running callback was registered with, as the
    /// address it was given as.
    #[cfg(not(test))]
    #[inline(always)]
    pub fn user_data(&self) -> usize {
        // SAFETY: `raw` is the event LVGL is dispatching (`from_raw`); the
        // value is returned as a number, never dereferenced here.
        unsafe { lv_event_get_user_data(self.as_ptr()) as usize }
    }
}

/// Define an LVGL object-event callback.
///
/// ```ignore
/// lv_event_cb! {
///     /// Doc comment.
///     fn value_changed_cb(e) {
///         QUEUE.push(e.target().addr());
///     }
/// }
/// ```
///
/// expands to `unsafe extern "C" fn value_changed_cb(raw: *mut lv_event_t)`
/// whose body sees `e: ObjEvent<'_>`. Register it with [`add_event_cb`] (or
/// [`lv_obj_add_event_cb`]) on an object — never on a display or an input
/// device, whose events have no object target; those callbacks are written
/// by hand against the raw event.
#[macro_export]
macro_rules! lv_event_cb {
    ($(#[$attr:meta])* $vis:vis fn $name:ident($e:ident) $body:block) => {
        $(#[$attr])*
        $vis unsafe extern "C" fn $name(raw: *mut $crate::lv_event_t) {
            // SAFETY: LVGL calls an object-event callback only from
            // `lv_obj_send_event`, with the event it is dispatching, valid
            // for the call; `ObjEvent`'s lifetime keeps it inside the body.
            // These callbacks are registered on objects: putting one on a
            // display's or an input device's list takes the `unsafe`
            // `lv_display_add_event_cb` / `lv_indev_add_event_cb`, whose
            // contract rules that out.
            #[allow(unused_variables)]
            let $e = unsafe { $crate::ObjEvent::from_raw(raw) };
            $body
        }
    };
}

/// A custom layout's update callback: `fn name(cont)` with `cont: Obj`, the
/// container LVGL is laying out. Registered through [`layout_create`].
#[macro_export]
macro_rules! lv_layout_cb {
    ($(#[$attr:meta])* $vis:vis fn $name:ident($cont:ident) $body:block) => {
        $(#[$attr])*
        $vis unsafe extern "C" fn $name(raw: *mut $crate::lv_obj_t, _user_data: *mut ::core::ffi::c_void) {
            // SAFETY: LVGL calls a layout's update callback from
            // `lv_layout_apply`, with the container it is laying out, which
            // is live for the call.
            let Some($cont) = (unsafe { $crate::Obj::from_raw(raw) }) else {
                return;
            };
            $body
        }
    };
}

/// A custom layout's minimum-size callback: `fn name(cont, width) ->
/// Option<i32>` with `cont: Obj` and `width: bool` (the axis asked for);
/// `Some(size)` answers LVGL, `None` lets it walk the children instead.
#[macro_export]
macro_rules! lv_min_size_cb {
    ($(#[$attr:meta])* $vis:vis fn $name:ident($cont:ident, $width:ident) -> Option<i32> $body:block) => {
        $(#[$attr])*
        $vis unsafe extern "C" fn $name(
            raw: *mut $crate::lv_obj_t,
            req_size: *mut i32,
            $width: bool,
            _user_data: *mut ::core::ffi::c_void,
        ) -> bool {
            // SAFETY: LVGL calls a layout's min-size callback from
            // `lv_layout_get_min_size`, with the container it is sizing,
            // which is live for the call.
            let Some($cont) = (unsafe { $crate::Obj::from_raw(raw) }) else {
                return false;
            };
            let answer: Option<i32> = $body;
            match answer {
                Some(size) => {
                    // SAFETY: `req_size` is the out-parameter LVGL passed
                    // for this call, valid to write once.
                    unsafe { *req_size = size };
                    true
                }
                None => false,
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Extern "C" function declarations
// ---------------------------------------------------------------------------
// Excluded under `cfg(test)` so the constants + drift-check tests below
// compile on the host without linking against LVGL.

/// `lv_mem_monitor_t` (include/lvgl/stdlib/lv_mem.h): six `size_t`s and two
/// percentages.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct lv_mem_monitor_t {
    pub total_size: usize,
    pub free_cnt: usize,
    pub free_size: usize,
    pub free_biggest_size: usize,
    pub used_cnt: usize,
    pub max_used: usize,
    pub used_pct: u8,
    pub frag_pct: u8,
}

// Every `lv_*_create` returns `Option<Obj>`. LVGL asserts on a failed
// allocation of the object itself (`LV_ASSERT_MALLOC`, a `while(1)` here), but
// `lv_obj_class_create_obj` returns NULL without asserting when growing the
// parent's child array fails (`lv_obj_add_child`, third_party/lvgl
// src/core/lv_obj.c), so the NULL is a real, if unlikely, return.
//
// Functions that create a display, an input device or a group, or that walk
// LVGL's heap, stay `unsafe`: called before `lv_init` they reach an allocator
// that does not exist yet.
//
// Widget classes: a function named after a widget class (`lv_label_*`,
// `lv_bar_*`, `lv_roller_*`, …) casts its object to that class's struct and
// reads or writes past the base `lv_obj_t` (`LV_USE_CHECK_OBJ_CLASSTYPE` is 0,
// so nothing checks). An `Obj` does not carry its class, so those functions
// stay `unsafe`: the caller shows that the object was made by that class's
// `lv_*_create` (or a subclass's). The `lv_obj_*`, style, flag, state and
// group functions read only the base object and are `safe`.
#[cfg(not(test))]
unsafe extern "C" {
    // Core
    /// # Safety
    /// Once per boot, before any other LVGL call.
    pub fn lv_init();
    // lvgl/lv_draw_buf_sram.c: a pool in PSRAM keeps its render targets in
    // SRAM (board.toml `lv_mem_in_psram`).
    /// # Safety
    /// After `lv_init`, before the display is created.
    #[cfg(lv_mem_in_psram)]
    pub fn picodroid_lv_draw_buf_use_sram();
    #[cfg(lv_mem_in_psram)]
    pub safe fn picodroid_lv_draw_buf_pool_fallbacks() -> u32;
    pub safe fn lv_tick_inc(tick_period: u32);
    // Walks the TLSF pool: a few hundred blocks, for the rare caller that
    // needs the pool's free level (lifecycle's reclaim policy).
    /// # Safety
    /// After `lv_init` (the pool does not exist before).
    pub fn lv_mem_monitor(mon_p: &mut lv_mem_monitor_t);
    /// Runs LVGL's timers: rendering, input reads and every callback they
    /// fire.
    ///
    /// # Safety
    /// After `lv_init`. Like any delete, a timer may free objects; no `Obj`
    /// held across this call may be used after it unless it is known to
    /// survive (see [`Obj`]).
    pub fn lv_timer_handler() -> u32;

    // Display
    /// # Safety
    /// After `lv_init`.
    pub fn lv_display_create(hor_res: i32, ver_res: i32) -> Option<Display>;
    pub safe fn lv_display_set_flush_cb(disp: Display, flush_cb: lv_display_flush_cb_t);
    /// With a wait callback set, LVGL never spins on `flushing`: it calls
    /// this wherever it needs the last flush finished and clears the flag
    /// itself afterwards, so `lv_display_flush_ready` is not called at all.
    pub safe fn lv_display_set_flush_wait_cb(disp: Display, wait_cb: lv_display_flush_wait_cb_t);
    /// # Safety
    /// `buf1` (and `buf2` unless null) must point at `buf_size` writable
    /// bytes, aligned to `LV_DRAW_BUF_ALIGN`, that outlive the display and
    /// that nothing else writes.
    pub fn lv_display_set_buffers(
        disp: Display,
        buf1: *mut c_void,
        buf2: *mut c_void,
        buf_size: u32,
        render_mode: lv_display_render_mode_t,
    );
    pub safe fn lv_display_get_default() -> Option<Display>;
    /// The display's logical size — what the screen and its layers are laid
    /// out in: the panel. (An app's compat window is an object on the
    /// screen, `graphics/lvgl/window.rs`, not a display resolution.)
    pub safe fn lv_display_get_horizontal_resolution(disp: Display) -> i32;
    pub safe fn lv_display_get_vertical_resolution(disp: Display) -> i32;
    /// The layer LVGL draws above every screen: where system overlays (toast,
    /// snackbar, dialog scrim, the soft keyboard) live, so a screen that pans
    /// its oversized content leaves them in place. `None` before a display
    /// exists.
    pub safe fn lv_layer_top() -> Option<Obj>;
    /// Display-level events (`LV_EVENT_INVALIDATE_AREA`, the refresh and
    /// render milestones). Same descriptor model as `lv_obj_add_event_cb`,
    /// but there is nothing to return: the display keeps the list.
    ///
    /// # Safety
    /// `event_cb` must treat its event as a display event — never an
    /// [`lv_event_cb!`] callback, whose [`ObjEvent`] assumes an object
    /// target — and must expect `user_data` as it is passed.
    pub fn lv_display_add_event_cb(
        disp: Display,
        event_cb: lv_event_cb_t,
        filter: lv_event_code_t,
        user_data: *mut c_void,
    );
    /// The event's parameter: for `LV_EVENT_INVALIDATE_AREA` the mutable
    /// `lv_area_t` LVGL is about to record.
    ///
    /// # Safety
    /// `e` must be the event of a running callback.
    pub fn lv_event_get_param(e: *mut lv_event_t) -> *mut c_void;

    // Scroll position and the scrollbar geometry LVGL would draw for it.
    // Positions are the distance scrolled (positive once content has moved
    // up or left), i.e. the negated internal offset.
    pub safe fn lv_obj_get_scroll_x(obj: Obj) -> i32;
    pub safe fn lv_obj_get_scroll_y(obj: Obj) -> i32;
    /// The thumb rectangles LVGL will draw, or `x2 < x1` when a bar is not
    /// shown.
    pub safe fn lv_obj_get_scrollbar_area(
        obj: Obj,
        hor_area: &mut lv_area_t,
        ver_area: &mut lv_area_t,
    );
    /// Mark part of `obj` for redraw. Returns `lv_result_t`, which nothing
    /// here reads.
    pub safe fn lv_obj_invalidate_area(obj: Obj, area: &lv_area_t);
    /// Detach the styles bound to exactly `selector` (a null `style` means
    /// all of them) — how a widget sheds a theme binding it does not want.
    ///
    /// # Safety
    /// `style` must be null or a live `lv_style_t`.
    pub fn lv_obj_remove_style(obj: Obj, style: *const c_void, selector: lv_style_selector_t);
    /// Drop one property of `obj`'s own (local) style for `selector`, so the
    /// theme's value shows again. Returns whether it was set.
    pub safe fn lv_obj_remove_local_style_prop(
        obj: Obj,
        prop: lv_style_prop_t,
        selector: lv_style_selector_t,
    ) -> bool;
    /// A null transition descriptor turns the theme's animated style changes
    /// off for that part.
    ///
    /// # Safety
    /// `value` must be null or an `lv_style_transition_dsc_t` that outlives
    /// the object.
    pub fn lv_obj_set_style_transition(
        obj: Obj,
        value: *const c_void,
        selector: lv_style_selector_t,
    );

    // lvgl/hw_vscroll.c: the two questions the panel-scroll path has to ask
    // through LVGL's private headers (board.toml-derived `hw_vscroll`).
    /// May `scroller`'s visible rows be moved by the panel? On yes, `region`
    /// is those rows (always full width) and the return value is how many of
    /// `overlays` were filled with static things drawn on top of them; a
    /// negative value names why not.
    ///
    /// # Safety
    /// `overlays` must point at `max_overlays` writable areas.
    #[cfg(hw_vscroll)]
    pub fn picodroid_hw_vscroll_region(
        scroller: Obj,
        region: &mut lv_area_t,
        overlays: *mut lv_area_t,
        max_overlays: i32,
    ) -> i32;
    /// The panel moved `region`'s content by `dy` rows: grow every area
    /// already queued for redraw to cover where its pixels went.
    #[cfg(hw_vscroll)]
    pub safe fn picodroid_hw_vscroll_shift_pending(disp: Display, region: &lv_area_t, dy: i32);
    /// The scrollbar thumb's corner radius when the thumb is one flat opaque
    /// colour — so that only its two ends need repainting after a shift —
    /// or -1 when it is translucent, bordered or otherwise not.
    #[cfg(hw_vscroll)]
    pub safe fn picodroid_hw_vscroll_thumb_flat_radius(obj: Obj) -> i32;

    // Input device
    /// # Safety
    /// After `lv_init`.
    pub fn lv_indev_create() -> Option<Indev>;
    pub safe fn lv_indev_set_type(indev: Indev, indev_type: lv_indev_type_t);
    pub safe fn lv_indev_set_read_cb(indev: Indev, read_cb: lv_indev_read_cb_t);
    /// Ignore the current press until it is released: the finger that woke
    /// the panel must not land as a click on what it finds there.
    pub safe fn lv_indev_wait_release(indev: Indev);
    pub safe fn lv_indev_set_scroll_limit(indev: Indev, scroll_limit: u8);
    /// The indev that triggered the currently-running event. Read inside an
    /// `LV_EVENT_GESTURE` callback to recover gesture parameters.
    pub safe fn lv_indev_active() -> Option<Indev>;
    /// Direction of the most recent gesture detected on `indev`. Returns
    /// one of `LV_DIR_LEFT/RIGHT/TOP/BOTTOM` (or NONE if no gesture).
    pub safe fn lv_indev_get_gesture_dir(indev: Indev) -> lv_dir_t;
    pub safe fn lv_indev_set_group(indev: Indev, group: Option<Group>);
    /// An input device's own event list. `LV_EVENT_PRESSED` and the other
    /// press and key events go here before they go to the pressed object,
    /// whatever that object's bubbling, with the object as the event's
    /// parameter (`lv_event_get_param`). Returns the descriptor, unused here.
    ///
    /// # Safety
    /// As for [`lv_display_add_event_cb`]: `event_cb` must treat its event
    /// as an input-device event, never an [`lv_event_cb!`] callback.
    pub fn lv_indev_add_event_cb(
        indev: Indev,
        event_cb: lv_event_cb_t,
        filter: lv_event_code_t,
        user_data: *mut c_void,
    ) -> *mut c_void;
    /// Remove every registration of `event_cb` with `user_data` from the
    /// device's list; returns how many went. Both are only compared.
    pub safe fn lv_indev_remove_event_cb_with_user_data(
        indev: Indev,
        event_cb: lv_event_cb_t,
        user_data: *mut c_void,
    ) -> u32;

    // Groups (keypad focus navigation)
    /// # Safety
    /// After `lv_init`.
    pub fn lv_group_create() -> Option<Group>;
    pub safe fn lv_group_add_obj(group: Group, obj: Obj);
    pub safe fn lv_group_set_default(group: Option<Group>);
    pub safe fn lv_group_get_default() -> Option<Group>;
    pub safe fn lv_group_get_focused(group: Group) -> Option<Obj>;
    /// Focus `obj` within its own group. No-op if `obj` belongs to no group,
    /// so it is safe to call on a non-focusable widget.
    pub safe fn lv_group_focus_obj(obj: Obj);
    /// Remove `obj` from whatever group it belongs to. No-op if ungrouped.
    pub safe fn lv_group_remove_obj(obj: Obj);
    /// The group `obj` belongs to, if any.
    pub safe fn lv_obj_get_group(obj: Obj) -> Option<Group>;
    /// Delete a focus group. Used to tear down a per-Activity group on pop.
    ///
    /// # Safety
    /// No copy of `group` may be used afterwards, and no input device or
    /// default may still point at it.
    pub fn lv_group_delete(group: Group);

    // Screen
    /// The active screen; `None` before a display exists.
    pub safe fn lv_screen_active() -> Option<Obj>;

    // Objects
    pub safe fn lv_obj_create(parent: Obj) -> Option<Obj>;
    /// Delete every child of `obj` (and their descendants).
    ///
    /// # Safety
    /// No `Obj` naming one of the deleted objects may be used afterwards.
    pub fn lv_obj_clean(obj: Obj);
    pub safe fn lv_obj_set_pos(obj: Obj, x: i32, y: i32);
    /// `lv_align_t`: the reference point `set_pos` / `set_x` / `set_y` offset
    /// from. Widgets whose constructor aligns them elsewhere (`lv_keyboard`
    /// is BOTTOM_MID) need it back at TOP_LEFT for absolute placement.
    pub safe fn lv_obj_set_align(obj: Obj, align: u8);
    /// `lv_obj_set_align` and `lv_obj_set_pos` in one: place `obj` against
    /// `align` in its parent, offset by (`x_ofs`, `y_ofs`).
    pub safe fn lv_obj_align(obj: Obj, align: u8, x_ofs: i32, y_ofs: i32);
    /// Set just the x coordinate (preserves y). Used by ViewPropertyAnimator
    /// to animate axes independently — `lv_obj_set_pos` would clobber the
    /// other axis if a y-anim were running concurrently.
    pub safe fn lv_obj_set_x(obj: Obj, x: i32);
    /// See [`lv_obj_set_x`].
    pub safe fn lv_obj_set_y(obj: Obj, y: i32);
    pub safe fn lv_obj_set_size(obj: Obj, w: i32, h: i32);
    /// Grow the area a press lands in by `size` pixels on every side without
    /// moving a pixel of layout: how a small clickable view reaches the
    /// board's minimum touch target.
    pub safe fn lv_obj_set_ext_click_area(obj: Obj, size: i32);
    // Laid-out geometry readback for the View.getWidth/getHeight/getLeft/
    // getTop getters. Reads are only valid after a layout pass —
    // `lv_obj_update_layout` forces one (cheap no-op when nothing is dirty).
    pub safe fn lv_obj_get_x(obj: Obj) -> i32;
    pub safe fn lv_obj_get_y(obj: Obj) -> i32;
    pub safe fn lv_obj_get_width(obj: Obj) -> i32;
    pub safe fn lv_obj_get_height(obj: Obj) -> i32;
    /// Fill `coords` with the object's screen-absolute bounding box (x1/y1 =
    /// top-left). Unlike `lv_obj_get_x` (parent-relative), this gives the
    /// absolute origin needed to convert a screen touch point to
    /// view-relative MotionEvent coordinates.
    pub safe fn lv_obj_get_coords(obj: Obj, coords: &mut lv_area_t);
    pub safe fn lv_obj_update_layout(obj: Obj);
    pub safe fn lv_obj_center(obj: Obj);
    pub safe fn lv_obj_set_style_bg_color(
        obj: Obj,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    /// # Safety
    /// `event_cb` must be fine to run for any event LVGL sends `obj` with
    /// `user_data` as given: either it ignores `user_data`, or it reads it
    /// as the value passed here. [`add_event_cb`] is the safe form, with a
    /// null `user_data`.
    pub fn lv_obj_add_event_cb(
        obj: Obj,
        event_cb: lv_event_cb_t,
        filter: lv_event_code_t,
        user_data: *mut c_void,
    ) -> *mut lv_event_dsc_t;
    /// Remove every callback descriptor on `obj` whose function matches
    /// `event_cb`. Returns the count removed. Used by the soft keyboard's
    /// press-outside dismiss to detach its screen-level press hook on hide.
    pub safe fn lv_obj_remove_event_cb(obj: Obj, event_cb: lv_event_cb_t) -> u32;
    /// Walk one step up the widget tree. `None` when `obj` is a screen. Used
    /// by the keyboard outside-press hook to decide if a tap landed on the
    /// keyboard or one of its keys.
    pub safe fn lv_obj_get_parent(obj: Obj) -> Option<Obj>;
    /// `obj`'s position among its parent's children, or `-1` for a screen.
    /// A clicked ListView row reports its item position with it.
    pub safe fn lv_obj_get_index(obj: Obj) -> i32;
    /// Reorder `obj` among its siblings; `-1` is the last (topmost) slot.
    /// Refuses an out-of-range index and a parentless object.
    pub safe fn lv_obj_move_to_index(obj: Obj, index: i32);
    /// The widget class `obj` was created as; compared against the class
    /// statics below to tell a slider from a roller when the keypad asks
    /// what the focused widget is (events/keypad.rs).
    pub safe fn lv_obj_get_class(obj: Obj) -> *const lv_obj_class_t;
    pub static lv_label_class: lv_obj_class_t;
    pub static lv_textarea_class: lv_obj_class_t;
    pub static lv_bar_class: lv_obj_class_t;
    pub static lv_spinner_class: lv_obj_class_t;
    pub static lv_slider_class: lv_obj_class_t;
    pub static lv_checkbox_class: lv_obj_class_t;
    pub static lv_roller_class: lv_obj_class_t;
    pub static lv_dropdown_class: lv_obj_class_t;
    pub static lv_buttonmatrix_class: lv_obj_class_t;
    /// Scroll by `(dx, dy)`, clamped to the content: the keypad's page step
    /// on a `ScrollView` with nothing focusable inside.
    pub safe fn lv_obj_scroll_by_bounded(obj: Obj, dx: i32, dy: i32, anim_en: lv_anim_enable_t);
    /// Scroll so that `(x, y)` of the content sits at the object's origin:
    /// `(0, 0)` puts the screen back at its top-left after it panned.
    pub safe fn lv_obj_scroll_to(obj: Obj, x: i32, y: i32, anim_en: lv_anim_enable_t);
    pub safe fn lv_group_get_obj_count(group: Group) -> u32;
    /// How far the content reaches past the right / bottom edge, i.e. how
    /// far the object can still scroll that way: the `[layout]` fit check
    /// reads them off the screen after a `setContentView`.
    pub safe fn lv_obj_get_scroll_right(obj: Obj) -> i32;
    pub safe fn lv_obj_get_scroll_bottom(obj: Obj) -> i32;

    // Restrict the axes a scrollable object will scroll/over-pull on.
    // Default is LV_DIR_ALL; set to LV_DIR_VER on ScrollView so horizontal
    // drags don't trigger elastic over-pull or a transient scrollbar.
    pub safe fn lv_obj_set_scroll_dir(obj: Obj, dir: lv_dir_t);

    // Control scrollbar drawing independently of scrollability. OFF on
    // EditText: the object stays scrollable (horizontal scroll-to-cursor
    // for long one-line text) but never draws a scrollbar, like Android.
    pub safe fn lv_obj_set_scrollbar_mode(obj: Obj, mode: lv_scrollbar_mode_t);

    // Flex layout
    pub safe fn lv_obj_set_flex_flow(obj: Obj, flow: lv_flex_flow_t);
    pub safe fn lv_obj_set_flex_align(
        obj: Obj,
        main_place: lv_flex_align_t,
        cross_place: lv_flex_align_t,
        track_cross_place: lv_flex_align_t,
    );

    // Grid layout (LV_USE_GRID). A cell set on a child marks the parent's
    // layout dirty; a template set on the parent switches it to the grid
    // layout and re-lays it out, even when the same arrays are passed again.
    pub safe fn lv_obj_set_grid_cell(
        obj: Obj,
        column_align: lv_grid_align_t,
        col_pos: i32,
        col_span: i32,
        row_align: lv_grid_align_t,
        row_pos: i32,
        row_span: i32,
    );
    /// # Safety
    /// LVGL keeps the two pointers (it does not copy the arrays): each must
    /// point at an `i32` array that ends in `LV_GRID_TEMPLATE_LAST` and
    /// outlives `obj`, or is replaced by another such array before the
    /// memory is reused.
    pub fn lv_obj_set_grid_dsc_array(obj: Obj, col_dsc: *const i32, row_dsc: *const i32);

    // Custom layouts (lv_layout.h): picodroid-core's RelativeLayout and
    // LinearLayout are layouts LVGL calls back into, as it calls its flex.
    /// # Safety
    /// The callbacks run whenever LVGL lays out an object put on this layout
    /// (`lv_obj_set_layout`), with that object; `user_data` is handed back
    /// to them untouched, so it must stay valid for as long as the layout is
    /// used, which is the process's life (layouts are never removed).
    pub fn lv_layout_create(callbacks: lv_layout_callbacks_t, user_data: *mut c_void) -> u32;
    /// Put `obj` on a layout: `LV_LAYOUT_NONE`, LVGL's own, or an id from
    /// `lv_layout_create`.
    pub safe fn lv_obj_set_layout(obj: Obj, layout: u32);
    /// Place `obj` at (`x`, `y`) in its parent's content area, with no style
    /// written: what a layout callback does for each child.
    pub safe fn lv_obj_move_to(obj: Obj, x: i32, y: i32);
    /// Recompute `obj`'s size from its style; `true` when it changed. A
    /// layout callback calls it on a content-sized container once the
    /// children are placed, as LVGL's flex does.
    pub safe fn lv_obj_refr_size(obj: Obj) -> bool;
    pub safe fn lv_obj_get_content_width(obj: Obj) -> i32;
    pub safe fn lv_obj_get_content_height(obj: Obj) -> i32;
    pub safe fn lv_obj_mark_layout_as_dirty(obj: Obj);

    // Custom style properties (lv_style.h): a property id of this crate's
    // own, carried per object as a local style value.
    /// A new property id with `flag` (`LV_STYLE_PROP_FLAG_*`), or
    /// `LV_STYLE_PROP_INV` when the ids are used up.
    pub safe fn lv_style_register_prop(flag: u8) -> lv_style_prop_t;
    /// # Safety
    /// `value` is read as the property's type: a number for a numeric or
    /// custom property, a pointer that must outlive the object for a
    /// pointer-valued one. `set_style_num` is the numeric form.
    pub fn lv_obj_set_local_style_prop(
        obj: Obj,
        prop: lv_style_prop_t,
        value: lv_style_value_t,
        selector: lv_style_selector_t,
    );
    /// # Safety
    /// `value` must point at writable storage for one `lv_style_value_t`;
    /// it is written only when the answer is `LV_STYLE_RES_FOUND`.
    pub fn lv_obj_get_local_style_prop(
        obj: Obj,
        prop: lv_style_prop_t,
        value: *mut lv_style_value_t,
        selector: lv_style_selector_t,
    ) -> lv_style_res_t;

    // Label widget
    pub safe fn lv_label_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_label`, and `text` a NUL-terminated string (LVGL
    /// copies it).
    pub fn lv_label_set_text(obj: Obj, text: *const c_char);
    /// # Safety
    /// `obj` must be an `lv_label` (see "Widget classes" above).
    pub fn lv_label_set_long_mode(obj: Obj, long_mode: lv_label_long_mode_t);
    /// # Safety
    /// `obj` must be an `lv_label` (see "Widget classes" above).
    pub fn lv_label_get_long_mode(obj: Obj) -> lv_label_long_mode_t;
    /// Line height of a font from `lv_obj_get_style_prop(.., LV_STYLE_TEXT_FONT).ptr`.
    ///
    /// # Safety
    /// `font` must be a live `lv_font_t`.
    pub fn lv_font_get_line_height(font: *const lv_font_t) -> i32;
    /// The faces this build compiled (`lvgl/pd_fonts.c`), ascending by size; `count` receives
    /// the row count. Never empty: the default face is always the first row.
    pub safe fn pd_font_table(count: &mut usize) -> *const pd_font_t;
    /// Blank rows between a face's line top and its digit tops (`lvgl/pd_fonts.c`); 0 for null.
    ///
    /// # Safety
    /// `font` must be null or a live `lv_font_t`.
    pub fn pd_font_top_leading(font: *const lv_font_t) -> i32;

    // The retained display list behind `picodroid.graphics.Canvas` (`lvgl/pd_canvas.c`).
    // View-local coordinates, inclusive right/bottom; colours 0xAARRGGBB. Each op returns 0,
    // or -1 when it was dropped (no list on the object, or the list is full). An object
    // without a list (its `user_data` null, as LVGL leaves it) ignores every op.
    /// Give an object a display list and its draw and delete hooks; once per object.
    pub safe fn pd_canvas_attach(obj: Obj) -> i32;
    /// Empty the list before a new `onDraw`.
    pub safe fn pd_canvas_begin(obj: Obj);
    /// Invalidate the object; returns the ops this pass dropped for want of room.
    pub safe fn pd_canvas_end(obj: Obj) -> i32;
    pub safe fn pd_canvas_fill(obj: Obj, argb: u32) -> i32;
    pub safe fn pd_canvas_rect(
        obj: Obj,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        radius: i32,
        fill_argb: u32,
        stroke_argb: u32,
        stroke_width: i32,
    ) -> i32;
    pub safe fn pd_canvas_line(
        obj: Obj,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        width: i32,
        argb: u32,
        round_caps: i32,
    ) -> i32;
    pub safe fn pd_canvas_arc(
        obj: Obj,
        cx: i32,
        cy: i32,
        radius: i32,
        start_deg: i32,
        end_deg: i32,
        width: i32,
        argb: u32,
        rounded: i32,
    ) -> i32;
    /// `x` anchors per `align` (`LV_TEXT_ALIGN_*`), `baseline` is Android's `y`,
    /// `font_index` a row of `pd_font_table`; the text is copied, so it need not be
    /// NUL-terminated.
    ///
    /// # Safety
    /// `utf8` must point at `len` readable bytes.
    pub fn pd_canvas_text(
        obj: Obj,
        x: i32,
        baseline: i32,
        font_index: i32,
        argb: u32,
        align: i32,
        utf8: *const c_char,
        len: usize,
    ) -> i32;
    /// `Paint.ascent()`: the line's top above the baseline, negative; 0 for no face.
    pub safe fn pd_canvas_ascent(font_index: i32) -> i32;
    /// `Paint.descent()`: the line's bottom below the baseline; 0 for no face.
    pub safe fn pd_canvas_descent(font_index: i32) -> i32;
    /// `Paint.measureText()`: one line's width in the face; the text is copied.
    ///
    /// # Safety
    /// `utf8` must point at `len` readable bytes.
    pub fn pd_canvas_text_width(font_index: i32, utf8: *const c_char, len: usize) -> i32;

    // Button widget
    pub safe fn lv_button_create(parent: Obj) -> Option<Obj>;

    // Bar widget
    pub safe fn lv_bar_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_bar` (see "Widget classes" above).
    pub fn lv_bar_set_value(obj: Obj, value: i32, anim: lv_anim_enable_t);
    /// # Safety
    /// `obj` must be an `lv_bar` (see "Widget classes" above).
    pub fn lv_bar_set_range(obj: Obj, min: i32, max: i32);

    // Spinner widget — indeterminate counterpart of lv_bar; animates a
    // rotating arc whose duration and sweep are configurable. Used by the
    // indeterminate variant of `picodroid.widget.ProgressBar`.
    pub safe fn lv_spinner_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_spinner` (see "Widget classes" above).
    pub fn lv_spinner_set_anim_params(obj: Obj, t_ms: u32, angle_deg: u32);

    // Arc widget — the determinate ring behind `CircularProgressIndicator`.
    // Angles are `lv_value_precise_t`, which is `int32_t` because lv_conf.h
    // sets `LV_USE_FLOAT 0` (guarded by `lv_use_float_is_off`): flipping it
    // would silently change these signatures to `float`.
    pub safe fn lv_arc_create(parent: Obj) -> Option<Obj>;
    /// The track's span, `start`..`end` clockwise from 3 o'clock; LVGL wraps
    /// either past 360 (so a full circle is exactly `0, 360`).
    /// # Safety
    /// `obj` must be an `lv_arc` (see "Widget classes" above).
    pub fn lv_arc_set_bg_angles(obj: Obj, start: i32, end: i32);
    /// Added to every angle at draw time; normalised into [0, 360).
    /// # Safety
    /// `obj` must be an `lv_arc` (see "Widget classes" above).
    pub fn lv_arc_set_rotation(obj: Obj, rotation: i32);
    /// # Safety
    /// `obj` must be an `lv_arc` (see "Widget classes" above).
    pub fn lv_arc_set_mode(obj: Obj, mode: lv_arc_mode_t);
    /// # Safety
    /// `obj` must be an `lv_arc` (see "Widget classes" above).
    pub fn lv_arc_set_value(obj: Obj, value: i32);
    /// # Safety
    /// `obj` must be an `lv_arc` (see "Widget classes" above).
    pub fn lv_arc_set_range(obj: Obj, min: i32, max: i32);

    // Switch widget
    pub safe fn lv_switch_create(parent: Obj) -> Option<Obj>;

    // Color
    pub safe fn lv_color_hex(c: u32) -> lv_color_t;

    // Events — the raw accessors behind `ObjEvent`.
    /// # Safety
    /// `e` must be the event of a running callback.
    pub fn lv_event_get_target_obj(e: *mut lv_event_t) -> *mut lv_obj_t;
    /// The `user_data` passed to `lv_obj_add_event_cb` for the descriptor whose
    /// callback is currently running. The handle table rides the slot id here so
    /// its delete callback can clear the right entry in O(1).
    ///
    /// # Safety
    /// `e` must be the event of a running callback.
    pub fn lv_event_get_user_data(e: *mut lv_event_t) -> *mut c_void;
    /// The widget the firing handler is bound to. Differs from
    /// `lv_event_get_target_obj` when an event bubbles up from a child
    /// (e.g. lv_calendar's inner btnmatrix VALUE_CHANGED bubbles to the
    /// calendar root via LV_OBJ_FLAG_EVENT_BUBBLE).
    ///
    /// # Safety
    /// `e` must be the event of a running callback.
    pub fn lv_event_get_current_target_obj(e: *mut lv_event_t) -> *mut lv_obj_t;

    /// Get the indev (touch / keypad) that originated `e`. Used by the
    /// touch-event trampolines to fetch the current pointer position.
    ///
    /// # Safety
    /// `e` must be the event of a running callback.
    pub fn lv_event_get_indev(e: *mut lv_event_t) -> Option<Indev>;

    /// Fill `point` with the indev's current pointer position. Result is
    /// in display pixel coordinates. For a keypad it reads the keypad's
    /// unused point field — a meaningless value, but a defined one; only the
    /// PRESSED/PRESSING/RELEASED touch callbacks ask.
    pub safe fn lv_indev_get_point(indev: Indev, point: &mut lv_point_t);
    /// Synchronously fire `event_code` on `obj`. Invokes every matching
    /// event callback on this object (so e.g. `LV_EVENT_CLICKED` on a
    /// Button goes through the same `button_click_cb` a real touch would
    /// trigger). Returns `lv_result_t` (0 = OK, 1 = invalid).
    ///
    /// # Safety
    /// `param` is what every callback reached reads as the event's
    /// parameter: null, or what those callbacks expect for `event_code`.
    /// [`send_event`] is the safe form, with a null parameter.
    pub fn lv_obj_send_event(obj: Obj, event_code: lv_event_code_t, param: *mut c_void) -> u32;

    // Object lifecycle
    /// # Safety
    /// No `Obj` naming `obj` or one of its descendants may be used
    /// afterwards (see [`Obj`]).
    pub fn lv_obj_delete(obj: Obj);

    // Mark the entire object as needing redraw on the next refresh.
    pub safe fn lv_obj_invalidate(obj: Obj);
    /// LVGL's automatic style refresh, a global flag: off, `lv_obj_set_style_*`
    /// records the value and does nothing else (see `graphics::lvgl::style_batch`).
    pub safe fn lv_obj_enable_style_refresh(en: bool);
    /// The refresh a `lv_obj_set_style_*` of `prop` on `part` would have run.
    pub safe fn lv_obj_refresh_style(obj: Obj, part: lv_part_t, prop: lv_style_prop_t);

    // Object parent / child
    /// Move `obj` under `parent`. LVGL does not look for a cycle.
    ///
    /// # Safety
    /// `parent` must be neither `obj` nor one of its descendants: the tree
    /// would loop, and deleting it would recurse without end.
    pub fn lv_obj_set_parent(obj: Obj, parent: Obj);
    /// The child at `idx` (negative counts from the end), `None` past the
    /// last.
    pub safe fn lv_obj_get_child(obj: Obj, idx: i32) -> Option<Obj>;
    pub safe fn lv_obj_get_child_count(obj: Obj) -> u32;

    // Flex layout — set_flex_flow / align live above; this declaration
    // backs `LinearLayout.LayoutParams.weight` so weighted children expand
    // along the flex axis. Maps directly to `lv_obj_set_flex_grow`.
    pub safe fn lv_obj_set_flex_grow(obj: Obj, grow: u8);

    // Object flags
    pub safe fn lv_obj_add_flag(obj: Obj, f: u32);
    pub safe fn lv_obj_remove_flag(obj: Obj, f: u32);

    // ListView rows (widgets/list_view.rs): stretch an adapter's row view to
    // the list's width and give it the list button's padding and separator.
    pub safe fn lv_obj_set_width(obj: Obj, w: i32);
    pub safe fn lv_obj_set_height(obj: Obj, h: i32);
    pub safe fn lv_obj_set_style_pad_all(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_border_side(
        obj: Obj,
        value: lv_border_side_t,
        selector: lv_style_selector_t,
    );

    // Object state
    pub safe fn lv_obj_has_state(obj: Obj, state: u32) -> bool;
    pub safe fn lv_obj_add_state(obj: Obj, state: u32);
    pub safe fn lv_obj_remove_state(obj: Obj, state: u32);

    // Text style
    pub safe fn lv_obj_set_style_text_color(
        obj: Obj,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    /// # Safety
    /// `value` must be a font that outlives the object: one of
    /// `pd_font_table`'s compiled faces.
    pub fn lv_obj_set_style_text_font(
        obj: Obj,
        value: *const lv_font_t,
        selector: lv_style_selector_t,
    );
    pub safe fn lv_obj_set_style_text_align(
        obj: Obj,
        value: lv_text_align_t,
        selector: lv_style_selector_t,
    );

    // Margin style: the space a flex layout keeps clear around a child.
    pub safe fn lv_obj_set_style_margin_left(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_margin_top(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_margin_right(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_margin_bottom(obj: Obj, value: i32, selector: lv_style_selector_t);

    // Padding style
    pub safe fn lv_obj_set_style_pad_left(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_pad_right(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_pad_top(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_max_height(obj: Obj, value: i32, selector: lv_style_selector_t);
    /// `View.setMinimumWidth/Height` and `TextView.setMaxWidth`: bounds on
    /// the laid-out size that a content-sized or weighted view respects.
    pub safe fn lv_obj_set_style_min_width(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_min_height(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_max_width(obj: Obj, value: i32, selector: lv_style_selector_t);
    /// Whether `f` is set on `obj`.
    pub safe fn lv_obj_has_flag(obj: Obj, f: u32) -> bool;
    pub safe fn lv_obj_set_style_pad_bottom(obj: Obj, value: i32, selector: lv_style_selector_t);
    // Outline style — drawn outside the object's border. Used by NumberPicker
    // to replicate the theme's outline_primary/outline_secondary focus/edit
    // feedback, which the default theme applies to group-default widgets but
    // not to plain lv_obj containers.
    pub safe fn lv_obj_set_style_outline_width(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_outline_pad(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_outline_opa(obj: Obj, value: u8, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_outline_color(
        obj: Obj,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );

    // Active theme's primary/secondary accent colors (lv_theme.h). `obj` picks
    // the display whose theme is consulted; pass the widget being styled.
    pub safe fn lv_theme_get_color_primary(obj: Obj) -> lv_color_t;
    pub safe fn lv_theme_get_color_secondary(obj: Obj) -> lv_color_t;

    pub safe fn lv_obj_set_style_pad_row(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_pad_column(obj: Obj, value: i32, selector: lv_style_selector_t);

    // Opacity style
    pub safe fn lv_obj_set_style_opa(obj: Obj, value: u8, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_bg_opa(obj: Obj, value: u8, selector: lv_style_selector_t);
    /// Duration `LV_ANIM_ON` setters (`lv_bar_set_value`, ...) animate over;
    /// the property default is 0 ms.
    pub safe fn lv_obj_set_style_anim_duration(obj: Obj, value: u32, selector: lv_style_selector_t);
    /// # Safety
    /// `src` must be null or an image source (`lv_image_dsc_t`, a symbol or
    /// path string) that outlives the object.
    pub fn lv_obj_set_style_bg_image_src(
        obj: Obj,
        src: *const c_void,
        selector: lv_style_selector_t,
    );

    // Style readback. The typed `lv_obj_get_style_<prop>()` getters are
    // `static inline` in LVGL 9, so this generic exported one is the only
    // linkable entry; unset props return the property's default.
    pub safe fn lv_obj_get_style_prop(
        obj: Obj,
        part: lv_part_t,
        prop: lv_style_prop_t,
    ) -> lv_style_value_t;

    // Transform styles — ViewPropertyAnimator and View.setTranslationX & co.
    // translate in px; rotation in 0.1°; scale with LV_SCALE_NONE (256) = 1.0;
    // pivot as a coordinate (`lv_pct(50)` = the object's centre).
    pub safe fn lv_obj_set_style_translate_x(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_translate_y(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_transform_rotation(
        obj: Obj,
        value: i32,
        selector: lv_style_selector_t,
    );
    pub safe fn lv_obj_set_style_transform_scale_x(
        obj: Obj,
        value: i32,
        selector: lv_style_selector_t,
    );
    pub safe fn lv_obj_set_style_transform_scale_y(
        obj: Obj,
        value: i32,
        selector: lv_style_selector_t,
    );
    pub safe fn lv_obj_set_style_transform_pivot_x(
        obj: Obj,
        value: i32,
        selector: lv_style_selector_t,
    );
    pub safe fn lv_obj_set_style_transform_pivot_y(
        obj: Obj,
        value: i32,
        selector: lv_style_selector_t,
    );
    /// `LV_PCT(x)` as the exported function — percentage coordinate encoding.
    pub safe fn lv_pct(x: i32) -> i32;

    // Drawable styles — used by GradientDrawable to apply a bundle of
    // visual properties at once.
    pub safe fn lv_obj_set_style_radius(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_border_width(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_border_color(
        obj: Obj,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    pub safe fn lv_obj_set_style_border_opa(obj: Obj, value: u8, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_bg_grad_color(
        obj: Obj,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    pub safe fn lv_obj_set_style_bg_grad_dir(
        obj: Obj,
        value: lv_grad_dir_t,
        selector: lv_style_selector_t,
    );

    // List widget
    pub safe fn lv_list_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `text` must be a NUL-terminated string (LVGL copies it).
    pub fn lv_list_add_text(list: Obj, text: *const c_char) -> Option<Obj>;
    /// # Safety
    /// `icon` must be null or an image source that outlives the row; `text`
    /// null or a NUL-terminated string (LVGL copies it).
    pub fn lv_list_add_button(list: Obj, icon: *const c_char, text: *const c_char) -> Option<Obj>;

    // Image widget
    pub safe fn lv_image_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_image`, and `src` an image source
    /// (`lv_image_dsc_t`, symbol or path string) that outlives it: LVGL keeps
    /// the pointer.
    pub fn lv_image_set_src(obj: Obj, src: *const c_void);
    /// Set the inner alignment of the image inside its widget area. Used by
    /// `ImageView.setScaleType` to map Android's FIT_CENTER / CENTER_CROP /
    /// FIT_XY / TILE onto LVGL's `lv_image_align_t` enum.
    /// # Safety
    /// `obj` must be an `lv_image` (see "Widget classes" above).
    pub fn lv_image_set_inner_align(obj: Obj, align: lv_image_align_t);
    /// Uniform image scale; `256` = 1.0×.
    /// # Safety
    /// `obj` must be an `lv_image` (see "Widget classes" above).
    pub fn lv_image_set_scale(obj: Obj, zoom: u32);

    /// Image recolor (tint) — applies a tint color to the rendered image.
    /// Recolor is a *style* property on the lv_image widget; pair with
    /// `lv_obj_set_style_image_recolor_opa(..., 0..255, ...)` to control
    /// blend strength (0 = no tint, 255 = fully recolored).
    pub safe fn lv_obj_set_style_image_recolor(
        obj: Obj,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    pub safe fn lv_obj_set_style_image_recolor_opa(
        obj: Obj,
        value: u8,
        selector: lv_style_selector_t,
    );

    // Arc style — used by `lv_arc` and its subclass `lv_spinner`. Pair with
    // a part selector (e.g. `LV_PART_INDICATOR` for the moving sweep,
    // `LV_PART_MAIN` for the background ring).
    pub safe fn lv_obj_set_style_arc_color(
        obj: Obj,
        value: lv_color_t,
        selector: lv_style_selector_t,
    );
    pub safe fn lv_obj_set_style_arc_width(obj: Obj, value: i32, selector: lv_style_selector_t);
    pub safe fn lv_obj_set_style_arc_rounded(obj: Obj, value: bool, selector: lv_style_selector_t);
    /// Opacity of one part's arc (`LV_OPA_COVER` = 255): how a Java colour's
    /// alpha byte reaches the ring, `lv_color_t` carrying none.
    pub safe fn lv_obj_set_style_arc_opa(obj: Obj, value: u8, selector: lv_style_selector_t);

    // Slider widget
    pub safe fn lv_slider_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_slider` (see "Widget classes" above).
    pub fn lv_slider_set_value(obj: Obj, value: i32, anim: lv_anim_enable_t);
    /// # Safety
    /// `obj` must be an `lv_slider` (see "Widget classes" above).
    pub fn lv_slider_set_range(obj: Obj, min: i32, max: i32);
    /// # Safety
    /// `obj` must be an `lv_slider` (see "Widget classes" above).
    pub fn lv_slider_get_value(obj: Obj) -> i32;

    // Checkbox widget
    pub safe fn lv_checkbox_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_checkbox`, and `txt` a NUL-terminated string (LVGL
    /// copies it).
    pub fn lv_checkbox_set_text(obj: Obj, txt: *const c_char);

    // Button-matrix widget — one focusable lv_obj rendering a grid of
    // buttons from a NUL-terminated `map` of `*const c_char` (with `"\n"`
    // entries as row breaks and a final null entry). Backs AlertDialog's
    // item / choice lists: a single widget regardless of row count dodges
    // the >12-focusable-lv_list-rows renderer hang. Selecting fires
    // LV_EVENT_VALUE_CHANGED; `get_selected_button` returns
    // `LV_BUTTONMATRIX_BUTTON_NONE` (0xFFFF) when nothing is selected. The
    // per-button setters and getters ignore a `btn_id` past the last button
    // (lv_buttonmatrix.c checks it against `btn_cnt`).
    pub safe fn lv_buttonmatrix_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_buttonmatrix`; `map` and every string it points
    /// at must outlive the widget (LVGL keeps the pointers) and follow the
    /// map format above.
    pub fn lv_buttonmatrix_set_map(obj: Obj, map: *const *const c_char);
    /// # Safety
    /// `obj` must be an `lv_buttonmatrix` (see "Widget classes" above).
    pub fn lv_buttonmatrix_set_button_ctrl(obj: Obj, btn_id: u32, ctrl: u16);
    /// # Safety
    /// `obj` must be an `lv_buttonmatrix` (see "Widget classes" above).
    pub fn lv_buttonmatrix_clear_button_ctrl(obj: Obj, btn_id: u32, ctrl: u16);
    /// # Safety
    /// `obj` must be an `lv_buttonmatrix` (see "Widget classes" above).
    pub fn lv_buttonmatrix_set_one_checked(obj: Obj, en: bool);
    /// # Safety
    /// `obj` must be an `lv_buttonmatrix` (see "Widget classes" above).
    pub fn lv_buttonmatrix_get_selected_button(obj: Obj) -> u32;
    /// The key LEFT/RIGHT walk from and ENTER presses; a keypad-focused
    /// keyboard starts on its first character key, not on a mode switch.
    /// # Safety
    /// `obj` must be an `lv_buttonmatrix` (see "Widget classes" above).
    pub fn lv_buttonmatrix_set_selected_button(obj: Obj, btn_id: u32);
    /// # Safety
    /// `obj` must be an `lv_buttonmatrix` (see "Widget classes" above).
    pub fn lv_buttonmatrix_has_button_ctrl(obj: Obj, btn_id: u32, ctrl: u16) -> bool;

    // Dropdown widget
    pub safe fn lv_dropdown_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_dropdown`, and `options` a NUL-terminated string (LVGL
    /// copies it).
    pub fn lv_dropdown_set_options(obj: Obj, options: *const c_char);
    /// # Safety
    /// `obj` must be an `lv_dropdown` (see "Widget classes" above).
    pub fn lv_dropdown_get_selected(obj: Obj) -> u32;
    /// Whether the option list is showing: while it is, the keypad's PREV/NEXT
    /// walk it (events/keypad.rs::widget_remap).
    /// # Safety
    /// `obj` must be an `lv_dropdown` (see "Widget classes" above).
    pub fn lv_dropdown_is_open(obj: Obj) -> bool;

    // Roller widget — vertically-scrolling option list. Used by TimePicker
    // (hour + minute) and could back a future date-roller variant.
    pub safe fn lv_roller_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_roller`, and `options` a NUL-terminated string (LVGL
    /// copies it).
    pub fn lv_roller_set_options(obj: Obj, options: *const c_char, mode: lv_roller_mode_t);
    /// # Safety
    /// `obj` must be an `lv_roller` (see "Widget classes" above).
    pub fn lv_roller_set_selected(obj: Obj, sel_opt: u32, anim: lv_anim_enable_t);
    /// # Safety
    /// `obj` must be an `lv_roller` (see "Widget classes" above).
    pub fn lv_roller_get_selected(obj: Obj) -> u32;
    /// # Safety
    /// `obj` must be an `lv_roller` (see "Widget classes" above).
    pub fn lv_roller_set_visible_row_count(obj: Obj, row_cnt: u32);

    // Calendar widget — backs DatePicker. `lv_calendar_get_pressed_date`
    // populates a caller-allocated `lv_calendar_date_t`.
    pub safe fn lv_calendar_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_calendar` (see "Widget classes" above).
    pub fn lv_calendar_set_today_date(obj: Obj, year: u32, month: u32, day: u32);
    /// # Safety
    /// `obj` must be an `lv_calendar` (see "Widget classes" above).
    pub fn lv_calendar_set_month_shown(obj: Obj, year: u32, month: u32);
    /// # Safety
    /// `obj` must be an `lv_calendar` (see "Widget classes" above).
    pub fn lv_calendar_get_pressed_date(obj: Obj, date: &mut lv_calendar_date_t) -> lv_result_t;

    // Textarea widget
    pub safe fn lv_textarea_create(parent: Obj) -> Option<Obj>;
    /// # Safety
    /// `obj` must be an `lv_textarea`, and `txt` a NUL-terminated string (LVGL
    /// copies it).
    pub fn lv_textarea_set_text(obj: Obj, txt: *const c_char);
    /// The text LVGL holds, NUL-terminated; valid until the next edit.
    /// # Safety
    /// `obj` must be an `lv_textarea` (see "Widget classes" above).
    pub fn lv_textarea_get_text(obj: Obj) -> *const c_char;
    /// As [`lv_textarea_get_text`], for a label.
    /// # Safety
    /// `obj` must be an `lv_label` (see "Widget classes" above).
    pub fn lv_label_get_text(obj: Obj) -> *const c_char;
    /// # Safety
    /// `obj` must be an `lv_textarea`, and `txt` a NUL-terminated string (LVGL
    /// copies it).
    pub fn lv_textarea_set_placeholder_text(obj: Obj, txt: *const c_char);
    /// # Safety
    /// `obj` must be an `lv_textarea` (see "Widget classes" above).
    pub fn lv_textarea_set_one_line(obj: Obj, en: bool);
    /// # Safety
    /// `obj` must be an `lv_textarea` (see "Widget classes" above).
    pub fn lv_textarea_set_password_mode(obj: Obj, en: bool);
    /// # Safety
    /// `obj` must be an `lv_textarea`, and `txt` a NUL-terminated string (LVGL
    /// copies it).
    pub fn lv_textarea_add_text(obj: Obj, txt: *const c_char);

    // Keyboard widget
    pub safe fn lv_keyboard_create(parent: Obj) -> Option<Obj>;
    /// Bind the keyboard to `ta`, or to nothing.
    ///
    /// # Safety
    /// `kb` must be an `lv_keyboard` and `ta` an `lv_textarea`.
    pub fn lv_keyboard_set_textarea(kb: Obj, ta: Option<Obj>);
    /// # Safety
    /// `obj` must be an `lv_keyboard` (see "Widget classes" above).
    pub fn lv_keyboard_set_mode(kb: Obj, mode: lv_keyboard_mode_t);
}

// ---------------------------------------------------------------------------
// Safe forms of the calls above that take strings or optional pointers
// ---------------------------------------------------------------------------

/// [`lv_obj_add_event_cb`] with a null `user_data`: what every registration
/// but the handle table's makes. `event_cb` is an `unsafe extern "C" fn` LVGL
/// calls with the event it is dispatching to `obj`, which is all an object
/// callback may assume.
#[cfg(not(test))]
#[inline(always)]
pub fn add_event_cb(
    obj: Obj,
    event_cb: unsafe extern "C" fn(*mut lv_event_t),
    filter: lv_event_code_t,
) {
    // SAFETY: a null `user_data` — the callback is given nothing it could
    // misread.
    unsafe { lv_obj_add_event_cb(obj, Some(event_cb), filter, core::ptr::null_mut()) };
}

/// The events [`send_event`] may send with no parameter and outside an input
/// device's read. In the vendored sources no class reads a parameter for any
/// of them (`lv_event_get_param` appears under none of these codes), every
/// class handler that asks which input device caused one checks for none
/// (lv_obj, slider, roller, button matrix, textarea), and LVGL itself sends
/// `VALUE_CHANGED` with a null parameter, so no object callback can rely on
/// one. `PRESSING` is left out: the button matrix reads its device
/// unchecked there.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Notice {
    Clicked,
    LongPressed,
    Pressed,
    Released,
    ValueChanged,
}

/// [`lv_obj_send_event`] for a [`Notice`], with a null parameter.
#[cfg(not(test))]
#[inline(always)]
pub fn send_event(obj: Obj, notice: Notice) -> u32 {
    let code = match notice {
        Notice::Clicked => LV_EVENT_CLICKED,
        Notice::LongPressed => LV_EVENT_LONG_PRESSED,
        Notice::Pressed => LV_EVENT_PRESSED,
        Notice::Released => LV_EVENT_RELEASED,
        Notice::ValueChanged => LV_EVENT_VALUE_CHANGED,
    };
    // SAFETY: a null parameter, for a code nothing reads a parameter of
    // (see `Notice`).
    unsafe { lv_obj_send_event(obj, code, core::ptr::null_mut()) }
}

/// [`lv_layout_create`] with a null `user_data`: the callbacks are given
/// nothing but the container.
#[cfg(not(test))]
#[inline(always)]
pub fn layout_create(
    update: unsafe extern "C" fn(*mut lv_obj_t, *mut c_void),
    min_size: Option<unsafe extern "C" fn(*mut lv_obj_t, *mut i32, bool, *mut c_void) -> bool>,
) -> u32 {
    let callbacks = lv_layout_callbacks_t {
        layout_update_cb: Some(update),
        get_min_size_cb: min_size,
    };
    // SAFETY: a null `user_data`, which the callbacks never read.
    unsafe { lv_layout_create(callbacks, core::ptr::null_mut()) }
}

/// [`lv_obj_set_local_style_prop`] with a number, on the main part: for a
/// numeric built-in property or one of this crate's own.
#[cfg(not(test))]
#[inline(always)]
pub fn set_style_num(obj: Obj, prop: lv_style_prop_t, num: i32) {
    // SAFETY: a numeric value for a property read as a number.
    unsafe { lv_obj_set_local_style_prop(obj, prop, lv_style_value_t { num }, 0) };
}

/// The number `obj`'s own (local) style holds for `prop` on the main part,
/// `None` when it holds none.
#[cfg(not(test))]
#[inline(always)]
pub fn local_style_num(obj: Obj, prop: lv_style_prop_t) -> Option<i32> {
    let mut value = lv_style_value_t { num: 0 };
    // SAFETY: `value` is a local LVGL may write one `lv_style_value_t` into.
    let res = unsafe { lv_obj_get_local_style_prop(obj, prop, &mut value, 0) };
    if res == LV_STYLE_RES_FOUND {
        // SAFETY: the property was written as a number (`set_style_num`, or
        // a numeric built-in), which LVGL stores in `num`.
        Some(unsafe { value.num })
    } else {
        None
    }
}

/// [`lv_obj_remove_style`] for every style bound to `selector`.
#[cfg(not(test))]
#[inline(always)]
pub fn remove_styles(obj: Obj, selector: lv_style_selector_t) {
    // SAFETY: a null style means "all of them".
    unsafe { lv_obj_remove_style(obj, core::ptr::null(), selector) };
}

/// [`lv_obj_set_style_transition`] with no transition: style changes on
/// `selector` apply at once.
#[cfg(not(test))]
#[inline(always)]
pub fn remove_transition(obj: Obj, selector: lv_style_selector_t) {
    // SAFETY: a null descriptor, which LVGL reads as "no transition".
    unsafe { lv_obj_set_style_transition(obj, core::ptr::null(), selector) };
}

/// [`lv_list_add_button`] with no icon; LVGL copies the text.
#[cfg(not(test))]
#[inline(always)]
pub fn list_add_button(list: Obj, text: &CStr) -> Option<Obj> {
    // SAFETY: no icon, and `text` is NUL-terminated and copied by LVGL.
    unsafe { lv_list_add_button(list, core::ptr::null(), text.as_ptr()) }
}

/// [`pd_canvas_text`] over a byte slice; the text is copied.
#[cfg(not(test))]
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub fn canvas_text(
    obj: Obj,
    x: i32,
    baseline: i32,
    font_index: i32,
    argb: u32,
    align: i32,
    utf8: &[u8],
) -> i32 {
    // SAFETY: `utf8` is `len` readable bytes, copied before the call returns.
    unsafe {
        pd_canvas_text(
            obj,
            x,
            baseline,
            font_index,
            argb,
            align,
            utf8.as_ptr().cast(),
            utf8.len(),
        )
    }
}

/// [`pd_canvas_text_width`] over a byte slice.
#[cfg(not(test))]
#[inline(always)]
pub fn canvas_text_width(font_index: i32, utf8: &[u8]) -> i32 {
    // SAFETY: `utf8` is `len` readable bytes, read before the call returns.
    unsafe { pd_canvas_text_width(font_index, utf8.as_ptr().cast(), utf8.len()) }
}

/// [`picodroid_hw_vscroll_region`] filling a slice of overlay areas.
#[cfg(all(not(test), hw_vscroll))]
#[inline(always)]
pub fn hw_vscroll_region(scroller: Obj, region: &mut lv_area_t, overlays: &mut [lv_area_t]) -> i32 {
    // SAFETY: `overlays` is `len` writable areas.
    unsafe {
        picodroid_hw_vscroll_region(
            scroller,
            region,
            overlays.as_mut_ptr(),
            overlays.len() as i32,
        )
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

    const LV_EVENT_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/core/lv_event.h");

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
            (LV_EVENT_LONG_PRESSED_REPEAT, "LV_EVENT_LONG_PRESSED_REPEAT"),
            (LV_EVENT_CLICKED, "LV_EVENT_CLICKED"),
            (LV_EVENT_RELEASED, "LV_EVENT_RELEASED"),
            (LV_EVENT_GESTURE, "LV_EVENT_GESTURE"),
            (LV_EVENT_VALUE_CHANGED, "LV_EVENT_VALUE_CHANGED"),
            (LV_EVENT_READY, "LV_EVENT_READY"),
            (LV_EVENT_KEY, "LV_EVENT_KEY"),
            (LV_EVENT_FOCUSED, "LV_EVENT_FOCUSED"),
            (LV_EVENT_DEFOCUSED, "LV_EVENT_DEFOCUSED"),
            (LV_EVENT_DELETE, "LV_EVENT_DELETE"),
            (LV_EVENT_SIZE_CHANGED, "LV_EVENT_SIZE_CHANGED"),
            (LV_EVENT_SCROLL, "LV_EVENT_SCROLL"),
            (LV_EVENT_INVALIDATE_AREA, "LV_EVENT_INVALIDATE_AREA"),
            (LV_EVENT_REFR_START, "LV_EVENT_REFR_START"),
            (LV_EVENT_RENDER_START, "LV_EVENT_RENDER_START"),
        ] {
            let header_ord = lookup_event_ordinal(name)
                .unwrap_or_else(|| panic!("{} not found in vendored lv_event.h", name));
            assert_eq!(
                rust_const, header_ord,
                "{}: Rust FFI ({}) drifted from vendored header ({}). \
                 The LVGL enum is implicit-ordinal — a single inserted variant \
                 shifts everything below it. Re-sync pd-lvgl-sys/src/lib.rs to match the \
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

    const LV_GROUP_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/core/lv_group.h");
    const LV_OBJ_STYLE_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/core/lv_obj_style.h");
    const LV_OBJ_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/core/lv_obj.h");
    const LV_COLOR_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/draw/lv_color.h");
    const LV_AREA_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/core/lv_area.h");
    const LV_FLEX_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/layouts/lv_flex.h");
    const LV_LAYOUT_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/layouts/lv_layout.h");
    const LV_GRID_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/layouts/lv_grid.h");
    const LV_BTNMATRIX_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/widgets/lv_buttonmatrix.h");
    const LV_IMAGE_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/widgets/lv_image.h");
    const LV_IMAGE_DSC_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/draw/lv_image_dsc.h");
    const LV_DRAW_RECT_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/draw/lv_draw_rect.h");
    const LV_STYLE_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/core/lv_style.h");
    const LV_LABEL_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/widgets/lv_label.h");
    const LV_TEXT_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/font/lv_text.h");
    const LV_ARC_HEADER: &str =
        include_str!("../../../third_party/lvgl/include/lvgl/widgets/lv_arc.h");
    const LV_CONF: &str = include_str!("../lvgl/lv_conf.h");
    const PD_CANVAS_HEADER: &str = include_str!("../lvgl/pd_canvas.h");
    const LV_DRAW_SW_FILL: &str =
        include_str!("../../../third_party/lvgl/src/draw/sw/lv_draw_sw_fill.c");
    const LV_DRAW_SW_TRIANGLE: &str =
        include_str!("../../../third_party/lvgl/src/draw/sw/lv_draw_sw_triangle.c");

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

    /// `lv_style_prop_t` is a plain `enum _lv_style_prop_t { … };` with no
    /// typedef close anchor for `enum_body`, so slice from its first member
    /// to the closing `};`. It mixes explicit `= N` anchors (`LV_STYLE_OPA =
    /// 112`) with implicit ordinals (`LV_STYLE_TRANSLATE_X` four past
    /// `LV_STYLE_MIN_WIDTH = 8`), which is exactly what `lookup_ordinal`
    /// models.
    fn style_prop_body() -> &'static str {
        let start = LV_STYLE_HEADER
            .find("LV_STYLE_PROP_INV")
            .expect("lv_style_prop_t enum not found in vendored lv_style.h");
        let end = LV_STYLE_HEADER[start..]
            .find("\n};")
            .expect("lv_style_prop_t enum unterminated");
        &LV_STYLE_HEADER[start..start + end]
    }

    #[test]
    fn lv_style_prop_constants_match_vendored_header() {
        let body = style_prop_body();
        for (rust_const, name) in [
            (LV_STYLE_TRANSLATE_X, "LV_STYLE_TRANSLATE_X"),
            (LV_STYLE_TRANSLATE_Y, "LV_STYLE_TRANSLATE_Y"),
            (LV_STYLE_OPA, "LV_STYLE_OPA"),
            (LV_STYLE_TRANSFORM_SCALE_X, "LV_STYLE_TRANSFORM_SCALE_X"),
            (LV_STYLE_TRANSFORM_SCALE_Y, "LV_STYLE_TRANSFORM_SCALE_Y"),
            (LV_STYLE_TRANSFORM_ROTATION, "LV_STYLE_TRANSFORM_ROTATION"),
            (LV_STYLE_MAX_HEIGHT, "LV_STYLE_MAX_HEIGHT"),
            (LV_STYLE_PAD_TOP, "LV_STYLE_PAD_TOP"),
            (LV_STYLE_PAD_BOTTOM, "LV_STYLE_PAD_BOTTOM"),
            (LV_STYLE_BORDER_WIDTH, "LV_STYLE_BORDER_WIDTH"),
            (LV_STYLE_TEXT_FONT, "LV_STYLE_TEXT_FONT"),
            (LV_STYLE_TEXT_LINE_SPACE, "LV_STYLE_TEXT_LINE_SPACE"),
            (LV_STYLE_FLEX_FLOW, "LV_STYLE_FLEX_FLOW"),
            (LV_STYLE_BG_OPA, "LV_STYLE_BG_OPA"),
            (LV_STYLE_BG_COLOR, "LV_STYLE_BG_COLOR"),
            (LV_STYLE_WIDTH, "LV_STYLE_WIDTH"),
            (LV_STYLE_HEIGHT, "LV_STYLE_HEIGHT"),
            (LV_STYLE_PAD_LEFT, "LV_STYLE_PAD_LEFT"),
            (LV_STYLE_PAD_RIGHT, "LV_STYLE_PAD_RIGHT"),
            (LV_STYLE_PAD_ROW, "LV_STYLE_PAD_ROW"),
            (LV_STYLE_PAD_COLUMN, "LV_STYLE_PAD_COLUMN"),
            (LV_STYLE_MARGIN_TOP, "LV_STYLE_MARGIN_TOP"),
            (LV_STYLE_MARGIN_BOTTOM, "LV_STYLE_MARGIN_BOTTOM"),
            (LV_STYLE_MARGIN_LEFT, "LV_STYLE_MARGIN_LEFT"),
            (LV_STYLE_MARGIN_RIGHT, "LV_STYLE_MARGIN_RIGHT"),
            (LV_STYLE_LAYOUT, "LV_STYLE_LAYOUT"),
            (LV_STYLE_FLEX_MAIN_PLACE, "LV_STYLE_FLEX_MAIN_PLACE"),
            (LV_STYLE_FLEX_CROSS_PLACE, "LV_STYLE_FLEX_CROSS_PLACE"),
            (LV_STYLE_FLEX_GROW, "LV_STYLE_FLEX_GROW"),
        ] {
            let header_val = lookup_ordinal(body, "LV_STYLE_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_style.h"));
            assert_eq!(
                u32::from(rust_const),
                header_val,
                "{name}: Rust FFI drifted from vendored lv_style.h — the animation \
                 engine reads its implicit `from` through lv_obj_get_style_prop with \
                 these ids, so a shift silently animates from the wrong property."
            );
        }
        // Anchor the helper on both sides of an explicit `= N`: the first
        // anchor and the implicit ordinal right after it.
        assert_eq!(lookup_ordinal(body, "LV_STYLE_", "LV_STYLE_WIDTH"), Some(1));
        assert_eq!(
            lookup_ordinal(body, "LV_STYLE_", "LV_STYLE_HEIGHT"),
            Some(2)
        );
        // Scale units: 256 = 1.0, used by to_units/from_units in animations.rs.
        assert_eq!(
            lookup_define(LV_STYLE_HEADER, "LV_SCALE_NONE"),
            Some(LV_SCALE_NONE as u32),
            "LV_SCALE_NONE drifted — View.setScaleX/getScaleX would mis-scale"
        );
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
    fn lv_label_long_mode_constants_match_vendored_header() {
        let body = enum_body(LV_LABEL_HEADER, "} lv_label_long_mode_t")
            .expect("lv_label_long_mode_t enum not found");
        for (rust_const, name) in [
            (LV_LABEL_LONG_MODE_WRAP, "LV_LABEL_LONG_MODE_WRAP"),
            (LV_LABEL_LONG_MODE_DOTS, "LV_LABEL_LONG_MODE_DOTS"),
            (LV_LABEL_LONG_MODE_SCROLL, "LV_LABEL_LONG_MODE_SCROLL"),
            (
                LV_LABEL_LONG_MODE_SCROLL_CIRCULAR,
                "LV_LABEL_LONG_MODE_SCROLL_CIRCULAR",
            ),
            (LV_LABEL_LONG_MODE_CLIP, "LV_LABEL_LONG_MODE_CLIP"),
        ] {
            let header_val = lookup_ordinal(body, "LV_LABEL_LONG_MODE_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_label.h"));
            assert_eq!(
                u32::from(rust_const),
                header_val,
                "{name}: Rust FFI drifted from vendored lv_label.h — TextView.setSingleLine / \
                 setEllipsize / setMaxLines pick the label's long mode by these values."
            );
        }
    }

    #[test]
    fn lv_coord_max_matches_vendored_header() {
        let line = LV_AREA_HEADER
            .lines()
            .find(|l| l.trim_start().starts_with("#define LV_COORD_TYPE_SHIFT"))
            .expect("LV_COORD_TYPE_SHIFT not found in vendored lv_area.h");
        let digits: String = line
            .split("LV_COORD_TYPE_SHIFT")
            .nth(1)
            .unwrap()
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect();
        let shift: u32 = digits.parse().expect("LV_COORD_TYPE_SHIFT value");
        assert_eq!(
            LV_COORD_MAX,
            (1i32 << shift) - 1,
            "LV_COORD_MAX drifted from vendored lv_area.h — TextView's line cap clears \
             max_height by setting it to this default."
        );
        // LV_COORD_TYPE_SPEC is `1 << LV_COORD_TYPE_SHIFT`; LV_SIZE_CONTENT ors
        // it into LV_COORD_MAX.
        assert_eq!(
            LV_SIZE_CONTENT,
            LV_COORD_MAX | (1i32 << shift),
            "LV_SIZE_CONTENT drifted from vendored lv_area.h — the snackbar's height \
             follows its content through it."
        );
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
            (LV_STATE_SCROLLED, "LV_STATE_SCROLLED"),
            (LV_STATE_DISABLED, "LV_STATE_DISABLED"),
            (LV_STATE_ANY, "LV_STATE_ANY"),
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
    fn lv_align_constants_match_vendored_header() {
        let body = enum_body(LV_AREA_HEADER, "} lv_align_t").expect("lv_align_t enum not found");
        for (rust_const, name) in [
            (LV_ALIGN_TOP_LEFT, "LV_ALIGN_TOP_LEFT"),
            (LV_ALIGN_TOP_MID, "LV_ALIGN_TOP_MID"),
            (LV_ALIGN_TOP_RIGHT, "LV_ALIGN_TOP_RIGHT"),
            (LV_ALIGN_BOTTOM_LEFT, "LV_ALIGN_BOTTOM_LEFT"),
            (LV_ALIGN_BOTTOM_MID, "LV_ALIGN_BOTTOM_MID"),
            (LV_ALIGN_BOTTOM_RIGHT, "LV_ALIGN_BOTTOM_RIGHT"),
            (LV_ALIGN_LEFT_MID, "LV_ALIGN_LEFT_MID"),
            (LV_ALIGN_RIGHT_MID, "LV_ALIGN_RIGHT_MID"),
            (LV_ALIGN_CENTER, "LV_ALIGN_CENTER"),
        ] {
            let header_val = lookup_ordinal(body, "LV_ALIGN_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_area.h"));
            assert_eq!(
                u32::from(rust_const),
                header_val,
                "{name}: Rust FFI drifted from vendored lv_area.h — a FrameLayout child's \
                 gravity picks its corner by these values."
            );
        }
    }

    #[test]
    fn lv_text_align_constants_match_vendored_header() {
        let body =
            enum_body(LV_TEXT_HEADER, "} lv_text_align_t").expect("lv_text_align_t enum not found");
        for (rust_const, name) in [
            (LV_TEXT_ALIGN_AUTO, "LV_TEXT_ALIGN_AUTO"),
            (LV_TEXT_ALIGN_LEFT, "LV_TEXT_ALIGN_LEFT"),
            (LV_TEXT_ALIGN_CENTER, "LV_TEXT_ALIGN_CENTER"),
            (LV_TEXT_ALIGN_RIGHT, "LV_TEXT_ALIGN_RIGHT"),
        ] {
            let header_val = lookup_ordinal(body, "LV_TEXT_ALIGN_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_text.h"));
            assert_eq!(
                u32::from(rust_const),
                header_val,
                "{name}: Rust FFI drifted from vendored lv_text.h — TextView.setGravity picks \
                 the label's text alignment by these values."
            );
        }
    }

    #[test]
    fn lv_part_constants_match_vendored_header() {
        let body = enum_body(LV_OBJ_STYLE_HEADER, "} lv_part_t").expect("lv_part_t enum not found");
        for (rust_const, name) in [
            (LV_PART_MAIN, "LV_PART_MAIN"),
            (LV_PART_INDICATOR, "LV_PART_INDICATOR"),
            (LV_PART_KNOB, "LV_PART_KNOB"),
            (LV_PART_ANY, "LV_PART_ANY"),
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
            (LV_OBJ_FLAG_CLICK_FOCUSABLE, "LV_OBJ_FLAG_CLICK_FOCUSABLE"),
            (LV_OBJ_FLAG_CHECKABLE, "LV_OBJ_FLAG_CHECKABLE"),
            (LV_OBJ_FLAG_SCROLLABLE, "LV_OBJ_FLAG_SCROLLABLE"),
            (LV_OBJ_FLAG_GESTURE_BUBBLE, "LV_OBJ_FLAG_GESTURE_BUBBLE"),
            (LV_OBJ_FLAG_IGNORE_LAYOUT, "LV_OBJ_FLAG_IGNORE_LAYOUT"),
            (LV_OBJ_FLAG_FLOATING, "LV_OBJ_FLAG_FLOATING"),
            (LV_OBJ_FLAG_USER_1, "LV_OBJ_FLAG_USER_1"),
            (LV_OBJ_FLAG_USER_2, "LV_OBJ_FLAG_USER_2"),
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
    fn lv_arc_mode_constants_match_vendored_header() {
        let body = enum_body(LV_ARC_HEADER, "} lv_arc_mode_t").expect("lv_arc_mode_t not found");
        for (rust_const, name) in [
            (LV_ARC_MODE_NORMAL, "LV_ARC_MODE_NORMAL"),
            (LV_ARC_MODE_REVERSE, "LV_ARC_MODE_REVERSE"),
        ] {
            let header_val = lookup_ordinal(body, "LV_ARC_MODE_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_arc.h"));
            assert_eq!(
                u32::from(rust_const),
                header_val,
                "{name}: Rust FFI drifted from vendored lv_arc.h — this enum is \
                 implicit-ordinal, so one inserted variant flips the ring's fill \
                 direction."
            );
        }
    }

    /// The arc angle externs are `i32` only while `LV_USE_FLOAT` is off:
    /// `lv_value_precise_t` becomes `float` otherwise (lv_types.h).
    #[test]
    fn lv_use_float_is_off() {
        assert_eq!(
            lookup_define(LV_CONF, "LV_USE_FLOAT"),
            Some(0),
            "LV_USE_FLOAT changed: the lv_arc angle externs must switch to f32"
        );
    }

    /// A horizontal gradient is not a fill: the software renderer blends
    /// the gradient map as an RGB888 source image, so the RGB888 source arm
    /// of the blenders must be linked even though no asset uses the format.
    /// D2 in docs/designs/claudeusage-gaps-roadmap-2026-09.md: with the
    /// switch off, `LEFT_RIGHT` GradientDrawables drew nothing, silently.
    /// The second half re-reads the vendored fill so an LVGL bump that
    /// changes the gradient source format shows up here, not on a display.
    #[test]
    fn rgb888_source_blender_stays_on_for_gradients() {
        assert_eq!(
            lookup_define(LV_CONF, "LV_DRAW_SW_SUPPORT_RGB888"),
            Some(1),
            "LV_DRAW_SW_SUPPORT_RGB888 is off: horizontal gradients (and gradient \
             triangles) fall into the blenders' unsupported-source arm and render \
             nothing"
        );
        for (name, src) in [
            ("lv_draw_sw_fill.c", LV_DRAW_SW_FILL),
            ("lv_draw_sw_triangle.c", LV_DRAW_SW_TRIANGLE),
        ] {
            assert!(
                src.contains("blend_dsc.src_color_format = LV_COLOR_FORMAT_RGB888;"),
                "vendored {name} no longer blends its gradient as an RGB888 source: \
                 re-check which LV_DRAW_SW_SUPPORT_* the gradient path needs and \
                 update lv_conf.h and this test together"
            );
        }
    }

    /// `build_support/lvgl.rs` compiles pd_draw_sw_stubs.c in place of
    /// LVGL's box-shadow and blur renderers, and lv_conf.h turns the line
    /// widget off (docs/designs/flash-budget-2026-09.md §6.5). Each rests on
    /// something this crate controls: the dark theme (the light one styles
    /// buttons with a shadow), no bound shadow or blur style setter, and no
    /// bound lv_line function. Bind one and this test says what to undo.
    #[test]
    fn stubbed_draw_paths_and_dropped_widgets_stay_unreachable() {
        assert_eq!(
            lookup_define(LV_CONF, "LV_THEME_DEFAULT_DARK"),
            Some(1),
            "the light default theme sets a button box shadow: restore \
             lv_draw_sw_box_shadow.c in build_support/lvgl.rs before switching"
        );
        assert_eq!(
            lookup_define(LV_CONF, "LV_USE_LINE"),
            Some(0),
            "LV_USE_LINE is on but no SDK widget wraps lv_line"
        );
        // The names are joined to `fn ` at run time so this test's own
        // source does not match itself.
        let me = include_str!("lib.rs");
        for name in [
            "lv_obj_set_style_shadow_",
            "lv_style_set_shadow_",
            "lv_obj_set_style_blur",
            "lv_style_set_blur",
            "lv_line_",
        ] {
            let decl = format!("fn {name}");
            assert!(
                !me.contains(&decl),
                "{name}* is bound: the renderer behind it is stubbed out \
                 (pd_draw_sw_stubs.c) or compiled out (LV_USE_LINE); restore it \
                 in build_support/lvgl.rs / lv_conf.h before using the binding"
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
    fn lv_style_res_and_prop_flags_match_vendored_header() {
        let body =
            enum_body(LV_STYLE_HEADER, "} lv_style_res_t").expect("lv_style_res_t not found");
        for (rust_const, name) in [
            (LV_STYLE_RES_NOT_FOUND, "LV_STYLE_RES_NOT_FOUND"),
            (LV_STYLE_RES_FOUND, "LV_STYLE_RES_FOUND"),
        ] {
            let header_val = lookup_ordinal(body, "LV_STYLE_RES_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_style.h"));
            assert_eq!(u32::from(rust_const), header_val, "{name} drifted");
        }
        for (rust_const, name) in [
            (
                LV_STYLE_PROP_FLAG_LAYOUT_UPDATE,
                "LV_STYLE_PROP_FLAG_LAYOUT_UPDATE",
            ),
            (
                LV_STYLE_PROP_FLAG_PARENT_LAYOUT_UPDATE,
                "LV_STYLE_PROP_FLAG_PARENT_LAYOUT_UPDATE",
            ),
        ] {
            assert_eq!(
                lookup_define(LV_STYLE_HEADER, name),
                Some(u32::from(rust_const)),
                "{name} drifted — a custom layout property would stop re-laying its parent"
            );
        }
        assert_eq!(
            lookup_ordinal(style_prop_body(), "LV_STYLE_", "LV_STYLE_PROP_INV"),
            Some(u32::from(LV_STYLE_PROP_INV)),
            "LV_STYLE_PROP_INV drifted"
        );
    }

    #[test]
    fn lv_layout_constants_match_vendored_header() {
        let body = enum_body(LV_LAYOUT_HEADER, "} lv_layout_t").expect("lv_layout_t not found");
        for (rust_const, name) in [
            (LV_LAYOUT_NONE, "LV_LAYOUT_NONE"),
            (LV_LAYOUT_FLEX, "LV_LAYOUT_FLEX"),
            (LV_LAYOUT_GRID, "LV_LAYOUT_GRID"),
        ] {
            let header_val = lookup_ordinal(body, "LV_LAYOUT_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_layout.h"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored lv_layout.h — the custom layouts \
                 take the ids after these."
            );
        }
        // Both built-in layouts are compiled in (lv_conf.h), so the first
        // custom id is 3: the one past GRID, where the header's enum ends.
        assert!(
            body.trim_end().ends_with("LV_LAYOUT_LAST"),
            "lv_layout_t grew past GRID"
        );
        assert!(
            LV_LAYOUT_HEADER.contains(
                "uint32_t lv_layout_create(lv_layout_callbacks_t callbacks, void * user_data);"
            ),
            "lv_layout_create's signature drifted"
        );
        assert!(
            LV_LAYOUT_HEADER.contains(
                "typedef bool (*lv_layout_get_min_size_cb_t)(lv_obj_t *, int32_t * req_size, bool width, void * user_data);"
            ),
            "lv_layout_get_min_size_cb_t drifted"
        );
    }

    #[test]
    fn lv_grid_constants_match_vendored_header() {
        let align =
            enum_body(LV_GRID_HEADER, "} lv_grid_align_t").expect("lv_grid_align_t not found");
        for (rust_const, name) in [
            (LV_GRID_ALIGN_START, "LV_GRID_ALIGN_START"),
            (LV_GRID_ALIGN_CENTER, "LV_GRID_ALIGN_CENTER"),
            (LV_GRID_ALIGN_END, "LV_GRID_ALIGN_END"),
            (LV_GRID_ALIGN_STRETCH, "LV_GRID_ALIGN_STRETCH"),
        ] {
            let header_val = lookup_ordinal(align, "LV_GRID_ALIGN_", name)
                .unwrap_or_else(|| panic!("{name} not found in vendored lv_grid.h"));
            assert_eq!(
                rust_const, header_val,
                "{name}: Rust FFI drifted from vendored lv_grid.h — \
                 GridLayout cell alignment routes through these."
            );
        }
        // The track-size macros are arithmetic on LV_COORD_MAX; pin their text.
        let define = |name: &str| -> String {
            let needle = format!("#define {name}");
            let line = LV_GRID_HEADER
                .lines()
                .find(|l| l.starts_with(&needle))
                .unwrap_or_else(|| panic!("{name} not defined in vendored lv_grid.h"));
            line[needle.len()..].split_whitespace().collect::<String>()
        };
        assert_eq!(
            define("LV_GRID_FR(x)"),
            "(LV_COORD_MAX-100+x)",
            "LV_GRID_FR drifted"
        );
        assert_eq!(
            define("LV_GRID_CONTENT"),
            "(LV_COORD_MAX-101)",
            "LV_GRID_CONTENT drifted"
        );
        assert_eq!(
            define("LV_GRID_TEMPLATE_LAST"),
            "(LV_COORD_MAX)",
            "LV_GRID_TEMPLATE_LAST drifted"
        );
        assert_eq!(lv_grid_fr(1), LV_COORD_MAX - 99);
        assert_eq!(lv_grid_fr(99), LV_GRID_TEMPLATE_LAST - 1);
    }

    #[test]
    fn lv_buttonmatrix_constants_match_vendored_header() {
        let body = enum_body(LV_BTNMATRIX_HEADER, "} lv_buttonmatrix_ctrl_t")
            .expect("lv_buttonmatrix_ctrl_t not found");
        for (rust_const, name) in [
            (
                LV_BUTTONMATRIX_CTRL_DISABLED as u32,
                "LV_BUTTONMATRIX_CTRL_DISABLED",
            ),
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
            (
                PD_CANVAS_MAX_BYTES as u32,
                "PD_CANVAS_MAX_BYTES",
                PD_CANVAS_HEADER,
                "pd_canvas.h",
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
            Some(LV_PART_SCROLLBAR)
        );
        assert_eq!(lookup_assigned_value(parts, "LV_PART_KNOB"), Some(0x030000));

        let flags = enum_body(LV_OBJ_HEADER, "} lv_obj_flag_t").unwrap();
        assert_eq!(
            lookup_assigned_value(flags, "LV_OBJ_FLAG_CLICK_FOCUSABLE"),
            Some(1 << 2)
        );
        assert_eq!(
            lookup_assigned_value(flags, "LV_OBJ_FLAG_SCROLL_ON_FOCUS"),
            Some(LV_OBJ_FLAG_SCROLL_ON_FOCUS)
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

    /// `lvgl/pd_fonts.c` is the one list of faces `TextView.setTextSize` can snap to. Hold its
    /// rows to the generated files in `lvgl/fonts/` and to `scripts/gen-fonts.sh`'s `SIZES`, so
    /// a face cannot be generated and forgotten, or listed and missing, or listed out of order
    /// (the runtime assumes an ascending table).
    #[test]
    fn pd_fonts_table_matches_the_generated_faces() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let read = |rel: &str| {
            std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
        };
        let table = read("lvgl/pd_fonts.c");
        let guarded: Vec<u32> = table
            .lines()
            .filter_map(|l| {
                l.trim()
                    .strip_prefix("#if PD_FONT_MONTSERRAT_")?
                    .trim()
                    .parse()
                    .ok()
            })
            .collect();
        assert!(!guarded.is_empty(), "pd_fonts.c lists no generated face");
        assert!(
            table.contains("{ &lv_font_montserrat_14, 14 },"),
            "the 14 px row is missing"
        );
        for &px in &guarded {
            assert!(
                table.contains(&format!("{{ &pd_font_montserrat_{px}, {px} }},")),
                "pd_fonts.c guards {px} px but has no table row for it"
            );
            assert!(
                table.contains(&format!("PD_ROW_{px}\n")),
                "pd_fonts.c defines a row for {px} px but never places it"
            );
        }
        let mut sorted = guarded.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            guarded, sorted,
            "pd_fonts.c rows must ascend by size, once each"
        );

        let mut files: Vec<u32> = std::fs::read_dir(root.join("lvgl/fonts"))
            .expect("lvgl/fonts")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter_map(|n| {
                n.strip_prefix("pd_font_montserrat_")?
                    .strip_suffix(".c")?
                    .parse()
                    .ok()
            })
            .collect();
        files.sort_unstable();
        assert_eq!(
            files, sorted,
            "lvgl/fonts/ and pd_fonts.c list different sizes"
        );
        for &px in &files {
            let face = read(&format!("lvgl/fonts/pd_font_montserrat_{px}.c"));
            assert!(
                face.contains(&format!("#if PD_FONT_MONTSERRAT_{px}\n")),
                "pd_font_montserrat_{px}.c was not generated with the guard pd_fonts.c expects"
            );
        }

        let script = read("../../scripts/gen-fonts.sh");
        let sizes = script
            .lines()
            .find_map(|l| l.strip_prefix("SIZES=\""))
            .expect("scripts/gen-fonts.sh: SIZES=\"...\"");
        let mut script_sizes: Vec<u32> = sizes
            .trim_end_matches('"')
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        script_sizes.sort_unstable();
        assert_eq!(
            script_sizes, sorted,
            "scripts/gen-fonts.sh SIZES and pd_fonts.c differ"
        );
    }
}
