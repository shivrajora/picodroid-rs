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

/// `TextView.setGravity` as LVGL sees it: the horizontal field of a gravity as the label's
/// `text_align`. `LEFT` / `START` and an unspecified axis read as LEFT — Android's default for a
/// `TextView` is `TOP | START` — `CENTER_HORIZONTAL` as CENTER, `RIGHT` / `END` as RIGHT, and
/// `FILL_HORIZONTAL`, which sets both pulls, as LEFT (a label cannot stretch its text). The
/// vertical field is not read: an `lv_label` is its text's height and has nowhere to move it.
pub(crate) fn text_align(gravity: i32) -> lv_text_align_t {
    let horizontal = gravity & AXIS_MASK;
    if horizontal & AXIS_PULL_BEFORE != 0 {
        LV_TEXT_ALIGN_LEFT
    } else if horizontal & AXIS_PULL_AFTER != 0 {
        LV_TEXT_ALIGN_RIGHT
    } else if horizontal & AXIS_SPECIFIED != 0 {
        LV_TEXT_ALIGN_CENTER
    } else {
        LV_TEXT_ALIGN_LEFT
    }
}

/// One axis of a gravity, as the layout solvers (`graphics/lvgl/layouts`) read it: `FILL` (both
/// pulls) is its own place there, since a solver can stretch a child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AxisPlace {
    Unset,
    Start,
    Center,
    End,
    Fill,
}

pub(crate) fn axis_place(gravity: i32, vertical: bool) -> AxisPlace {
    let field = if vertical {
        (gravity >> VERTICAL_SHIFT) & AXIS_MASK
    } else {
        gravity & AXIS_MASK
    };
    let (before, after) = (field & AXIS_PULL_BEFORE != 0, field & AXIS_PULL_AFTER != 0);
    match (before, after) {
        (true, true) => AxisPlace::Fill,
        (true, false) => AxisPlace::Start,
        (false, true) => AxisPlace::End,
        (false, false) if field & AXIS_SPECIFIED != 0 => AxisPlace::Center,
        (false, false) => AxisPlace::Unset,
    }
}

/// Where a `FrameLayout` child's `layout_gravity` puts it, as the `lv_align_t` its position is
/// then an offset from. Each axis reads as it does for a `TextView`: the start unless the gravity
/// pulls to the end or names the centre, which is Android's default of `TOP | START` for a
/// gravity that says nothing, and `FILL`, which cannot stretch the child, as the start.
pub(crate) fn frame_align(gravity: i32) -> u8 {
    /// 0 start, 1 centre, 2 end.
    fn place(field: i32) -> usize {
        if field & AXIS_PULL_BEFORE != 0 {
            0
        } else if field & AXIS_PULL_AFTER != 0 {
            2
        } else if field & AXIS_SPECIFIED != 0 {
            1
        } else {
            0
        }
    }
    const ALIGN: [[u8; 3]; 3] = [
        [LV_ALIGN_TOP_LEFT, LV_ALIGN_TOP_MID, LV_ALIGN_TOP_RIGHT],
        [LV_ALIGN_LEFT_MID, LV_ALIGN_CENTER, LV_ALIGN_RIGHT_MID],
        [
            LV_ALIGN_BOTTOM_LEFT,
            LV_ALIGN_BOTTOM_MID,
            LV_ALIGN_BOTTOM_RIGHT,
        ],
    ];
    let horizontal = place(gravity & AXIS_MASK);
    let vertical = place((gravity >> VERTICAL_SHIFT) & AXIS_MASK);
    ALIGN[vertical][horizontal]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_align_names_the_corner_edge_or_centre() {
        assert_eq!(frame_align(NO_GRAVITY), LV_ALIGN_TOP_LEFT);
        assert_eq!(frame_align(LEFT | TOP), LV_ALIGN_TOP_LEFT);
        assert_eq!(frame_align(START), LV_ALIGN_TOP_LEFT);
        assert_eq!(frame_align(RIGHT), LV_ALIGN_TOP_RIGHT);
        assert_eq!(frame_align(END | BOTTOM), LV_ALIGN_BOTTOM_RIGHT);
        assert_eq!(frame_align(BOTTOM), LV_ALIGN_BOTTOM_LEFT);
        assert_eq!(frame_align(CENTER_HORIZONTAL), LV_ALIGN_TOP_MID);
        assert_eq!(frame_align(CENTER_VERTICAL), LV_ALIGN_LEFT_MID);
        assert_eq!(frame_align(CENTER), LV_ALIGN_CENTER);
        assert_eq!(frame_align(CENTER_HORIZONTAL | BOTTOM), LV_ALIGN_BOTTOM_MID);
        assert_eq!(frame_align(RIGHT | CENTER_VERTICAL), LV_ALIGN_RIGHT_MID);
        // FILL sets both pulls on an axis: the start, since a child cannot be stretched.
        assert_eq!(frame_align(FILL_VERTICAL | 0x07), LV_ALIGN_TOP_LEFT);
    }

    #[test]
    fn text_align_reads_the_horizontal_field_only() {
        assert_eq!(text_align(LEFT), LV_TEXT_ALIGN_LEFT);
        assert_eq!(text_align(START), LV_TEXT_ALIGN_LEFT);
        assert_eq!(text_align(RIGHT), LV_TEXT_ALIGN_RIGHT);
        assert_eq!(text_align(END), LV_TEXT_ALIGN_RIGHT);
        assert_eq!(text_align(CENTER_HORIZONTAL), LV_TEXT_ALIGN_CENTER);
        assert_eq!(text_align(CENTER), LV_TEXT_ALIGN_CENTER);
        // RIGHT shares the SPECIFIED bit with LEFT: the pull bits decide, not a mask test.
        assert_eq!(text_align(RIGHT | CENTER_VERTICAL), LV_TEXT_ALIGN_RIGHT);
        // The vertical field alone, or nothing, leaves the text where Android's default puts it.
        assert_eq!(text_align(BOTTOM), LV_TEXT_ALIGN_LEFT);
        assert_eq!(text_align(NO_GRAVITY), LV_TEXT_ALIGN_LEFT);
        // FILL_HORIZONTAL sets both pulls; a label cannot stretch, so it reads as the start.
        assert_eq!(text_align(FILL_VERTICAL | 0x07), LV_TEXT_ALIGN_LEFT);
    }

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

    #[test]
    fn axis_place_reads_one_axis_with_fill_of_its_own() {
        assert_eq!(axis_place(NO_GRAVITY, false), AxisPlace::Unset);
        assert_eq!(axis_place(BOTTOM, false), AxisPlace::Unset);
        assert_eq!(axis_place(BOTTOM, true), AxisPlace::End);
        assert_eq!(axis_place(START, false), AxisPlace::Start);
        assert_eq!(axis_place(END, false), AxisPlace::End);
        assert_eq!(axis_place(CENTER, true), AxisPlace::Center);
        assert_eq!(axis_place(FILL_VERTICAL, true), AxisPlace::Fill);
        assert_eq!(axis_place(TOP | RIGHT, true), AxisPlace::Start);
        assert_eq!(axis_place(TOP | RIGHT, false), AxisPlace::End);
    }
}
