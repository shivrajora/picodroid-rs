// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `TextView` (LVGL `lv_label`).

use crate::lvgl_ffi::*;
use core::ffi::{c_char, CStr};

use super::super::handle_table;
use super::super::lifecycle;

/// Create an `lv_label` on the active screen and register its handle.
pub(in crate::graphics) fn create() -> i32 {
    lifecycle::screen_ptr().map_or(0, create_under)
}

/// [`create`], under `parent` instead of the screen (the inflater's path).
#[inline(never)]
pub(in crate::graphics) fn create_under(parent: Obj) -> i32 {
    match lv_label_create(parent) {
        Some(label) => handle_table::register(label),
        None => 0,
    }
}

/// Set the label's text: the object's own for a bare label, the child label's
/// for a `Button` (see [`label_of`]); an `EditText`'s text area keeps its
/// cursor and typed text, so it takes the textarea call.
pub(in crate::graphics) fn set_text(id: i32, text: &str) {
    if textarea_of(id).is_some() {
        super::edit_text::set_text(id, text);
        return;
    }
    let Some(label) = label_of(id) else {
        return; // stale handle — mutating a destroyed View is a no-op
    };
    // SAFETY: `label_of` answers only an `lv_label`; the text is
    // NUL-terminated and LVGL copies it.
    with_cstr(text, |p| unsafe { lv_label_set_text(label, p.as_ptr()) });
}

/// Run `f` on `text` as a NUL-terminated C string of any length. LVGL copies
/// the text into the widget, so the buffer is only needed for the call. A
/// 128-byte stack buffer used to cap every `setText` at 127 bytes (QA
/// 2026-09-13); it remains the fallback for a heap that cannot spare the
/// bytes, where the text is cut rather than the app stopped. A NUL inside
/// `text` ends the string there, as it always did in C.
pub(in crate::graphics) fn with_cstr<R>(text: &str, f: impl FnOnce(&CStr) -> R) -> R {
    let mut owned: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    if owned.try_reserve_exact(text.len() + 1).is_ok() {
        owned.extend_from_slice(text.as_bytes());
        owned.push(0);
        // SAFETY: `owned` ends in the NUL just pushed, so the string is
        // terminated inside the allocation, which outlives the borrow.
        // (`from_ptr` rather than `from_bytes_until_nul`: the same answer
        // through the `strlen` LVGL already links, without core's 448-byte
        // `memchr`.)
        return f(unsafe { CStr::from_ptr(owned.as_ptr().cast()) });
    }
    let mut buf = [0u8; 128];
    let len = text.len().min(127);
    buf[..len].copy_from_slice(&text.as_bytes()[..len]);
    buf[len] = 0;
    // SAFETY: `buf[len]` is the NUL just written, inside `buf`, which
    // outlives the borrow.
    f(unsafe { CStr::from_ptr(buf.as_ptr().cast()) })
}

/// The label behind a `TextView` handle: the object itself for a bare label, its first child
/// for a `Button` (an `lv_button` holding a label). A private native such as `nativeSetLineMode`
/// is an `invokespecial` on `TextView`, so a `Button` receiver dispatches here too — and an
/// `lv_label` has no children, so a child means the button. `None` for a stale handle, and for
/// anything that is not a label after all: `TextView`'s constructor that takes a handle is
/// `protected`, so a subclass can hand it any widget, and the `lv_label_*` calls cast blindly
/// (`LV_USE_CHECK_OBJ_CLASSTYPE` is 0).
fn label_of(id: i32) -> Option<Obj> {
    let obj = handle_table::get(id)?;
    let label = lv_obj_get_child(obj, 0).unwrap_or(obj);
    super::super::is_class(label, &raw const lv_label_class).then_some(label)
}

/// The text area behind an `EditText` handle (`EditText extends TextView`, so TextView's natives
/// land here with one): `None` for a label, a button or a stale handle. Its child 0 is a label
/// too, which `label_of` would answer — but the cursor and placeholder measure with the
/// textarea's own font and alignment (`lv_textarea.c`), so those styles go on the textarea, and
/// the text and line mode through its own API.
fn textarea_of(id: i32) -> Option<Obj> {
    let obj = handle_table::get(id)?;
    super::super::is_class(obj, &raw const lv_textarea_class).then_some(obj)
}

/// The bytes of a NUL-terminated C string, without the terminator.
///
/// # Safety
/// `p` must point at a NUL-terminated string that outlives the returned
/// slice — LVGL's own text buffer, read while the widget is untouched.
pub(in crate::graphics) unsafe fn cstr_bytes<'a>(p: *const c_char) -> &'a [u8] {
    // SAFETY: the caller's contract: `p` is a NUL-terminated string that
    // outlives `'a`.
    unsafe { CStr::from_ptr(p) }.to_bytes()
}

/// `TextView.nativeSetLineMode`: the long mode for the ellipsize kind, and a `max_height` cap of
/// `lines` lines for a single-line or max-lines label. LVGL's dots mode puts the dots on the last
/// line that fits the box, and `max_height` clamps content-sized and explicit heights alike, so
/// the cap is what makes "one line" hold.
pub(in crate::graphics) fn set_line_mode(id: i32, kind: i32, max_lines: i32, single: bool) {
    if let Some(ta) = textarea_of(id) {
        // A field has no ellipsis or line cap, only Android's single-line mode.
        // SAFETY: `textarea_of` answers only an `lv_textarea`.
        unsafe { lv_textarea_set_one_line(ta, single) };
        return;
    }
    let Some(label) = label_of(id) else {
        return;
    };
    let (mode, lines) = super::line_mode::line_mode(kind, max_lines, single);
    let cap = if lines > 0 {
        box_height_for(label, lines)
    } else {
        LV_COORD_MAX
    };
    // SAFETY: `label_of` answers only an `lv_label`.
    unsafe { lv_label_set_long_mode(label, mode) };
    lv_obj_set_style_max_height(label, cap, 0);
}

/// The `max_height` that holds the label's box to `lines` lines: the lines and the spacing between
/// them, plus the padding and border that `max_height` clamps together with the content (so the
/// Java side re-applies the mode after a padding change). `LV_COORD_MAX` — no cap — when the font
/// gives no line height to measure by.
fn box_height_for(label: Obj, lines: i32) -> i32 {
    let num = |prop: lv_style_prop_t| {
        let v = lv_obj_get_style_prop(label, LV_PART_MAIN, prop);
        // SAFETY: line space, pads and border width are numeric properties,
        // which LVGL returns in `num`.
        unsafe { v.num }
    };
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
fn font_of(label: Obj) -> *const lv_font_t {
    let v = lv_obj_get_style_prop(label, LV_PART_MAIN, LV_STYLE_TEXT_FONT);
    // SAFETY: the text font is a pointer property, which LVGL returns in
    // `ptr`.
    unsafe { v.ptr as *const lv_font_t }
}

/// The line height of a label's face; 0 when there is none to measure by.
fn line_height_of(label: Obj) -> i32 {
    let font = font_of(label);
    if font.is_null() {
        0
    } else {
        // SAFETY: a label's text font is a face LVGL holds for the label's
        // life: the theme's, or one of `pd_font_table`'s compiled faces
        // (`set_text_size`).
        unsafe { lv_font_get_line_height(font) }
    }
}

/// `TextView.nativeSetTextSize`: the compiled face nearest `px` (`text_size::nearest`) on the
/// label — a `Button` receiver lands here too, hence `label_of`. The face table is whatever the
/// C build compiled for this board (`pd_fonts.c`), so the ladder is asked for, never assumed.
/// `LV_STYLE_TEXT_FONT` carries LVGL's layout-update flag: the label, and a content-sized button
/// around it, re-lay out on the next frame by themselves.
pub(in crate::graphics) fn set_text_size(id: i32, px: f32) {
    // On a text area the font goes on the area itself: its label inherits it, and the cursor and
    // placeholder measure with the area's font.
    let Some(target) = textarea_of(id).or_else(|| label_of(id)) else {
        return;
    };
    let font = face_for_px(px);
    if font.is_null() {
        return;
    }
    // SAFETY: `font` is one of `pd_font_table`'s compiled faces, static.
    unsafe { lv_obj_set_style_text_font(target, font, 0) };
}

/// The compiled face nearest `px` (`text_size::nearest` over `pd_fonts.c`'s table), or null for
/// an empty table or a NaN. `TextView.setTextSize` applies it; `Canvas.drawText` records its row
/// ([`face_index_for_px`]), so a paint's text size snaps to the same ladder a label's does.
pub(in crate::graphics) fn face_for_px(px: f32) -> *const lv_font_t {
    match face_index_for_px(px) {
        Some(i) => font_table()
            .get(i)
            .map_or(core::ptr::null(), |face| face.font),
        None => core::ptr::null(),
    }
}

/// `pd_fonts.c`'s face table: the faces this build compiled, ascending.
fn font_table() -> &'static [pd_font_t] {
    let mut count = 0usize;
    let table = pd_font_table(&mut count);
    if table.is_null() {
        return &[];
    }
    // SAFETY: `pd_font_table` returns a static array of `count` rows
    // (lvgl/pd_fonts.c), never written.
    unsafe { core::slice::from_raw_parts(table, count) }
}

/// The row of `pd_fonts.c`'s table nearest `px`, or `None` for an empty table or a NaN.
pub(in crate::graphics) fn face_index_for_px(px: f32) -> Option<usize> {
    let faces = font_table();
    if faces.is_empty() {
        return None;
    }
    // The ladder is a handful of faces; a fixed buffer keeps the pick allocation-free.
    let mut sizes = [0u8; 16];
    let n = faces.len().min(sizes.len());
    for (size, face) in sizes.iter_mut().zip(faces) {
        *size = face.px;
    }
    super::text_size::nearest(&sizes[..n], px)
}

/// `TextView.nativeSetGravity`: the horizontal field of an Android gravity as the label's
/// `text_align` (`gravity::text_align`). Only visible when the label is wider than its text — a
/// content-sized label has no slack — which is also when Android's `setGravity` shows. A `Button`
/// receiver lands here too (`label_of`): its label is content-sized and centred by the button, so
/// the call is honoured but changes nothing, as on Android where a Button is centred anyway.
pub(in crate::graphics) fn set_gravity(id: i32, gravity: i32) {
    // A text area aligns its text, placeholder and cursor by its own style (`lv_textarea_set_align`).
    let Some(target) = textarea_of(id).or_else(|| label_of(id)) else {
        return;
    };
    lv_obj_set_style_text_align(target, super::gravity::text_align(gravity), 0);
}

/// `TextView.nativeGetLineHeight`: one line of the face in use, in pixels; 0 for a stale handle.
pub(in crate::graphics) fn line_height(id: i32) -> i32 {
    label_of(id).map_or(0, line_height_of)
}

pub(in crate::graphics) fn set_text_color(id: i32, argb: u32) {
    let color = lv_color_t {
        red: ((argb >> 16) & 0xFF) as u8,
        green: ((argb >> 8) & 0xFF) as u8,
        blue: (argb & 0xFF) as u8,
    };
    let Some(obj) = handle_table::get(id) else {
        return; // stale handle — mutating a destroyed View is a no-op
    };
    lv_obj_set_style_text_color(obj, color, 0);
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
    if textarea_of(id).is_some() {
        return; // a field's padding is the text area's own tuning (`edit_text::create`)
    }
    let Some(label) = label_of(id) else {
        return;
    };
    let pad_top = if include {
        0
    } else {
        // SAFETY: `font_of` answers null or the label's face, which LVGL
        // holds for the label's life (see `line_height_of`).
        -unsafe { pd_font_top_leading(font_of(label)) }
    };
    let pad_bot = if include { 0 } else { -BOTTOM_LEADING_PX };
    lv_obj_set_style_pad_top(label, pad_top, 0);
    lv_obj_set_style_pad_bottom(label, pad_bot, 0);
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
