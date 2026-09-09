// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `TextView` (LVGL `lv_label`).

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

/// Create an `lv_label` on the active screen and register its handle.
pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe { lv_label_create(lifecycle::screen_ptr()) };
    handle_table::register(ptr)
}

pub(in crate::graphics) fn set_text(id: i32, text: &str) {
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    unsafe { lv_label_set_text(handle_table::lookup(id), buf.as_ptr() as *const c_char) };
}

/// The label behind a `TextView` handle: the object itself for a bare label, its first child
/// for a `Button` (an `lv_button` holding a label). A private native such as `nativeSetLineMode`
/// is an `invokespecial` on `TextView`, so a `Button` receiver dispatches here too — and an
/// `lv_label` has no children, so a child means the button. Null for a stale handle.
fn label_of(id: i32) -> *mut lv_obj_t {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return obj;
    }
    let child = unsafe { lv_obj_get_child(obj, 0) };
    if child.is_null() {
        obj
    } else {
        child
    }
}

/// Copy a label's text into `dst` (capped at 256 bytes): the byte length written, or `None` for
/// a null label or text. In dots mode LVGL has overwritten the tail of its own buffer with the
/// dots; setting the text to NULL ("refresh the current text") restores it until the next layout
/// pass, so the copy is the whole text — what Android's `getText()` returns.
pub(in crate::graphics) fn label_text(label: *mut lv_obj_t, dst: &mut [u8; 256]) -> Option<usize> {
    if label.is_null() {
        return None;
    }
    unsafe {
        if lv_label_get_long_mode(label) == LV_LABEL_LONG_MODE_DOTS {
            lv_label_set_text(label, core::ptr::null());
        }
        copy_cstr(lv_label_get_text(label), dst)
    }
}

/// Copy the label's current text into `dst` (capped at 256 bytes). Returns
/// the byte length written, or `None` when LVGL returned a null pointer.
pub(in crate::graphics) fn get_text(id: i32, dst: &mut [u8; 256]) -> Option<usize> {
    label_text(label_of(id), dst)
}

/// `TextView.nativeSetLineMode`: the long mode for the ellipsize kind, and a `max_height` cap of
/// `lines` lines for a single-line or max-lines label. LVGL's dots mode puts the dots on the last
/// line that fits the box, and `max_height` clamps content-sized and explicit heights alike, so
/// the cap is what makes "one line" hold.
pub(in crate::graphics) fn set_line_mode(id: i32, kind: i32, max_lines: i32, single: bool) {
    let label = label_of(id);
    if label.is_null() {
        return;
    }
    let (mode, lines) = super::line_mode::line_mode(kind, max_lines, single);
    let cap = if lines > 0 {
        box_height_for(label, lines)
    } else {
        LV_COORD_MAX
    };
    unsafe {
        lv_label_set_long_mode(label, mode);
        lv_obj_set_style_max_height(label, cap, 0);
    }
}

/// The height of the label's box holding `lines` lines: the lines and the spacing between them,
/// plus the padding and border that `max_height` clamps together with the content (so the Java
/// side re-applies the mode after a padding change). `LV_COORD_MAX` — no cap — when the font
/// gives no line height to measure by.
fn box_height_for(label: *mut lv_obj_t, lines: i32) -> i32 {
    let num =
        |prop: lv_style_prop_t| unsafe { lv_obj_get_style_prop(label, LV_PART_MAIN, prop).num };
    let font = unsafe { lv_obj_get_style_prop(label, LV_PART_MAIN, LV_STYLE_TEXT_FONT).ptr }
        as *const lv_font_t;
    let line_height = if font.is_null() {
        0
    } else {
        unsafe { lv_font_get_line_height(font) }
    };
    if line_height <= 0 {
        return LV_COORD_MAX;
    }
    let pads = num(LV_STYLE_PAD_TOP) + num(LV_STYLE_PAD_BOTTOM);
    lines * line_height
        + (lines - 1) * num(LV_STYLE_TEXT_LINE_SPACE)
        + pads
        + 2 * num(LV_STYLE_BORDER_WIDTH)
}

/// Bounded copy of a NUL-terminated LVGL string. `c_char` is `i8` on x86_64
/// and `u8` on ARM; the cast is unconditional for portability.
pub(in crate::graphics) fn copy_cstr(cstr: *const c_char, dst: &mut [u8; 256]) -> Option<usize> {
    if cstr.is_null() {
        return None;
    }
    #[allow(clippy::unnecessary_cast)]
    let cstr = cstr as *const u8;
    let mut len = 0usize;
    unsafe {
        while len < dst.len() && *cstr.add(len) != 0 {
            len += 1;
        }
    }
    for (i, slot) in dst[..len].iter_mut().enumerate() {
        *slot = unsafe { *cstr.add(i) };
    }
    Some(len)
}

pub(in crate::graphics) fn set_text_color(id: i32, argb: u32) {
    let color = lv_color_t {
        red: ((argb >> 16) & 0xFF) as u8,
        green: ((argb >> 8) & 0xFF) as u8,
        blue: (argb & 0xFF) as u8,
    };
    unsafe { lv_obj_set_style_text_color(handle_table::lookup(id), color, 0) };
}

/// Mirrors Android `TextView.setIncludeFontPadding(boolean)`. `lv_label` content-sizes to the
/// font's full `line_height`, which leaves a few pixels of top side-bearing whitespace inside the
/// box; with `include = false` we apply negative top/bottom pad so the label height hugs the
/// glyphs and reads as balanced inside a flex column. Tuned for LVGL's bundled Montserrat font.
pub(in crate::graphics) fn set_include_font_padding(id: i32, include: bool) {
    const TOP_LEADING_PX: i32 = 3;
    const BOTTOM_LEADING_PX: i32 = 1;
    let pad_top = if include { 0 } else { -TOP_LEADING_PX };
    let pad_bot = if include { 0 } else { -BOTTOM_LEADING_PX };
    unsafe {
        let label = handle_table::lookup(id);
        lv_obj_set_style_pad_top(label, pad_top, 0);
        lv_obj_set_style_pad_bottom(label, pad_bot, 0);
    }
}
