// SPDX-License-Identifier: GPL-3.0-only
//! `LinearLayout.setGravity` as LVGL sees it: an Android gravity bitmask decoded into the flex
//! alignment of the layout's main and cross axes. Pure, so it is unit-tested through the shim in
//! `lib.rs` although the graphics tree is `cfg(not(test))`; `linear_layout.rs` applies the result.

use crate::lvgl_ffi::*;

/// Android packs a gravity into two three-bit fields: horizontal in bits 0-2, vertical in bits
/// 4-6. Each field holds `AXIS_SPECIFIED` (1), `AXIS_PULL_BEFORE` (2) and `AXIS_PULL_AFTER` (4),
/// so `LEFT` is `0x03` and `RIGHT` is `0x05` — the two ends of an axis share the SPECIFIED bit,
/// and only the pull bits tell them apart. A mask test against `LEFT` therefore matches `RIGHT`
/// too, and one against `TOP` matches `BOTTOM`; the pulls have to be read on their own.
const AXIS_SPECIFIED: i32 = 0x1;
const AXIS_PULL_BEFORE: i32 = 0x2;
const AXIS_PULL_AFTER: i32 = 0x4;

/// The three-bit field width, and the shift that reaches the vertical one.
const AXIS_MASK: i32 = 0x7;
const VERTICAL_SHIFT: i32 = 4;

/// Where one axis of a gravity places its children, or `None` when that axis says nothing —
/// `Gravity.RIGHT` alone leaves the vertical axis unspecified.
///
/// `FILL` sets both pull bits. Android stretches the child to the axis; LVGL's flex alignment
/// cannot, so it reads as the start of the axis, which is where a stretched child begins.
fn axis_align(field: i32) -> Option<lv_flex_align_t> {
    if field & AXIS_PULL_BEFORE != 0 {
        Some(LV_FLEX_ALIGN_START)
    } else if field & AXIS_PULL_AFTER != 0 {
        Some(LV_FLEX_ALIGN_END)
    } else if field & AXIS_SPECIFIED != 0 {
        // SPECIFIED with neither pull is CENTER_HORIZONTAL / CENTER_VERTICAL.
        Some(LV_FLEX_ALIGN_CENTER)
    } else {
        None
    }
}

/// The `(main, cross)` flex alignment for a gravity on a layout whose flow is a column when
/// `vertical_flow`, a row otherwise. Which Android axis is the main one follows the flow: the
/// vertical bits drive a column, the horizontal bits a row.
///
/// An axis the gravity leaves out keeps the default the layout was created with — the start of
/// the main axis, as an Android `LinearLayout` defaults to `START | TOP`, and the centre of the
/// cross axis, which is a picodroid divergence that predates per-axis routing and that no
/// caller has had a way to ask for anything else.
///
/// `Gravity.START` and `Gravity.END` carry a relative-direction bit above the two fields; masking
/// each field drops it, so they land on `LEFT` and `RIGHT` as they do in a left-to-right locale.
pub(crate) fn flex_align(gravity: i32, vertical_flow: bool) -> (lv_flex_align_t, lv_flex_align_t) {
    let horizontal = gravity & AXIS_MASK;
    let vertical = (gravity >> VERTICAL_SHIFT) & AXIS_MASK;
    let (main, cross) = if vertical_flow {
        (vertical, horizontal)
    } else {
        (horizontal, vertical)
    };
    (
        axis_align(main).unwrap_or(LV_FLEX_ALIGN_START),
        axis_align(cross).unwrap_or(LV_FLEX_ALIGN_CENTER),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Gravity constants, spelled as sdk/java/picodroid/view/Gravity.java has them.
    const TOP: i32 = 0x30;
    const BOTTOM: i32 = 0x50;
    const LEFT: i32 = 0x03;
    const RIGHT: i32 = 0x05;
    const CENTER_VERTICAL: i32 = 0x10;
    const CENTER_HORIZONTAL: i32 = 0x01;
    const CENTER: i32 = CENTER_VERTICAL | CENTER_HORIZONTAL;
    const FILL_VERTICAL: i32 = 0x70;
    const START: i32 = 0x0080_0003;
    const END: i32 = 0x0080_0005;
    const NO_GRAVITY: i32 = 0;

    const ROW: bool = false;
    const COLUMN: bool = true;

    #[test]
    fn the_far_end_of_an_axis_is_not_read_as_the_near_one() {
        // The bug this function exists to rule out: RIGHT shares the SPECIFIED
        // bit with LEFT, and BOTTOM with TOP, so a mask test reads both ends as
        // the start and no layout can ever right-align.
        assert_eq!(flex_align(RIGHT, ROW).0, LV_FLEX_ALIGN_END);
        assert_eq!(flex_align(BOTTOM, COLUMN).0, LV_FLEX_ALIGN_END);
        assert_eq!(flex_align(LEFT, ROW).0, LV_FLEX_ALIGN_START);
        assert_eq!(flex_align(TOP, COLUMN).0, LV_FLEX_ALIGN_START);
    }

    #[test]
    fn the_main_axis_follows_the_flow() {
        // A vertical gravity says nothing about a row's main axis, and a
        // horizontal one nothing about a column's.
        assert_eq!(flex_align(BOTTOM, ROW).0, LV_FLEX_ALIGN_START);
        assert_eq!(flex_align(RIGHT, COLUMN).0, LV_FLEX_ALIGN_START);
        // It does reach the cross axis, which is the other half of the pair.
        assert_eq!(flex_align(BOTTOM, ROW).1, LV_FLEX_ALIGN_END);
        assert_eq!(flex_align(RIGHT, COLUMN).1, LV_FLEX_ALIGN_END);
    }

    #[test]
    fn centre_reaches_both_axes() {
        assert_eq!(
            flex_align(CENTER, COLUMN),
            (LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER)
        );
        assert_eq!(
            flex_align(CENTER, ROW),
            (LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER)
        );
        assert_eq!(flex_align(CENTER_VERTICAL, COLUMN).0, LV_FLEX_ALIGN_CENTER);
        assert_eq!(flex_align(CENTER_HORIZONTAL, ROW).0, LV_FLEX_ALIGN_CENTER);
    }

    #[test]
    fn an_unspecified_axis_keeps_the_layouts_default() {
        assert_eq!(
            flex_align(NO_GRAVITY, COLUMN),
            (LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_CENTER)
        );
        // Only the main axis named: the cross keeps centring, as before.
        assert_eq!(flex_align(BOTTOM, COLUMN).1, LV_FLEX_ALIGN_CENTER);
    }

    #[test]
    fn relative_start_and_end_land_on_left_and_right() {
        assert_eq!(flex_align(START, ROW).0, LV_FLEX_ALIGN_START);
        assert_eq!(flex_align(END, ROW).0, LV_FLEX_ALIGN_END);
    }

    #[test]
    fn fill_degrades_to_the_start_of_the_axis() {
        assert_eq!(flex_align(FILL_VERTICAL, COLUMN).0, LV_FLEX_ALIGN_START);
    }

    #[test]
    fn a_gravity_naming_both_axes_places_both() {
        assert_eq!(
            flex_align(BOTTOM | RIGHT, COLUMN),
            (LV_FLEX_ALIGN_END, LV_FLEX_ALIGN_END)
        );
        assert_eq!(
            flex_align(TOP | RIGHT, ROW),
            (LV_FLEX_ALIGN_END, LV_FLEX_ALIGN_START)
        );
    }
}
