// SPDX-License-Identifier: GPL-3.0-only
//! `LinearLayout`'s solver: Android's `measureHorizontal` / `layoutHorizontal`
//! (and their vertical twins), pure.
//!
//! Children are stacked along the orientation with their margins (and the
//! container's `pad_row` / `pad_column` gap, which nothing sets since
//! `setSpacing` went in 2026-10: Android has no gap). A weight shares the space left over
//! — or the overflow, when the children are too big — in proportion
//! (`weight / remaining sum × remaining delta`, sequentially, as Android has
//! done since N). On the cross axis each child is placed by its own
//! `layout_gravity` (a horizontal layout reads only the vertical bits, a
//! vertical one only the horizontal bits, as Android does), else by the
//! container's gravity, else at the start; `match_parent` fills the cross
//! axis within the child's margins. `fill` gravity places at the start, as
//! Android's `LinearLayout` does (only `match_parent` stretches there).

use super::place::{gravity_axis, place_axis, Kid, Place, Rect};
use super::relative::Span;

/// The most children one `LinearLayout` lays out by the solver.
pub const MAX_KIDS: usize = 32;

/// `layout_weight` as thousandths: `1.0f` is 1000.
pub const WEIGHT_UNIT: i32 = 1000;

/// A child as the solver sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinearKid {
    pub kid: Kid,
    /// `layout_weight` in thousandths; 0 for none.
    pub weight: i32,
    /// The child's `layout_gravity`, Android's bits; 0 for none.
    pub gravity: i32,
}

/// The container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Linear {
    pub horizontal: bool,
    /// The gap between adjacent children.
    pub gap: i32,
    /// `setGravity`, Android's bits; 0 for the default (`START | TOP`).
    pub gravity: i32,
}

/// What `solve` found out: the extent the children need on each axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Solved {
    pub w: i32,
    pub h: i32,
}

/// The container's content span on the main axis, and the cross axis.
fn spans(l: &Linear, span: Span) -> (Option<i32>, Option<i32>) {
    if l.horizontal {
        (span.w, span.h)
    } else {
        (span.h, span.w)
    }
}

fn main_size(l: &Linear, k: &Kid) -> i32 {
    if l.horizontal {
        k.w
    } else {
        k.h
    }
}

fn cross_size(l: &Linear, k: &Kid) -> i32 {
    if l.horizontal {
        k.h
    } else {
        k.w
    }
}

fn main_match(l: &Linear, k: &Kid) -> bool {
    if l.horizontal {
        k.match_w
    } else {
        k.match_h
    }
}

fn cross_match(l: &Linear, k: &Kid) -> bool {
    if l.horizontal {
        k.match_h
    } else {
        k.match_w
    }
}

/// (start, end) margins on the main axis.
fn main_margins(l: &Linear, k: &Kid) -> (i32, i32) {
    if l.horizontal {
        (k.margin[0], k.margin[2])
    } else {
        (k.margin[1], k.margin[3])
    }
}

fn cross_margins(l: &Linear, k: &Kid) -> (i32, i32) {
    if l.horizontal {
        (k.margin[1], k.margin[3])
    } else {
        (k.margin[0], k.margin[2])
    }
}

/// A child's weight: `match_parent` on the main axis with no weight of its
/// own takes the room left, as a weight of 1 would.
fn weight_of(l: &Linear, k: &LinearKid) -> i32 {
    if k.weight > 0 {
        k.weight
    } else if main_match(l, &k.kid) {
        WEIGHT_UNIT
    } else {
        0
    }
}

/// The size a child brings to the main axis before weights: its own, or
/// nothing for a `match_parent` child (which is all weight).
fn base_size(l: &Linear, k: &LinearKid) -> i32 {
    if main_match(l, &k.kid) {
        0
    } else {
        main_size(l, &k.kid)
    }
}

/// Place `kids` in a container of `span`, writing each child's rectangle to
/// `out`. `out` is as long as `kids`, at most [`MAX_KIDS`] of them.
pub fn solve(span: Span, l: Linear, kids: &[LinearKid], out: &mut [Rect]) -> Solved {
    let n = kids.len().min(out.len()).min(MAX_KIDS);
    let (kids, out) = (&kids[..n], &mut out[..n]);
    let (main_span, cross_span) = spans(&l, span);

    // Main axis: the bases, the margins and the gaps, then the delta shared
    // by weight.
    let mut size = [0i32; MAX_KIDS];
    let mut total = 0;
    let mut weight_sum = 0;
    let mut first = true;
    for (i, k) in kids.iter().enumerate() {
        if k.kid.gone {
            continue;
        }
        size[i] = base_size(&l, k);
        let (ms, me) = main_margins(&l, &k.kid);
        total += size[i] + ms + me + if first { 0 } else { l.gap };
        weight_sum += weight_of(&l, k);
        first = false;
    }
    let extent_main = total;
    if let Some(span) = main_span {
        if weight_sum > 0 {
            let mut remaining = span - total;
            let mut remaining_weight = weight_sum;
            for (i, k) in kids.iter().enumerate() {
                let w = if k.kid.gone { 0 } else { weight_of(&l, k) };
                if w == 0 {
                    continue;
                }
                // Android's share: `weight * delta / sum`, in i64 since the
                // weights are thousandths.
                let share = (w as i64 * remaining as i64 / remaining_weight as i64) as i32;
                remaining -= share;
                remaining_weight -= w;
                size[i] = (size[i] + share).max(0);
            }
            total = span.max(0);
        }
    }

    // Cross axis: the extent is the widest child with its margins, a
    // `match_parent` child counting its margins only.
    let mut extent_cross = 0;
    for k in kids {
        if k.kid.gone {
            continue;
        }
        let (cs, ce) = cross_margins(&l, &k.kid);
        let own = if cross_match(&l, &k.kid) {
            0
        } else {
            cross_size(&l, &k.kid)
        };
        extent_cross = extent_cross.max(own + cs + ce);
    }
    let cross = cross_span.unwrap_or(extent_cross);

    // Positions.
    let main_place = gravity_axis(l.gravity, !l.horizontal).unwrap_or(Place::Start);
    let mut cursor = match main_span {
        Some(span) if weight_sum == 0 => place_axis(main_place, span, total),
        _ => 0,
    };
    let container_cross = gravity_axis(l.gravity, l.horizontal);
    let mut first = true;
    for (i, k) in kids.iter().enumerate() {
        let r = &mut out[i];
        if k.kid.gone {
            *r = Rect::default();
            continue;
        }
        let (ms, me) = main_margins(&l, &k.kid);
        if !first {
            cursor += l.gap;
        }
        first = false;
        let main_pos = cursor + ms;
        cursor += ms + size[i] + me;

        let (cs, ce) = cross_margins(&l, &k.kid);
        let (cross_pos, cross_sz) = if cross_match(&l, &k.kid) {
            (cs, (cross - cs - ce).max(0))
        } else {
            let own = cross_size(&l, &k.kid);
            let place = gravity_axis(k.gravity, l.horizontal)
                .or(container_cross)
                .unwrap_or(Place::Start);
            // Android's `layoutHorizontal`: centre within the space, then the
            // margins' difference; end flush with the end margin.
            let pos = match place {
                Place::Center => (cross - own) / 2 + cs - ce,
                Place::End => cross - own - ce,
                Place::Start | Place::Fill => cs,
            };
            (pos, own)
        };
        if l.horizontal {
            *r = Rect {
                x: main_pos,
                y: cross_pos,
                w: size[i],
                h: cross_sz,
            };
        } else {
            *r = Rect {
                x: cross_pos,
                y: main_pos,
                w: cross_sz,
                h: size[i],
            };
        }
    }

    let (w, h) = if l.horizontal {
        (main_span.unwrap_or(extent_main), cross)
    } else {
        (cross, main_span.unwrap_or(extent_main))
    };
    Solved { w, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: i32 = 0x03;
    const RIGHT: i32 = 0x05;
    const TOP: i32 = 0x30;
    const BOTTOM: i32 = 0x50;
    const CENTER_HORIZONTAL: i32 = 0x01;
    const CENTER_VERTICAL: i32 = 0x10;
    const CENTER: i32 = 0x11;
    const FILL_HORIZONTAL: i32 = 0x07;

    fn row(gap: i32, gravity: i32) -> Linear {
        Linear {
            horizontal: true,
            gap,
            gravity,
        }
    }

    fn column(gap: i32, gravity: i32) -> Linear {
        Linear {
            horizontal: false,
            gap,
            gravity,
        }
    }

    fn fixed(w: i32, h: i32) -> Span {
        Span {
            w: Some(w),
            h: Some(h),
        }
    }

    fn plain(w: i32, h: i32) -> LinearKid {
        LinearKid {
            kid: Kid::sized(w, h),
            weight: 0,
            gravity: 0,
        }
    }

    fn weighted(w: i32, h: i32, weight: i32) -> LinearKid {
        LinearKid {
            kid: Kid::sized(w, h),
            weight,
            gravity: 0,
        }
    }

    fn rects() -> [Rect; MAX_KIDS] {
        [Rect::default(); MAX_KIDS]
    }

    #[test]
    fn a_row_stacks_children_with_margins_and_gaps_as_flex_did() {
        // layoutdemo's "spaced" row: 10 px boxes, the second with margins 6/9.
        let kids = [
            plain(10, 10),
            LinearKid {
                kid: Kid::sized(10, 10).with_margins(6, 0, 9, 0),
                ..plain(10, 10)
            },
            plain(10, 10),
        ];
        let mut out = rects();
        solve(fixed(200, 10), row(0, 0), &kids, &mut out);
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
                x: 16,
                y: 0,
                w: 10,
                h: 10
            }
        );
        assert_eq!(
            out[2],
            Rect {
                x: 35,
                y: 0,
                w: 10,
                h: 10
            }
        );
        // With a gap.
        solve(fixed(200, 10), row(4, 0), &kids, &mut out);
        assert_eq!((out[0].x, out[1].x, out[2].x), (0, 20, 43));
    }

    #[test]
    fn a_weighted_space_pushes_its_neighbours_apart() {
        // layoutdemo's "vocab" row: a box, a 0dp weighted Space, a box.
        let kids = [plain(10, 10), weighted(0, 10, 1000), plain(10, 10)];
        let mut out = rects();
        let s = solve(fixed(320, 10), row(0, 0), &kids, &mut out);
        assert_eq!(out[1].w, 300);
        assert_eq!(out[2].x, 310);
        assert_eq!(s.w, 320);
    }

    #[test]
    fn weights_share_the_delta_sequentially_as_android_does() {
        let kids = [
            weighted(0, 10, 1000),
            weighted(0, 10, 1000),
            weighted(0, 10, 1000),
        ];
        let mut out = rects();
        solve(fixed(100, 10), row(0, 0), &kids, &mut out);
        assert_eq!((out[0].w, out[1].w, out[2].w), (33, 33, 34));
        assert_eq!((out[0].x, out[1].x, out[2].x), (0, 33, 66));
        // Fractional weights keep their ratio: 1.5 : 1.
        let kids = [weighted(0, 10, 1500), weighted(0, 10, 1000)];
        solve(fixed(100, 10), row(0, 0), &kids, &mut out);
        assert_eq!((out[0].w, out[1].w), (60, 40));
    }

    #[test]
    fn a_weight_adds_to_a_measured_size_and_a_negative_delta_shrinks() {
        // wrap_content + weight: the child keeps its text and takes the rest.
        let kids = [plain(40, 10), weighted(20, 10, 1000)];
        let mut out = rects();
        solve(fixed(100, 10), row(0, 0), &kids, &mut out);
        assert_eq!(
            out[1],
            Rect {
                x: 40,
                y: 0,
                w: 60,
                h: 10
            }
        );
        // Too big for the row: the weighted child gives the overflow back.
        let kids = [plain(80, 10), weighted(50, 10, 1000)];
        solve(fixed(100, 10), row(0, 0), &kids, &mut out);
        assert_eq!(
            out[1],
            Rect {
                x: 80,
                y: 0,
                w: 20,
                h: 10
            }
        );
    }

    #[test]
    fn match_parent_on_the_main_axis_takes_the_room_left() {
        let kids = [
            plain(30, 10),
            LinearKid {
                kid: Kid::sized(1, 10).matching(true, false),
                ..plain(1, 10)
            },
        ];
        let mut out = rects();
        solve(fixed(100, 10), row(4, 0), &kids, &mut out);
        assert_eq!(
            out[1],
            Rect {
                x: 34,
                y: 0,
                w: 66,
                h: 10
            }
        );
        // Two of them split it, as Android's match_parent + weight idiom does.
        let kids = [
            LinearKid {
                kid: Kid::sized(1, 10).matching(true, false),
                weight: 1000,
                gravity: 0,
            },
            LinearKid {
                kid: Kid::sized(1, 10).matching(true, false),
                weight: 1000,
                gravity: 0,
            },
        ];
        solve(fixed(100, 10), row(0, 0), &kids, &mut out);
        assert_eq!((out[0].w, out[1].w, out[1].x), (50, 50, 50));
    }

    #[test]
    fn container_gravity_places_the_run_on_the_main_axis() {
        let kids = [plain(10, 10), plain(10, 10)];
        let mut out = rects();
        solve(fixed(100, 10), row(0, RIGHT), &kids, &mut out);
        assert_eq!((out[0].x, out[1].x), (80, 90));
        solve(fixed(100, 10), row(0, CENTER_HORIZONTAL), &kids, &mut out);
        assert_eq!((out[0].x, out[1].x), (40, 50));
        // In a column, the vertical bits are the main axis.
        solve(fixed(10, 100), column(0, BOTTOM), &kids, &mut out);
        assert_eq!((out[0].y, out[1].y), (80, 90));
        // A weighted child takes the room, so there is none to place.
        let kids = [plain(10, 10), weighted(0, 10, 1000)];
        solve(fixed(100, 10), row(0, RIGHT), &kids, &mut out);
        assert_eq!((out[0].x, out[1].x, out[1].w), (0, 10, 90));
    }

    #[test]
    fn the_cross_axis_follows_the_container_gravity_then_each_childs_own() {
        // A 30-tall row: a 20 and a 10 tall child.
        let kids = [plain(10, 20), plain(10, 10)];
        let mut out = rects();
        solve(fixed(100, 30), row(0, 0), &kids, &mut out);
        assert_eq!((out[0].y, out[1].y), (0, 0));
        solve(fixed(100, 30), row(0, CENTER_VERTICAL), &kids, &mut out);
        assert_eq!((out[0].y, out[1].y), (5, 10));
        solve(fixed(100, 30), row(0, BOTTOM), &kids, &mut out);
        assert_eq!((out[0].y, out[1].y), (10, 20));
        // Per-child gravity overrides the container's; a horizontal row reads
        // only the vertical bits, so RIGHT alone changes nothing.
        let kids = [
            LinearKid {
                gravity: BOTTOM,
                ..plain(10, 20)
            },
            LinearKid {
                gravity: CENTER_VERTICAL,
                ..plain(10, 10)
            },
            LinearKid {
                gravity: RIGHT,
                ..plain(10, 10)
            },
        ];
        solve(fixed(100, 30), row(0, 0), &kids, &mut out);
        assert_eq!((out[0].y, out[1].y, out[2].y), (10, 10, 0));
        // In a column the horizontal bits place the child across.
        let kids = [
            LinearKid {
                gravity: RIGHT,
                ..plain(10, 10)
            },
            LinearKid {
                gravity: CENTER,
                ..plain(20, 10)
            },
            LinearKid {
                gravity: FILL_HORIZONTAL,
                ..plain(10, 10)
            },
        ];
        solve(fixed(100, 100), column(0, 0), &kids, &mut out);
        assert_eq!((out[0].x, out[1].x, out[2].x), (90, 40, 0));
        assert_eq!(
            out[2].w, 10,
            "fill places at the start, as Android's LinearLayout does"
        );
    }

    #[test]
    fn match_parent_fills_the_cross_axis_within_its_margins() {
        let kids = [
            LinearKid {
                kid: Kid::sized(1, 10)
                    .matching(true, false)
                    .with_margins(8, 0, 4, 0),
                ..plain(1, 10)
            },
            plain(10, 10),
        ];
        let mut out = rects();
        solve(
            fixed(100, 100),
            column(0, CENTER_HORIZONTAL),
            &kids,
            &mut out,
        );
        assert_eq!(
            out[0],
            Rect {
                x: 8,
                y: 0,
                w: 88,
                h: 10
            }
        );
        assert_eq!(out[1].x, 45);
    }

    #[test]
    fn cross_margins_shift_a_centred_child_as_android_does() {
        let kids = [LinearKid {
            kid: Kid::sized(10, 10).with_margins(0, 6, 0, 2),
            gravity: CENTER_VERTICAL,
            ..plain(10, 10)
        }];
        let mut out = rects();
        solve(fixed(100, 30), row(0, 0), &kids, &mut out);
        assert_eq!(out[0].y, (30 - 10) / 2 + 6 - 2);
        let kids = [LinearKid {
            kid: Kid::sized(10, 10).with_margins(0, 0, 0, 3),
            gravity: BOTTOM,
            ..plain(10, 10)
        }];
        solve(fixed(100, 30), row(0, 0), &kids, &mut out);
        assert_eq!(out[0].y, 30 - 10 - 3);
    }

    #[test]
    fn gone_children_take_no_room_invisible_ones_keep_it() {
        let kids = [
            plain(10, 10),
            LinearKid {
                kid: Kid {
                    gone: true,
                    ..Kid::sized(50, 10)
                },
                ..plain(50, 10)
            },
            plain(10, 10),
        ];
        let mut out = rects();
        solve(fixed(100, 10), row(4, 0), &kids, &mut out);
        assert_eq!(out[1], Rect::default());
        assert_eq!(out[2].x, 14);
    }

    #[test]
    fn wrap_content_measures_both_axes() {
        let kids = [
            plain(10, 20),
            LinearKid {
                kid: Kid::sized(30, 10).with_margins(2, 3, 4, 5),
                ..plain(30, 10)
            },
            weighted(0, 10, 1000),
        ];
        let mut out = rects();
        let s = solve(Span { w: None, h: None }, row(4, 0), &kids, &mut out);
        // Main: 10 + 4 + (2 + 30 + 4) + 4 + 0; cross: max(20, 3 + 10 + 5).
        assert_eq!((s.w, s.h), (54, 20));
        assert_eq!(
            out[1],
            Rect {
                x: 16,
                y: 3,
                w: 30,
                h: 10
            }
        );
        // The weighted child keeps its measured size: there is nothing to share.
        assert_eq!(out[2].w, 0);
        // A column measured the other way round.
        let s = solve(Span { w: None, h: None }, column(0, 0), &kids, &mut out);
        assert_eq!((s.w, s.h), (36, 48));
    }

    #[test]
    fn a_wrap_content_cross_axis_centres_within_the_widest_child() {
        let kids = [plain(10, 20), plain(10, 10)];
        let mut out = rects();
        solve(
            Span {
                w: Some(100),
                h: None,
            },
            row(0, CENTER_VERTICAL),
            &kids,
            &mut out,
        );
        assert_eq!((out[0].y, out[1].y), (0, 5));
    }

    #[test]
    fn a_column_child_at_the_start_of_the_cross_axis_by_default() {
        let kids = [plain(10, 10)];
        let mut out = rects();
        solve(fixed(100, 100), column(0, 0), &kids, &mut out);
        assert_eq!(
            out[0],
            Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10
            }
        );
        solve(fixed(100, 100), column(0, LEFT | TOP), &kids, &mut out);
        assert_eq!(out[0].x, 0);
    }
}
