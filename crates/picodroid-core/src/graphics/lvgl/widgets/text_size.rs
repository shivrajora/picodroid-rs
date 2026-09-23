// SPDX-License-Identifier: GPL-3.0-only
//! `TextView.setTextSize` as LVGL sees it: the compiled face nearest the size an app asked for.
//! The faces a board compiles (its `text_sizes` ladder, listed by `pd_fonts.c`) are bitmaps, so
//! a size in between snaps; `text_view.rs` applies the pick.

/// The index into `sizes` (pixel sizes, any order) of the face nearest `px`. A tie goes to the
/// larger face: Android would render the exact size, so err on the side of legibility. A size
/// of zero or less takes the smallest face (Android draws nothing at 0; a blank label is no use
/// on a display this small). `None` for an empty ladder or a NaN, which the caller ignores.
pub(crate) fn nearest(sizes: &[u8], px: f32) -> Option<usize> {
    if sizes.is_empty() || px.is_nan() {
        return None;
    }
    let px = if px > 0.0 { px } else { 0.0 };
    let mut best: Option<(usize, f32, u8)> = None;
    for (i, &size) in sizes.iter().enumerate() {
        let distance = (f32::from(size) - px).abs();
        let better = match best {
            None => true,
            Some((_, best_distance, best_size)) => {
                distance < best_distance || (distance == best_distance && size > best_size)
            }
        };
        if better {
            best = Some((i, distance, size));
        }
    }
    best.map(|(i, _, _)| i)
}

#[cfg(test)]
mod tests {
    use super::nearest;

    const LADDER: &[u8] = &[14, 20, 28, 64];

    #[test]
    fn an_empty_ladder_has_no_face() {
        assert_eq!(nearest(&[], 14.0), None);
    }

    #[test]
    fn a_nan_size_is_ignored() {
        assert_eq!(nearest(LADDER, f32::NAN), None);
    }

    #[test]
    fn an_exact_size_is_its_own_face() {
        assert_eq!(nearest(LADDER, 14.0), Some(0));
        assert_eq!(nearest(LADDER, 28.0), Some(2));
        assert_eq!(nearest(LADDER, 64.0), Some(3));
    }

    #[test]
    fn between_two_faces_the_nearer_wins() {
        assert_eq!(nearest(LADDER, 23.0), Some(1));
        assert_eq!(nearest(LADDER, 25.0), Some(2));
        assert_eq!(nearest(LADDER, 17.0), Some(1));
    }

    #[test]
    fn halfway_goes_to_the_larger_face() {
        assert_eq!(nearest(LADDER, 24.0), Some(2));
        assert_eq!(nearest(LADDER, 17.0), Some(1));
        assert_eq!(nearest(LADDER, 46.0), Some(3));
    }

    #[test]
    fn below_the_smallest_face_is_the_smallest() {
        assert_eq!(nearest(LADDER, 1.0), Some(0));
        assert_eq!(nearest(LADDER, 0.0), Some(0));
        assert_eq!(nearest(LADDER, -5.0), Some(0));
    }

    #[test]
    fn above_the_largest_face_is_the_largest() {
        assert_eq!(nearest(LADDER, 200.0), Some(3));
        assert_eq!(nearest(LADDER, f32::INFINITY), Some(3));
    }

    #[test]
    fn one_face_is_always_it() {
        assert_eq!(nearest(&[14], 64.0), Some(0));
        assert_eq!(nearest(&[14], 0.0), Some(0));
    }

    #[test]
    fn the_ladder_order_does_not_matter() {
        assert_eq!(nearest(&[64, 14, 28, 20], 23.0), Some(3));
        assert_eq!(nearest(&[64, 14, 28, 20], 24.0), Some(2));
    }
}
