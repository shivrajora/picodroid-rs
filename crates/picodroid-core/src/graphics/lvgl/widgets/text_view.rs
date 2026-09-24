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
    with_cstr(text, |p| unsafe {
        lv_label_set_text(handle_table::lookup(id), p)
    });
}

/// Run `f` on `text` as a NUL-terminated C string of any length. LVGL copies
/// the text into the widget, so the buffer is only needed for the call. A
/// 128-byte stack buffer used to cap every `setText` at 127 bytes (QA
/// 2026-09-13); it remains the fallback for a heap that cannot spare the
/// bytes, where the text is cut rather than the app stopped.
pub(in crate::graphics) fn with_cstr<R>(text: &str, f: impl FnOnce(*const c_char) -> R) -> R {
    let mut owned: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    if owned.try_reserve_exact(text.len() + 1).is_ok() {
        owned.extend_from_slice(text.as_bytes());
        owned.push(0);
        return f(owned.as_ptr() as *const c_char);
    }
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    f(buf.as_ptr() as *const c_char)
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
/// Run `f` over the label's current text, whatever its length. `None` when
/// there is no label or LVGL holds no text for it. Replaces a 256-byte
/// copy that silently truncated `getText()` of anything longer (QA
/// 2026-09-13).
pub(in crate::graphics) fn with_label_text<R>(
    label: *mut lv_obj_t,
    f: impl FnOnce(&[u8]) -> R,
) -> Option<R> {
    if label.is_null() {
        return None;
    }
    unsafe {
        if lv_label_get_long_mode(label) == LV_LABEL_LONG_MODE_DOTS {
            lv_label_set_text(label, core::ptr::null());
        }
        let text = lv_label_get_text(label);
        if text.is_null() {
            return None;
        }
        Some(f(cstr_bytes(text)))
    }
}

/// Run `f` over the view's current text (see [`with_label_text`]).
pub(in crate::graphics) fn with_text<R>(id: i32, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
    with_label_text(label_of(id), f)
}

/// The bytes of a NUL-terminated C string, without the terminator.
///
/// # Safety
/// `p` must point at a NUL-terminated string that outlives the returned
/// slice — LVGL's own text buffer, read while the widget is untouched.
pub(in crate::graphics) unsafe fn cstr_bytes<'a>(p: *const core::ffi::c_char) -> &'a [u8] {
    // c_char is i8 on x86_64 and u8 on ARM; cast unconditionally for portability.
    #[allow(clippy::unnecessary_cast)]
    let p = p as *const u8;
    let mut len = 0usize;
    while *p.add(len) != 0 {
        len += 1;
    }
    core::slice::from_raw_parts(p, len)
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

/// The `max_height` that holds the label's box to `lines` lines: the lines and the spacing between
/// them, plus the padding and border that `max_height` clamps together with the content (so the
/// Java side re-applies the mode after a padding change). `LV_COORD_MAX` — no cap — when the font
/// gives no line height to measure by.
fn box_height_for(label: *mut lv_obj_t, lines: i32) -> i32 {
    let num =
        |prop: lv_style_prop_t| unsafe { lv_obj_get_style_prop(label, LV_PART_MAIN, prop).num };
    let line_height = line_height_of(label);
    if line_height <= 0 {
        return LV_COORD_MAX;
    }
    let text = lines * line_height + (lines - 1) * num(LV_STYLE_TEXT_LINE_SPACE);
    let frame = num(LV_STYLE_PAD_TOP) + num(LV_STYLE_PAD_BOTTOM) + 2 * num(LV_STYLE_BORDER_WIDTH);
    line_cap(text, frame)
}

/// The `max_height` for a label whose text should measure `text` px inside a frame of pads and
/// border `frame` px (negative under `setIncludeFontPadding(false)`). LVGL's label clamps its own
/// text height by `max_height` (`lv_label` `GET_SELF_SIZE`) before the frame is added and the box
/// clamped again, so a cap that carried a negative frame would take it off twice: a 64 px face
/// trimmed by 9 px measured 48 px, not 57, and a row centring it dropped the digits 5 px. A
/// positive frame belongs in the cap, or the box would clamp to less than the frame plus the text.
fn line_cap(text: i32, frame: i32) -> i32 {
    text + frame.max(0)
}

/// The face a label draws with: its `LV_STYLE_TEXT_FONT`, inherited from the theme until
/// `set_text_size` sets one.
fn font_of(label: *mut lv_obj_t) -> *const lv_font_t {
    unsafe {
        lv_obj_get_style_prop(label, LV_PART_MAIN, LV_STYLE_TEXT_FONT).ptr as *const lv_font_t
    }
}

/// The line height of a label's face; 0 when there is none to measure by.
fn line_height_of(label: *mut lv_obj_t) -> i32 {
    let font = font_of(label);
    if font.is_null() {
        0
    } else {
        unsafe { lv_font_get_line_height(font) }
    }
}

/// `TextView.nativeSetTextSize`: the compiled face nearest `px` (`text_size::nearest`) on the
/// label — a `Button` receiver lands here too, hence `label_of`. The face table is whatever the
/// C build compiled for this board (`pd_fonts.c`), so the ladder is asked for, never assumed.
/// `LV_STYLE_TEXT_FONT` carries LVGL's layout-update flag: the label, and a content-sized button
/// around it, re-lay out on the next frame by themselves.
pub(in crate::graphics) fn set_text_size(id: i32, px: f32) {
    let label = label_of(id);
    if label.is_null() {
        return;
    }
    let mut count = 0usize;
    let table = unsafe { pd_font_table(&mut count) };
    if table.is_null() || count == 0 {
        return;
    }
    let faces = unsafe { core::slice::from_raw_parts(table, count) };
    // The ladder is a handful of faces; a fixed buffer keeps the pick allocation-free.
    let mut sizes = [0u8; 16];
    let n = count.min(sizes.len());
    for (size, face) in sizes.iter_mut().zip(faces) {
        *size = face.px;
    }
    let Some(i) = super::text_size::nearest(&sizes[..n], px) else {
        return;
    };
    unsafe { lv_obj_set_style_text_font(label, faces[i].font, 0) };
}

/// `TextView.nativeGetLineHeight`: one line of the face in use, in pixels; 0 for a stale handle.
pub(in crate::graphics) fn line_height(id: i32) -> i32 {
    let label = label_of(id);
    if label.is_null() {
        0
    } else {
        line_height_of(label)
    }
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
/// glyphs and reads as balanced inside a flex column. The top trim is the face's own leading
/// above its digits (`pd_font_top_leading`: 3 px for Montserrat 14, 7 px for the 64 px face —
/// not proportional, so it is measured, never scaled; a trim past the glyph tops would clip them),
/// the bottom trim the 1 px of descent the 14 px tuning gave up. The Java side re-applies it after
/// a size change. On the label itself (`label_of`), where the line cap reads the pads.
pub(in crate::graphics) fn set_include_font_padding(id: i32, include: bool) {
    const BOTTOM_LEADING_PX: i32 = 1;
    let label = label_of(id);
    if label.is_null() {
        return;
    }
    let pad_top = if include {
        0
    } else {
        -unsafe { pd_font_top_leading(font_of(label)) }
    };
    let pad_bot = if include { 0 } else { -BOTTOM_LEADING_PX };
    unsafe {
        lv_obj_set_style_pad_top(label, pad_top, 0);
        lv_obj_set_style_pad_bottom(label, pad_bot, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::line_cap;

    #[test]
    fn a_positive_frame_is_in_the_cap() {
        // A bordered button label: text plus its frame, so the box is not clamped short.
        assert_eq!(line_cap(16, 4), 20);
        assert_eq!(line_cap(16, 0), 16);
    }

    #[test]
    fn a_negative_frame_is_left_out_of_the_cap() {
        // Font padding off on the 64 px face: LVGL takes the -9 off the box itself, so a cap of
        // 57 would measure 48.
        assert_eq!(line_cap(66, -9), 66);
        assert_eq!(line_cap(16, -4), 16);
    }
}
