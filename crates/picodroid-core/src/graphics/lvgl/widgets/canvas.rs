// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of the drawing `View` (`new View(Context)` + `onDraw(Canvas)`).
//!
//! The widget is a plain, flat `lv_obj` carrying a display list
//! (`pd-lvgl-sys/lvgl/pd_canvas.c`): `Canvas.drawX` records one op, and the
//! list's `LV_EVENT_DRAW_MAIN` hook replays the ops whenever LVGL paints the
//! view. This file owns the Android-to-LVGL geometry. Android coordinates are
//! floats with exclusive right and bottom edges and a stroke centred on the
//! outline; LVGL wants whole pixels, inclusive edges, and draws a border and
//! an arc's width inward from the outline. The conversions are pure functions
//! so `cargo test` covers them; LVGL is linked on the host but never
//! initialised.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;

/// `Paint.Style` ordinals; FILL_AND_STROKE (2) is every other value.
pub(crate) const STYLE_FILL: i32 = 0;
pub(crate) const STYLE_STROKE: i32 = 1;
/// `Paint.Cap.ROUND`'s ordinal; BUTT and SQUARE both draw butt ends.
pub(crate) const CAP_ROUND: i32 = 1;

/// The `Paint` settings the ops read (`graphics/fields.rs::paint`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaintSpec {
    pub color: u32,
    pub stroke_width: f32,
    pub style: i32,
    pub cap: i32,
    pub text_size: f32,
    pub text_align: i32,
}

/// The nearest whole pixel, `floor(v + 0.5)`. Written so the answer is the same
/// whether the target's `as i32` truncates or floors (the RP2040's ROM
/// conversion floors negatives); a NaN is 0.
pub(crate) fn round_px(v: f32) -> i32 {
    if v.is_nan() {
        return 0;
    }
    let f = v + 0.5;
    let t = f as i32;
    if (t as f32) > f {
        t - 1
    } else {
        t
    }
}

/// A stroke's drawn width: Android's 0 is a one-pixel hairline.
fn stroke_px(width: f32) -> i32 {
    let w = round_px(width);
    if w < 1 {
        1
    } else {
        w
    }
}

/// One `pd_canvas_rect` call: inclusive corners, corner radius, fill (alpha 0
/// for none), border colour and width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RectOp {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub radius: i32,
    pub fill: u32,
    pub stroke: u32,
    pub stroke_width: i32,
}

/// `drawRect` / `drawRoundRect` / `drawCircle` in LVGL terms, or `None` when
/// Android would draw nothing. A stroke straddles the outline, so a stroked
/// shape grows by half the stroke on every side and LVGL's inward border
/// covers exactly Android's band; FILL_AND_STROKE is then one fill of the
/// grown shape, the stroke and the fill being the same colour.
pub(crate) fn rect_op(
    l: f32,
    t: f32,
    r: f32,
    b: f32,
    radius: f32,
    p: &PaintSpec,
) -> Option<RectOp> {
    if !(r > l && b > t) {
        return None;
    }
    let grow = if p.style == STYLE_FILL {
        0.0
    } else {
        stroke_px(p.stroke_width) as f32 / 2.0
    };
    let radius = if radius > 0.0 {
        round_px(radius + grow)
    } else {
        0
    };
    let x1 = round_px(l - grow);
    let y1 = round_px(t - grow);
    let x2 = round_px(r + grow) - 1;
    let y2 = round_px(b + grow) - 1;
    if x2 < x1 || y2 < y1 {
        return None;
    }
    let (fill, stroke, stroke_width) = match p.style {
        STYLE_STROKE => (0, p.color, stroke_px(p.stroke_width)),
        _ => (p.color, 0, 0),
    };
    Some(RectOp {
        x1,
        y1,
        x2,
        y2,
        radius,
        fill,
        stroke,
        stroke_width,
    })
}

/// One `pd_canvas_arc` call: centre, outer radius, start and end angle
/// (LVGL's, clockwise from 3 o'clock like Android's), width, round ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ArcOp {
    pub cx: i32,
    pub cy: i32,
    pub radius: i32,
    pub start: i32,
    pub end: i32,
    pub width: i32,
    pub rounded: bool,
}

/// Android's `startAngle` / `sweepAngle` as LVGL's start and end: a start in
/// [0, 360), an end after it, and a sweep of a full turn or more as exactly
/// `start + 360`, the one spelling LVGL reads as a whole circle. A negative
/// sweep runs anticlockwise, so it is the same arc started at `start + sweep`.
/// `None` for a sweep that rounds to nothing.
pub(crate) fn arc_angles(start: f32, sweep: f32) -> Option<(i32, i32)> {
    let sweep_i = round_px(sweep);
    if sweep_i == 0 {
        return None;
    }
    if sweep_i >= 360 || sweep_i <= -360 {
        let s = round_px(start).rem_euclid(360);
        return Some((s, s + 360));
    }
    let (s, len) = if sweep_i < 0 {
        (round_px(start) + sweep_i, -sweep_i)
    } else {
        (round_px(start), sweep_i)
    };
    let s = s.rem_euclid(360);
    Some((s, s + len))
}

/// `drawArc` over the circle inscribed in the bounds (the smaller side), or
/// `None` when Android would draw nothing. Filled, the arc is a wedge: LVGL's
/// arc as wide as its radius. Stroked, it straddles the circle like the rect's
/// border does.
pub(crate) fn arc_op(
    l: f32,
    t: f32,
    r: f32,
    b: f32,
    start: f32,
    sweep: f32,
    p: &PaintSpec,
) -> Option<ArcOp> {
    if !(r > l && b > t) {
        return None;
    }
    let (start, end) = arc_angles(start, sweep)?;
    let side = if r - l < b - t { r - l } else { b - t };
    let half = side / 2.0;
    let cx = round_px((l + r) / 2.0);
    let cy = round_px((t + b) / 2.0);
    let (radius, width, rounded) = match p.style {
        STYLE_FILL => {
            let radius = round_px(half);
            (radius, radius, false)
        }
        STYLE_STROKE => {
            let w = stroke_px(p.stroke_width);
            (round_px(half + w as f32 / 2.0), w, p.cap == CAP_ROUND)
        }
        _ => {
            let radius = round_px(half + stroke_px(p.stroke_width) as f32 / 2.0);
            (radius, radius, false)
        }
    };
    if radius <= 0 {
        return None;
    }
    Some(ArcOp {
        cx,
        cy,
        radius,
        start,
        end,
        width,
        rounded,
    })
}

/// `Paint.Align` ordinal as an `LV_TEXT_ALIGN_*` value.
pub(crate) fn text_align(ordinal: i32) -> i32 {
    match ordinal {
        1 => i32::from(LV_TEXT_ALIGN_CENTER),
        2 => i32::from(LV_TEXT_ALIGN_RIGHT),
        _ => i32::from(LV_TEXT_ALIGN_LEFT),
    }
}

// ---------------------------------------------------------------------------
// LVGL
// ---------------------------------------------------------------------------

/// `View.nativeCreateView`: a flat, transparent, non-scrolling, non-clickable
/// object with a display list. A plain Android `View` takes no touches until it
/// gets a listener, which sets CLICKABLE again (`view::register_click_listener`).
pub(in crate::graphics) fn create() -> i32 {
    let ptr = unsafe {
        let o = lv_obj_create(lifecycle::screen_ptr());
        super::frame_layout::make_flat(o);
        lv_obj_remove_flag(o, LV_OBJ_FLAG_SCROLLABLE | LV_OBJ_FLAG_CLICKABLE);
        if pd_canvas_attach(o) != 0 {
            note_no_list();
        }
        o
    };
    handle_table::register(ptr)
}

/// `View.nativeBeginDraw`: empties the view's list and returns its laid-out
/// size, which the canvas reports as `getWidth` / `getHeight`. A size set in
/// this tick is not laid out yet, so layout runs first (a no-op when clean).
pub(in crate::graphics) fn begin(id: i32) -> (i32, i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return (0, 0);
    }
    unsafe {
        lv_obj_update_layout(obj);
        pd_canvas_begin(obj);
        (lv_obj_get_width(obj), lv_obj_get_height(obj))
    }
}

/// `View.nativeEndDraw`: the list is complete; repaint the view.
pub(in crate::graphics) fn end(id: i32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    let dropped = unsafe { pd_canvas_end(obj) };
    if dropped > 0 {
        note_dropped(dropped);
    }
}

pub(in crate::graphics) fn fill(id: i32, argb: u32) {
    let obj = handle_table::lookup(id);
    if !obj.is_null() {
        unsafe { pd_canvas_fill(obj, argb) };
    }
}

pub(in crate::graphics) fn rect(id: i32, op: &RectOp) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe {
        pd_canvas_rect(
            obj,
            op.x1,
            op.y1,
            op.x2,
            op.y2,
            op.radius,
            op.fill,
            op.stroke,
            op.stroke_width,
        )
    };
}

pub(in crate::graphics) fn line(id: i32, x1: f32, y1: f32, x2: f32, y2: f32, p: &PaintSpec) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe {
        pd_canvas_line(
            obj,
            round_px(x1),
            round_px(y1),
            round_px(x2),
            round_px(y2),
            stroke_px(p.stroke_width),
            p.color,
            i32::from(p.cap == CAP_ROUND),
        )
    };
}

pub(in crate::graphics) fn arc(id: i32, op: &ArcOp, argb: u32) {
    let obj = handle_table::lookup(id);
    if obj.is_null() {
        return;
    }
    unsafe {
        pd_canvas_arc(
            obj,
            op.cx,
            op.cy,
            op.radius,
            op.start,
            op.end,
            op.width,
            argb,
            i32::from(op.rounded),
        )
    };
}

pub(in crate::graphics) fn text(id: i32, s: &str, x: f32, y: f32, p: &PaintSpec) {
    let obj = handle_table::lookup(id);
    if obj.is_null() || s.is_empty() {
        return;
    }
    let Some(face) = super::text_view::face_index_for_px(p.text_size) else {
        return;
    };
    unsafe {
        pd_canvas_text(
            obj,
            round_px(x),
            round_px(y),
            face as i32,
            p.color,
            text_align(p.text_align),
            s.as_ptr().cast(),
            s.len(),
        )
    };
}

/// `Paint.ascent()`: the face's line top above the baseline, negative.
pub(in crate::graphics) fn ascent(text_size: f32) -> i32 {
    match super::text_view::face_index_for_px(text_size) {
        Some(face) => unsafe { pd_canvas_ascent(face as i32) },
        None => 0,
    }
}

/// `Paint.descent()`: the face's line bottom below the baseline.
pub(in crate::graphics) fn descent(text_size: f32) -> i32 {
    match super::text_view::face_index_for_px(text_size) {
        Some(face) => unsafe { pd_canvas_descent(face as i32) },
        None => 0,
    }
}

/// `Paint.measureText()`: one line's width in the face.
pub(in crate::graphics) fn text_width(s: &str, text_size: f32) -> i32 {
    match super::text_view::face_index_for_px(text_size) {
        Some(face) if !s.is_empty() => unsafe {
            pd_canvas_text_width(face as i32, s.as_ptr().cast(), s.len())
        },
        _ => 0,
    }
}

/// A view whose list header could not be allocated draws nothing.
#[cold]
fn note_no_list() {
    #[cfg(any(test, feature = "sim"))]
    eprintln!("[sim] View(Context): no room in the LVGL pool for a display list; it draws nothing");
    #[cfg(not(any(test, feature = "sim")))]
    defmt::warn!("View(Context): no LVGL pool for a display list; it draws nothing");
}

/// An `onDraw` recorded more than one view's list holds.
#[cold]
fn note_dropped(dropped: i32) {
    #[cfg(any(test, feature = "sim"))]
    eprintln!(
        "[sim] Canvas: onDraw recorded more than {} bytes of ops; {dropped} dropped",
        crate::lvgl_ffi::PD_CANVAS_MAX_BYTES
    );
    #[cfg(not(any(test, feature = "sim")))]
    defmt::warn!(
        "Canvas: onDraw overflowed its list; {=i32} ops dropped",
        dropped
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLE_FILL_AND_STROKE: i32 = 2;

    fn paint(style: i32, stroke_width: f32) -> PaintSpec {
        PaintSpec {
            color: 0xFF11_2233,
            stroke_width,
            style,
            cap: 0,
            text_size: 14.0,
            text_align: 0,
        }
    }

    #[test]
    fn round_px_is_floor_of_half_up_on_both_sides_of_zero() {
        assert_eq!(round_px(0.0), 0);
        assert_eq!(round_px(0.49), 0);
        assert_eq!(round_px(0.5), 1);
        assert_eq!(round_px(2.6), 3);
        assert_eq!(round_px(-0.4), 0);
        assert_eq!(round_px(-0.5), 0);
        assert_eq!(round_px(-0.6), -1);
        assert_eq!(round_px(-2.5), -2);
        assert_eq!(round_px(f32::NAN), 0);
    }

    #[test]
    fn a_filled_rect_covers_its_pixels_with_exclusive_right_and_bottom() {
        let op = rect_op(0.0, 0.0, 10.0, 4.0, 0.0, &paint(STYLE_FILL, 0.0)).unwrap();
        assert_eq!((op.x1, op.y1, op.x2, op.y2), (0, 0, 9, 3));
        assert_eq!((op.fill, op.stroke_width), (0xFF11_2233, 0));
    }

    #[test]
    fn an_empty_or_inverted_rect_draws_nothing() {
        assert_eq!(
            rect_op(5.0, 0.0, 5.0, 4.0, 0.0, &paint(STYLE_FILL, 0.0)),
            None
        );
        assert_eq!(
            rect_op(5.0, 4.0, 1.0, 0.0, 0.0, &paint(STYLE_FILL, 0.0)),
            None
        );
    }

    #[test]
    fn a_stroke_straddles_the_outline() {
        let op = rect_op(10.0, 10.0, 20.0, 20.0, 0.0, &paint(STYLE_STROKE, 4.0)).unwrap();
        assert_eq!((op.x1, op.y1, op.x2, op.y2), (8, 8, 21, 21));
        assert_eq!((op.fill, op.stroke, op.stroke_width), (0, 0xFF11_2233, 4));
    }

    #[test]
    fn a_hairline_stroke_is_one_pixel() {
        let op = rect_op(0.0, 0.0, 10.0, 10.0, 0.0, &paint(STYLE_STROKE, 0.0)).unwrap();
        assert_eq!(op.stroke_width, 1);
    }

    #[test]
    fn fill_and_stroke_is_one_grown_fill() {
        let op = rect_op(
            10.0,
            10.0,
            20.0,
            20.0,
            3.0,
            &paint(STYLE_FILL_AND_STROKE, 2.0),
        )
        .unwrap();
        assert_eq!((op.x1, op.y1, op.x2, op.y2), (9, 9, 20, 20));
        assert_eq!((op.fill, op.stroke_width, op.radius), (0xFF11_2233, 0, 4));
    }

    #[test]
    fn a_circle_is_a_square_with_a_radius_of_half_its_side() {
        // Canvas.drawCircle(20, 20, 5) arrives as a rect of radius 5.
        let op = rect_op(15.0, 15.0, 25.0, 25.0, 5.0, &paint(STYLE_FILL, 0.0)).unwrap();
        assert_eq!((op.x1, op.y1, op.x2, op.y2, op.radius), (15, 15, 24, 24, 5));
    }

    #[test]
    fn arc_angles_normalise_the_start_and_keep_the_end_after_it() {
        assert_eq!(arc_angles(0.0, 90.0), Some((0, 90)));
        assert_eq!(arc_angles(135.0, 270.0), Some((135, 405)));
        assert_eq!(arc_angles(-90.0, 90.0), Some((270, 360)));
        assert_eq!(arc_angles(450.0, 10.0), Some((90, 100)));
    }

    #[test]
    fn a_negative_sweep_is_the_same_arc_started_earlier() {
        assert_eq!(arc_angles(90.0, -90.0), Some((0, 90)));
        assert_eq!(arc_angles(0.0, -45.0), Some((315, 360)));
    }

    #[test]
    fn a_full_turn_is_start_plus_360_and_a_zero_sweep_is_nothing() {
        assert_eq!(arc_angles(30.0, 360.0), Some((30, 390)));
        assert_eq!(arc_angles(30.0, -720.0), Some((30, 390)));
        assert_eq!(arc_angles(30.0, 0.2), None);
    }

    #[test]
    fn a_filled_arc_is_a_wedge_of_the_inscribed_circle() {
        let op = arc_op(0.0, 0.0, 40.0, 30.0, 0.0, 90.0, &paint(STYLE_FILL, 0.0)).unwrap();
        assert_eq!((op.cx, op.cy, op.radius, op.width), (20, 15, 15, 15));
        assert!(!op.rounded);
    }

    #[test]
    fn a_stroked_arc_straddles_the_circle_and_takes_the_cap() {
        let mut p = paint(STYLE_STROKE, 6.0);
        p.cap = CAP_ROUND;
        let op = arc_op(0.0, 0.0, 100.0, 100.0, 135.0, 270.0, &p).unwrap();
        assert_eq!((op.cx, op.cy, op.radius, op.width), (50, 50, 53, 6));
        assert_eq!((op.start, op.end), (135, 405));
        assert!(op.rounded);
    }

    #[test]
    fn text_align_maps_paint_ordinals_to_lvgl() {
        assert_eq!(text_align(0), i32::from(LV_TEXT_ALIGN_LEFT));
        assert_eq!(text_align(1), i32::from(LV_TEXT_ALIGN_CENTER));
        assert_eq!(text_align(2), i32::from(LV_TEXT_ALIGN_RIGHT));
        assert_eq!(text_align(9), i32::from(LV_TEXT_ALIGN_LEFT));
    }
}
