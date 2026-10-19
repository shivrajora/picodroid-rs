// SPDX-License-Identifier: GPL-3.0-only
//! `RelativeLayout`'s solver: Android's `onMeasure` arithmetic, pure.
//!
//! Each child carries rules against its parent's edges or centre, or against
//! a sibling's edges. The two axes are independent and each is solved in
//! dependency order: a child whose anchors are placed is placed, passes
//! repeat until nothing is left, and a pass that places nothing with
//! children still waiting is a cycle, which Android refuses and this solver
//! resolves by placing the rest as if unanchored. A `wrap_content` axis is
//! Android's two steps: the children are placed with the rules that need the
//! far edge or the centre switched off, the extent is read, and they are
//! placed again against it.

use super::place::{place_axis, Kid, Place, Rect};

/// The most children one `RelativeLayout` lays out. Past it, the rest are
/// placed as unanchored (and the adapter says so once).
pub const MAX_KIDS: usize = 20;

// `Rules::flags` bits: the boolean verbs.
pub const F_PARENT_LEFT: u16 = 1 << 0;
pub const F_PARENT_TOP: u16 = 1 << 1;
pub const F_PARENT_RIGHT: u16 = 1 << 2;
pub const F_PARENT_BOTTOM: u16 = 1 << 3;
pub const F_CENTER_IN_PARENT: u16 = 1 << 4;
pub const F_CENTER_HORIZONTAL: u16 = 1 << 5;
pub const F_CENTER_VERTICAL: u16 = 1 << 6;
/// `alignWithParentIfMissing`: a sibling rule whose sibling is missing
/// becomes the parent-edge rule of the same side.
pub const F_IF_MISSING: u16 = 1 << 7;

// `Rules::anchors` slots: the verbs that name a sibling.
pub const A_ABOVE: usize = 0;
pub const A_BELOW: usize = 1;
pub const A_TO_LEFT_OF: usize = 2;
pub const A_TO_RIGHT_OF: usize = 3;
pub const A_ALIGN_LEFT: usize = 4;
pub const A_ALIGN_TOP: usize = 5;
pub const A_ALIGN_RIGHT: usize = 6;
pub const A_ALIGN_BOTTOM: usize = 7;
pub const ANCHORS: usize = 8;

/// What a sibling rule points at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    /// No such rule.
    #[default]
    None,
    /// A rule whose id names no sibling (or a GONE one): ignored, unless
    /// `F_IF_MISSING` turns it into the parent-edge rule.
    Missing,
    /// The sibling at this index.
    Sibling(u8),
}

/// One child's rules, with the anchors already resolved to sibling indices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    pub flags: u16,
    pub anchors: [Anchor; ANCHORS],
}

impl Rules {
    pub const fn flags(flags: u16) -> Self {
        Self {
            flags,
            anchors: [Anchor::None; ANCHORS],
        }
    }

    pub const fn anchor(mut self, slot: usize, to: Anchor) -> Self {
        self.anchors[slot] = to;
        self
    }
}

/// The container's content size per axis, `None` for `wrap_content`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub w: Option<i32>,
    pub h: Option<i32>,
}

/// What `solve` found out: the extent the children need on each axis
/// (what a `wrap_content` container becomes) and whether a cycle was broken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Solved {
    pub w: i32,
    pub h: i32,
    pub cycle: bool,
}

/// The rules of one axis, by slot and bit.
struct Axis {
    /// `toRightOf` / `below`: this child starts after the sibling's end.
    after: usize,
    /// `toLeftOf` / `above`: this child ends before the sibling's start.
    before: usize,
    align_start: usize,
    align_end: usize,
    parent_start: u16,
    parent_end: u16,
    center: u16,
    horizontal: bool,
}

const HORIZONTAL: Axis = Axis {
    after: A_TO_RIGHT_OF,
    before: A_TO_LEFT_OF,
    align_start: A_ALIGN_LEFT,
    align_end: A_ALIGN_RIGHT,
    parent_start: F_PARENT_LEFT,
    parent_end: F_PARENT_RIGHT,
    center: F_CENTER_HORIZONTAL,
    horizontal: true,
};

const VERTICAL: Axis = Axis {
    after: A_BELOW,
    before: A_ABOVE,
    align_start: A_ALIGN_TOP,
    align_end: A_ALIGN_BOTTOM,
    parent_start: F_PARENT_TOP,
    parent_end: F_PARENT_BOTTOM,
    center: F_CENTER_VERTICAL,
    horizontal: false,
};

impl Axis {
    fn size(&self, k: &Kid) -> i32 {
        if self.horizontal {
            k.w
        } else {
            k.h
        }
    }

    fn matches(&self, k: &Kid) -> bool {
        if self.horizontal {
            k.match_w
        } else {
            k.match_h
        }
    }

    fn margin_start(&self, k: &Kid) -> i32 {
        if self.horizontal {
            k.margin[0]
        } else {
            k.margin[1]
        }
    }

    fn margin_end(&self, k: &Kid) -> i32 {
        if self.horizontal {
            k.margin[2]
        } else {
            k.margin[3]
        }
    }
}

/// Place `kids` by `rules` in a container of `span` with `gravity`, writing
/// each child's rectangle to `out`. `rules` and `out` are as long as `kids`,
/// at most [`MAX_KIDS`] of them.
pub fn solve(
    span: Span,
    gravity: (Option<Place>, Option<Place>),
    kids: &[Kid],
    rules: &[Rules],
    out: &mut [Rect],
) -> Solved {
    let n = kids.len().min(rules.len()).min(out.len()).min(MAX_KIDS);
    let (kids, rules, out) = (&kids[..n], &rules[..n], &mut out[..n]);
    let mut pos = [0i32; MAX_KIDS];
    let mut size = [0i32; MAX_KIDS];
    let mut solved = Solved::default();

    for axis in [&HORIZONTAL, &VERTICAL] {
        let given = if axis.horizontal { span.w } else { span.h };
        let known = match given {
            Some(s) => s,
            None => {
                // Android's `wrap_content` first step: without a far edge,
                // then measured.
                solved.cycle |= solve_axis(axis, None, kids, rules, &mut pos, &mut size);
                extent(axis, kids, &pos, &size)
            }
        };
        solved.cycle |= solve_axis(axis, Some(known), kids, rules, &mut pos, &mut size);
        let place = if axis.horizontal {
            gravity.0
        } else {
            gravity.1
        };
        if let Some(place) = place.filter(|_| given.is_some()) {
            shift_group(axis, place, known, kids, &mut pos, &size);
        }
        if axis.horizontal {
            solved.w = known;
            for (i, r) in out.iter_mut().enumerate() {
                r.x = pos[i];
                r.w = size[i];
            }
        } else {
            solved.h = known;
            for (i, r) in out.iter_mut().enumerate() {
                r.y = pos[i];
                r.h = size[i];
            }
        }
    }
    solved
}

/// The far edge of the children on `axis`, margins included: what a
/// `wrap_content` container is on that axis. A `match_parent` child has no
/// extent of its own.
fn extent(axis: &Axis, kids: &[Kid], pos: &[i32], size: &[i32]) -> i32 {
    let mut end = 0;
    for (i, k) in kids.iter().enumerate() {
        if k.gone || axis.matches(k) {
            continue;
        }
        end = end.max(pos[i] + size[i] + axis.margin_end(k));
    }
    end
}

/// Android's `setGravity`: the children's bounding box, margins included,
/// is moved as one to where `place` says.
fn shift_group(axis: &Axis, place: Place, span: i32, kids: &[Kid], pos: &mut [i32], size: &[i32]) {
    let mut start = i32::MAX;
    let mut end = i32::MIN;
    for (i, k) in kids.iter().enumerate() {
        if k.gone {
            continue;
        }
        start = start.min(pos[i] - axis.margin_start(k));
        end = end.max(pos[i] + size[i] + axis.margin_end(k));
    }
    if start > end {
        return;
    }
    let delta = place_axis(place, span, end - start) - start;
    if delta == 0 {
        return;
    }
    for (i, k) in kids.iter().enumerate() {
        if !k.gone {
            pos[i] += delta;
        }
    }
}

/// One axis, in dependency order. Returns whether a cycle was broken.
fn solve_axis(
    axis: &Axis,
    span: Option<i32>,
    kids: &[Kid],
    rules: &[Rules],
    pos: &mut [i32],
    size: &mut [i32],
) -> bool {
    let n = kids.len();
    let mut done = [false; MAX_KIDS];
    let mut cycle = false;
    loop {
        let mut progress = false;
        let mut waiting = false;
        for i in 0..n {
            if done[i] {
                continue;
            }
            if kids[i].gone {
                pos[i] = 0;
                size[i] = 0;
                done[i] = true;
                continue;
            }
            if !anchors_placed(axis, &rules[i], &done) {
                waiting = true;
                continue;
            }
            place_child(axis, span, i, kids, rules, pos, size, &done, false);
            done[i] = true;
            progress = true;
        }
        if !waiting {
            return cycle;
        }
        if !progress {
            // Every child left waits on another that waits: a cycle. Place
            // them as if the sibling rules were not there.
            cycle = true;
            for i in 0..n {
                if !done[i] {
                    place_child(axis, span, i, kids, rules, pos, size, &done, true);
                    done[i] = true;
                }
            }
            return cycle;
        }
    }
}

fn anchors_placed(axis: &Axis, rules: &Rules, done: &[bool]) -> bool {
    [axis.after, axis.before, axis.align_start, axis.align_end]
        .into_iter()
        .all(|slot| match rules.anchors[slot] {
            Anchor::Sibling(j) => done.get(j as usize).copied().unwrap_or(true),
            _ => true,
        })
}

/// Android's `applyHorizontalSizeRules` + `getChildMeasureSpec` +
/// `positionChildHorizontal` for one child on one axis (and the vertical
/// twins). With `unanchored`, a sibling anchor reads as missing.
#[allow(clippy::too_many_arguments)]
fn place_child(
    axis: &Axis,
    span: Option<i32>,
    i: usize,
    kids: &[Kid],
    rules: &[Rules],
    pos: &mut [i32],
    size: &mut [i32],
    done: &[bool],
    unanchored: bool,
) {
    let k = &kids[i];
    let r = &rules[i];
    let m_start = axis.margin_start(k);
    let m_end = axis.margin_end(k);
    let if_missing = r.flags & F_IF_MISSING != 0;
    // The parent's edges, as a rule against them gives: the far one only
    // when the span is known (Android skips it in the wrap_content step).
    let parent_start = Some(m_start);
    let parent_end = span.map(|s| s - m_end);
    // A sibling that is placed on this axis, else the rule is missing.
    let sibling = |slot: usize| -> Option<usize> {
        match r.anchors[slot] {
            Anchor::Sibling(j) if !unanchored => {
                let j = j as usize;
                (j < kids.len() && j != i && done[j] && !kids[j].gone).then_some(j)
            }
            _ => None,
        }
    };
    let missing =
        |slot: usize| -> bool { r.anchors[slot] != Anchor::None && sibling(slot).is_none() };

    let mut start: Option<i32> = None;
    let mut end: Option<i32> = None;
    // In Android's order: the sibling rules, then the parent's edges, each
    // later one overriding an earlier one on the same edge.
    if let Some(j) = sibling(axis.before) {
        end = Some(pos[j] - axis.margin_start(&kids[j]) - m_end);
    } else if missing(axis.before) && if_missing {
        end = parent_end;
    }
    if let Some(j) = sibling(axis.after) {
        start = Some(pos[j] + size[j] + axis.margin_end(&kids[j]) + m_start);
    } else if missing(axis.after) && if_missing {
        start = parent_start;
    }
    if let Some(j) = sibling(axis.align_start) {
        start = Some(pos[j] + m_start);
    } else if missing(axis.align_start) && if_missing {
        start = parent_start;
    }
    if let Some(j) = sibling(axis.align_end) {
        end = Some(pos[j] + size[j] - m_end);
    } else if missing(axis.align_end) && if_missing {
        end = parent_end;
    }
    if r.flags & axis.parent_start != 0 {
        start = parent_start;
    }
    if r.flags & axis.parent_end != 0 && span.is_some() {
        end = parent_end;
    }

    let own = axis.size(k);
    // `match_parent` fills from its start edge (or the parent's) to its end
    // edge (or the parent's); on a wrap_content axis it keeps its size.
    let wanted = match span {
        Some(s) if axis.matches(k) => {
            let from = start.unwrap_or(m_start);
            let to = end.unwrap_or(s - m_end);
            (to - from).max(0)
        }
        _ => own,
    };
    let centred = r.flags & (F_CENTER_IN_PARENT | axis.center) != 0;
    let (p, sz) = match (start, end) {
        // Both edges given: the child is exactly that wide, whatever it
        // asked for (Android measures it EXACTLY).
        (Some(s), Some(e)) => (s, (e - s).max(0)),
        (Some(s), None) => (s, wanted),
        (None, Some(e)) => (e - wanted, wanted),
        (None, None) => match span {
            Some(s) if centred => (place_axis(Place::Center, s, wanted), wanted),
            _ => (m_start, wanted),
        },
    };
    pos[i] = p;
    size[i] = sz;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rects(n: usize) -> [Rect; MAX_KIDS] {
        let _ = n;
        [Rect::default(); MAX_KIDS]
    }

    fn fixed(w: i32, h: i32) -> Span {
        Span {
            w: Some(w),
            h: Some(h),
        }
    }

    const NONE: (Option<Place>, Option<Place>) = (None, None);

    #[test]
    fn unruled_children_sit_at_the_top_left_corner_by_their_margins() {
        let kids = [
            Kid::sized(10, 10),
            Kid::sized(20, 5).with_margins(3, 4, 0, 0),
        ];
        let rules = [Rules::default(); 2];
        let mut out = rects(2);
        let s = solve(fixed(100, 50), NONE, &kids, &rules, &mut out);
        assert_eq!(
            out[0],
            Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10
            }
        );
        assert_eq!(
            out[1],
            Rect {
                x: 3,
                y: 4,
                w: 20,
                h: 5
            }
        );
        assert_eq!((s.w, s.h, s.cycle), (100, 50, false));
    }

    #[test]
    fn picoclock_header_a_caption_right_of_the_back_button_centred_on_it() {
        // `back` 48x30 at the parent's left; `caption` 60x16 to its right,
        // centred vertically in the 30-tall header.
        let kids = [Kid::sized(48, 30), Kid::sized(60, 16)];
        let rules = [
            Rules::flags(F_PARENT_LEFT),
            Rules::flags(F_CENTER_VERTICAL).anchor(A_TO_RIGHT_OF, Anchor::Sibling(0)),
        ];
        let mut out = rects(2);
        solve(fixed(320, 30), NONE, &kids, &rules, &mut out);
        assert_eq!(
            out[0],
            Rect {
                x: 0,
                y: 0,
                w: 48,
                h: 30
            }
        );
        assert_eq!(
            out[1],
            Rect {
                x: 48,
                y: 7,
                w: 60,
                h: 16
            }
        );
    }

    #[test]
    fn a_below_chain_centred_horizontally_and_a_bottom_row() {
        // The clock page: a face, three lines below it in turn, each centred;
        // a button pinned to the bottom edge with a margin.
        let kids = [
            Kid::sized(200, 60),
            Kid::sized(100, 16),
            Kid::sized(80, 16).with_margins(0, 4, 0, 0),
            Kid::sized(120, 30).with_margins(0, 0, 0, 6),
        ];
        let rules = [
            Rules::flags(F_CENTER_HORIZONTAL),
            Rules::flags(F_CENTER_HORIZONTAL).anchor(A_BELOW, Anchor::Sibling(0)),
            Rules::flags(F_CENTER_HORIZONTAL).anchor(A_BELOW, Anchor::Sibling(1)),
            Rules::flags(F_PARENT_BOTTOM | F_PARENT_LEFT),
        ];
        let mut out = rects(4);
        solve(fixed(320, 480), NONE, &kids, &rules, &mut out);
        assert_eq!(
            out[0],
            Rect {
                x: 60,
                y: 0,
                w: 200,
                h: 60
            }
        );
        assert_eq!(
            out[1],
            Rect {
                x: 110,
                y: 60,
                w: 100,
                h: 16
            }
        );
        assert_eq!(
            out[2],
            Rect {
                x: 120,
                y: 80,
                w: 80,
                h: 16
            }
        );
        assert_eq!(
            out[3],
            Rect {
                x: 0,
                y: 480 - 30 - 6,
                w: 120,
                h: 30
            }
        );
    }

    #[test]
    fn a_stepper_stretches_its_label_between_the_buttons() {
        // "-" at the left edge, "+" at the right, the value between them:
        // match_parent fills the gap, wrap_content is stretched too (both
        // edges are given, as Android's EXACTLY spec says).
        let kids = [
            Kid::sized(40, 30),
            Kid::sized(40, 30),
            Kid::sized(20, 16).matching(true, false),
            Kid::sized(20, 16),
        ];
        let rules = [
            Rules::flags(F_PARENT_LEFT),
            Rules::flags(F_PARENT_RIGHT),
            Rules::flags(F_CENTER_VERTICAL)
                .anchor(A_TO_RIGHT_OF, Anchor::Sibling(0))
                .anchor(A_TO_LEFT_OF, Anchor::Sibling(1)),
            Rules::flags(F_CENTER_VERTICAL)
                .anchor(A_TO_RIGHT_OF, Anchor::Sibling(0))
                .anchor(A_TO_LEFT_OF, Anchor::Sibling(1)),
        ];
        let mut out = rects(4);
        solve(fixed(200, 30), NONE, &kids, &rules, &mut out);
        assert_eq!(out[0].x, 0);
        assert_eq!(out[1].x, 160);
        assert_eq!(
            out[2],
            Rect {
                x: 40,
                y: 7,
                w: 120,
                h: 16
            }
        );
        assert_eq!(
            out[3],
            Rect {
                x: 40,
                y: 7,
                w: 120,
                h: 16
            }
        );
    }

    #[test]
    fn match_parent_fills_from_its_start_edge_to_the_parents_end() {
        let kids = [Kid::sized(30, 10), Kid::sized(1, 10).matching(true, false)];
        let rules = [
            Rules::default(),
            Rules::default().anchor(A_TO_RIGHT_OF, Anchor::Sibling(0)),
        ];
        let mut out = rects(2);
        solve(fixed(100, 10), NONE, &kids, &rules, &mut out);
        assert_eq!(
            out[1],
            Rect {
                x: 30,
                y: 0,
                w: 70,
                h: 10
            }
        );
    }

    #[test]
    fn a_forward_reference_waits_for_its_anchor() {
        // The child named first sits to the right of the one named second.
        let kids = [Kid::sized(10, 10), Kid::sized(30, 10)];
        let rules = [
            Rules::default().anchor(A_TO_RIGHT_OF, Anchor::Sibling(1)),
            Rules::flags(F_PARENT_LEFT),
        ];
        let mut out = rects(2);
        let s = solve(fixed(100, 10), NONE, &kids, &rules, &mut out);
        assert_eq!(out[0].x, 30);
        assert!(!s.cycle);
    }

    #[test]
    fn a_cycle_is_broken_and_reported() {
        let kids = [Kid::sized(10, 10), Kid::sized(10, 10)];
        let rules = [
            Rules::default().anchor(A_BELOW, Anchor::Sibling(1)),
            Rules::default().anchor(A_BELOW, Anchor::Sibling(0)),
        ];
        let mut out = rects(2);
        let s = solve(fixed(100, 100), NONE, &kids, &rules, &mut out);
        assert!(s.cycle);
        assert_eq!(
            out[0],
            Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10
            }
        );
        assert_eq!(
            out[1],
            Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10
            }
        );
    }

    #[test]
    fn a_missing_anchor_is_ignored_unless_align_with_parent_if_missing() {
        let kids = [Kid::sized(10, 10), Kid::sized(10, 10)];
        let rules = [
            Rules::default().anchor(A_TO_LEFT_OF, Anchor::Missing),
            Rules::flags(F_IF_MISSING).anchor(A_TO_LEFT_OF, Anchor::Missing),
        ];
        let mut out = rects(2);
        solve(fixed(100, 10), NONE, &kids, &rules, &mut out);
        assert_eq!(out[0].x, 0);
        assert_eq!(out[1].x, 90);
    }

    #[test]
    fn a_gone_child_takes_no_room_and_anchors_nothing() {
        let kids = [
            Kid {
                gone: true,
                ..Kid::sized(50, 50)
            },
            Kid::sized(10, 10),
        ];
        let rules = [
            Rules::default(),
            Rules::default().anchor(A_BELOW, Anchor::Missing),
        ];
        let mut out = rects(2);
        let s = solve(Span { w: None, h: None }, NONE, &kids, &rules, &mut out);
        assert_eq!(out[0], Rect::default());
        assert_eq!(
            out[1],
            Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10
            }
        );
        assert_eq!((s.w, s.h), (10, 10));
    }

    #[test]
    fn wrap_content_measures_then_places_against_the_measured_extent() {
        // Two children stacked; the second pinned to the bottom edge with a
        // margin: the extent is the first child's height plus the second's
        // box, and the far-edge rule is applied against that.
        let kids = [
            Kid::sized(40, 20),
            Kid::sized(30, 10).with_margins(0, 0, 0, 5),
        ];
        let rules = [
            Rules::default(),
            Rules::flags(F_PARENT_BOTTOM).anchor(A_BELOW, Anchor::Sibling(0)),
        ];
        let mut out = rects(2);
        let s = solve(Span { w: None, h: None }, NONE, &kids, &rules, &mut out);
        assert_eq!((s.w, s.h), (40, 35));
        assert_eq!(
            out[1],
            Rect {
                x: 0,
                y: 20,
                w: 30,
                h: 10
            }
        );
    }

    #[test]
    fn a_centred_child_in_a_wrap_content_axis_sits_at_the_start() {
        let kids = [Kid::sized(40, 20), Kid::sized(10, 10)];
        let rules = [Rules::default(), Rules::flags(F_CENTER_HORIZONTAL)];
        let mut out = rects(2);
        let s = solve(
            Span {
                w: None,
                h: Some(20),
            },
            NONE,
            &kids,
            &rules,
            &mut out,
        );
        assert_eq!(s.w, 40);
        // Second step: centred in the measured 40.
        assert_eq!(out[1].x, 15);
    }

    #[test]
    fn margins_on_both_sides_of_a_sibling_rule() {
        let kids = [
            Kid::sized(10, 10).with_margins(2, 0, 3, 0),
            Kid::sized(10, 10).with_margins(4, 0, 0, 0),
        ];
        let rules = [
            Rules::flags(F_PARENT_LEFT),
            Rules::default().anchor(A_TO_RIGHT_OF, Anchor::Sibling(0)),
        ];
        let mut out = rects(2);
        solve(fixed(100, 10), NONE, &kids, &rules, &mut out);
        assert_eq!(out[0].x, 2);
        assert_eq!(out[1].x, 2 + 10 + 3 + 4);
    }

    #[test]
    fn align_edges_follow_the_siblings_edges() {
        let kids = [Kid::sized(50, 20), Kid::sized(10, 10), Kid::sized(10, 10)];
        let rules = [
            Rules::flags(F_CENTER_IN_PARENT),
            Rules::default()
                .anchor(A_ALIGN_LEFT, Anchor::Sibling(0))
                .anchor(A_ALIGN_TOP, Anchor::Sibling(0)),
            Rules::default()
                .anchor(A_ALIGN_RIGHT, Anchor::Sibling(0))
                .anchor(A_ALIGN_BOTTOM, Anchor::Sibling(0)),
        ];
        let mut out = rects(3);
        solve(fixed(100, 100), NONE, &kids, &rules, &mut out);
        assert_eq!(
            out[0],
            Rect {
                x: 25,
                y: 40,
                w: 50,
                h: 20
            }
        );
        assert_eq!(
            out[1],
            Rect {
                x: 25,
                y: 40,
                w: 10,
                h: 10
            }
        );
        assert_eq!(
            out[2],
            Rect {
                x: 65,
                y: 50,
                w: 10,
                h: 10
            }
        );
    }

    #[test]
    fn container_gravity_moves_the_group_as_one() {
        let kids = [Kid::sized(10, 10), Kid::sized(10, 10)];
        let rules = [
            Rules::default(),
            Rules::default().anchor(A_TO_RIGHT_OF, Anchor::Sibling(0)),
        ];
        let mut out = rects(2);
        solve(
            fixed(100, 50),
            (Some(Place::End), Some(Place::Center)),
            &kids,
            &rules,
            &mut out,
        );
        assert_eq!(
            out[0],
            Rect {
                x: 80,
                y: 20,
                w: 10,
                h: 10
            }
        );
        assert_eq!(
            out[1],
            Rect {
                x: 90,
                y: 20,
                w: 10,
                h: 10
            }
        );
        // A child at the far edge leaves nothing to move.
        let rules = [Rules::flags(F_PARENT_LEFT), Rules::flags(F_PARENT_RIGHT)];
        solve(
            fixed(100, 50),
            (Some(Place::End), None),
            &kids,
            &rules,
            &mut out,
        );
        assert_eq!((out[0].x, out[1].x), (0, 90));
    }

    #[test]
    fn a_right_edge_alone_places_the_child_by_its_end() {
        let kids = [Kid::sized(10, 10).with_margins(0, 0, 4, 0)];
        let rules = [Rules::flags(F_PARENT_RIGHT)];
        let mut out = rects(1);
        solve(fixed(100, 10), NONE, &kids, &rules, &mut out);
        assert_eq!(out[0].x, 86);
    }
}
