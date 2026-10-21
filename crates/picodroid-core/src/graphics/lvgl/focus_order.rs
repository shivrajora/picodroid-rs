// SPDX-License-Identifier: GPL-3.0-only
//! The focus order of a `ListView`'s rows, kept free of LVGL so it runs under
//! `cargo test` (docs/designs/listview-recycling-2026-10.md). A keypad board
//! walks the Activity's group in insertion order, so a row that is recycled
//! from the top of the window to the bottom, or a replacement row the adapter
//! built for a middle slot, lands in the group where it was *added*, not
//! where it now *sits*; [`sync`] puts the rows back in child order with
//! swaps, which is all LVGL's group offers for reordering.
//!
//! The group is seen through [`GroupOps`]: `widgets::list_view` implements it
//! over `lv_group_get_obj_count` / `lv_group_get_obj_by_index` /
//! `lv_group_swap_obj`; the tests implement it over a `Vec`.

/// A focus group, as much of it as reordering needs. `Id` is whatever names
/// a member: an `Obj` on the device, a small integer in the tests.
pub trait GroupOps {
    type Id: Copy + PartialEq;

    /// The members in focus order, by index.
    fn count(&self) -> u32;
    /// The member at `index`, or `None` past the end.
    fn at(&self, index: u32) -> Option<Self::Id>;
    /// Exchange two members' positions.
    fn swap(&mut self, a: Self::Id, b: Self::Id);
}

/// The index of `id` in `g`, or `None` when it is not a member.
fn index_of<G: GroupOps>(g: &G, id: G::Id) -> Option<u32> {
    (0..g.count()).find(|&i| g.at(i) == Some(id))
}

/// Make the group's run of `rows` follow the order of `rows` itself (the
/// list's child order): the lowest index any row holds is the run's base,
/// and row `i` is walked into slot `base + i` by swapping it with its
/// predecessor until it arrives. Adjacent swaps shift the members it passes
/// up by one and keep their relative order, so the widgets outside the run
/// (a button below the list) stay in the order they were added. A row that
/// is already in place costs nothing; a recycled row walks the window's
/// length. Rows that are not in the group (a touch board has no group at
/// all) are skipped. Returns the number of swaps made.
pub fn sync<G: GroupOps>(rows: &[G::Id], g: &mut G) -> u32 {
    let members: u32 = rows.iter().filter(|&&r| index_of(g, r).is_some()).count() as u32;
    if members == 0 {
        return 0;
    }
    let Some(base) = rows.iter().filter_map(|&r| index_of(g, r)).min() else {
        return 0;
    };
    // Only `members` slots exist for the run: a base near the end of the
    // group (which cannot happen when the rows were ever contiguous, but a
    // guard is cheaper than a reasoned invariant) is pulled back so every
    // target slot is a real index.
    let base = base.min(g.count().saturating_sub(members));
    let mut swaps = 0;
    let mut slot = base;
    for &row in rows {
        let Some(mut at) = index_of(g, row) else {
            continue;
        };
        // The slots below `slot` hold the rows already placed, so a row still
        // to place is never below its slot; the walk only ever moves up.
        while at > slot {
            let Some(before) = g.at(at - 1) else {
                break;
            };
            g.swap(row, before);
            swaps += 1;
            at -= 1;
        }
        slot += 1;
    }
    swaps
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A group as a vector of member ids in focus order.
    struct VecGroup(Vec<u32>);

    impl GroupOps for VecGroup {
        type Id = u32;

        fn count(&self) -> u32 {
            self.0.len() as u32
        }

        fn at(&self, index: u32) -> Option<u32> {
            self.0.get(index as usize).copied()
        }

        fn swap(&mut self, a: u32, b: u32) {
            let ia = self.0.iter().position(|&x| x == a);
            let ib = self.0.iter().position(|&x| x == b);
            if let (Some(ia), Some(ib)) = (ia, ib) {
                self.0.swap(ia, ib);
            }
        }
    }

    // Members: 100 = a widget above the list, 200 = a button below it;
    // 1..=4 are rows.

    #[test]
    fn already_ordered_is_a_no_op() {
        let mut g = VecGroup(vec![100, 1, 2, 3, 4, 200]);
        assert_eq!(sync(&[1, 2, 3, 4], &mut g), 0);
        assert_eq!(g.0, vec![100, 1, 2, 3, 4, 200]);
    }

    #[test]
    fn top_row_recycled_to_the_bottom() {
        // Child order is now 2, 3, 4, 1; the group still has 1 first.
        let mut g = VecGroup(vec![100, 1, 2, 3, 4, 200]);
        sync(&[2, 3, 4, 1], &mut g);
        assert_eq!(g.0, vec![100, 2, 3, 4, 1, 200]);
    }

    #[test]
    fn bottom_row_recycled_to_the_top() {
        let mut g = VecGroup(vec![100, 1, 2, 3, 4, 200]);
        sync(&[4, 1, 2, 3], &mut g);
        assert_eq!(g.0, vec![100, 4, 1, 2, 3, 200]);
    }

    #[test]
    fn replacement_row_appended_after_a_trailing_button() {
        // Row 2 was replaced by row 5, which `lv_group_add_obj` put last:
        // after the button below the list.
        let mut g = VecGroup(vec![100, 1, 3, 4, 200, 5]);
        sync(&[1, 5, 3, 4], &mut g);
        assert_eq!(g.0, vec![100, 1, 5, 3, 4, 200]);
    }

    #[test]
    fn trailing_widgets_keep_their_relative_order() {
        let mut g = VecGroup(vec![100, 1, 2, 200, 300, 3]);
        sync(&[1, 2, 3], &mut g);
        assert_eq!(g.0, vec![100, 1, 2, 3, 200, 300]);
    }

    #[test]
    fn rows_outside_the_group_are_skipped() {
        // Row 9 never joined (styled while no group was active).
        let mut g = VecGroup(vec![100, 2, 1, 200]);
        assert_eq!(sync(&[1, 9, 2], &mut g), 1);
        assert_eq!(g.0, vec![100, 1, 2, 200]);
    }

    #[test]
    fn no_group_members_is_a_no_op() {
        let mut g = VecGroup(vec![100, 200]);
        assert_eq!(sync(&[1, 2], &mut g), 0);
        assert_eq!(g.0, vec![100, 200]);
    }

    #[test]
    fn full_rotation_of_the_window() {
        // Every row moved (a jump by more than a window re-binds in place, so
        // the order is unchanged; but a drag of three rows rotates thrice).
        let mut g = VecGroup(vec![1, 2, 3, 4, 5, 200]);
        sync(&[4, 5, 1, 2, 3], &mut g);
        assert_eq!(g.0, vec![4, 5, 1, 2, 3, 200]);
    }
}
