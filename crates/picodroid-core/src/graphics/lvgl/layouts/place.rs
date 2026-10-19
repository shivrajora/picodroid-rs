// SPDX-License-Identifier: GPL-3.0-only
//! What the layout solvers work on: a child's measured box and margins, a
//! rectangle to put it in, and the three places a gravity can name on one
//! axis. Pure; the LVGL adapters in `mod.rs` fill these in and apply them.

use super::super::widgets::gravity;

/// A child as the solver sees it: its measured size, whether each axis is
/// `match_parent` (the size intent `setSize` recorded, since a stretched
/// child's style width no longer says), its margins, and whether it is GONE.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kid {
    pub w: i32,
    pub h: i32,
    pub match_w: bool,
    pub match_h: bool,
    /// Left, top, right, bottom.
    pub margin: [i32; 4],
    pub gone: bool,
}

impl Kid {
    pub const fn sized(w: i32, h: i32) -> Self {
        Self {
            w,
            h,
            match_w: false,
            match_h: false,
            margin: [0; 4],
            gone: false,
        }
    }

    pub const fn with_margins(mut self, left: i32, top: i32, right: i32, bottom: i32) -> Self {
        self.margin = [left, top, right, bottom];
        self
    }

    pub const fn matching(mut self, w: bool, h: bool) -> Self {
        self.match_w = w;
        self.match_h = h;
        self
    }
}

/// Where a child goes, in its parent's content area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// One axis of a gravity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Start,
    Center,
    End,
    /// Android's `FILL` on that axis: the child is stretched to it.
    Fill,
}

/// What one axis of an Android gravity says, `None` when it says nothing
/// (`Gravity.RIGHT` alone leaves the vertical axis to its default).
pub fn gravity_axis(gravity_bits: i32, vertical: bool) -> Option<Place> {
    match gravity::axis_place(gravity_bits, vertical) {
        gravity::AxisPlace::Unset => None,
        gravity::AxisPlace::Start => Some(Place::Start),
        gravity::AxisPlace::Center => Some(Place::Center),
        gravity::AxisPlace::End => Some(Place::End),
        gravity::AxisPlace::Fill => Some(Place::Fill),
    }
}

/// The start of a `size`-long child placed by `place` in a `span`-long
/// axis, as Android's `Gravity.apply` computes it (integer division, the
/// odd pixel to the end).
pub fn place_axis(place: Place, span: i32, size: i32) -> i32 {
    match place {
        Place::Start | Place::Fill => 0,
        Place::Center => (span - size) / 2,
        Place::End => span - size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_axis_follows_android_gravity_apply() {
        assert_eq!(place_axis(Place::Start, 100, 30), 0);
        assert_eq!(place_axis(Place::Center, 100, 30), 35);
        assert_eq!(place_axis(Place::Center, 101, 30), 35);
        assert_eq!(place_axis(Place::End, 100, 30), 70);
        assert_eq!(place_axis(Place::Fill, 100, 30), 0);
    }

    #[test]
    fn gravity_axis_reads_one_axis_and_folds_start_end() {
        const LEFT: i32 = 0x03;
        const RIGHT: i32 = 0x05;
        const TOP: i32 = 0x30;
        const BOTTOM: i32 = 0x50;
        const CENTER_H: i32 = 0x01;
        const CENTER_V: i32 = 0x10;
        const FILL_H: i32 = 0x07;
        const END: i32 = 0x0080_0005;
        assert_eq!(gravity_axis(0, false), None);
        assert_eq!(gravity_axis(LEFT, false), Some(Place::Start));
        assert_eq!(gravity_axis(RIGHT, false), Some(Place::End));
        assert_eq!(gravity_axis(END, false), Some(Place::End));
        assert_eq!(gravity_axis(RIGHT, true), None);
        assert_eq!(gravity_axis(TOP | RIGHT, true), Some(Place::Start));
        assert_eq!(gravity_axis(BOTTOM, true), Some(Place::End));
        assert_eq!(
            gravity_axis(CENTER_H | CENTER_V, false),
            Some(Place::Center)
        );
        assert_eq!(gravity_axis(CENTER_H | CENTER_V, true), Some(Place::Center));
        assert_eq!(gravity_axis(FILL_H, false), Some(Place::Fill));
    }
}
