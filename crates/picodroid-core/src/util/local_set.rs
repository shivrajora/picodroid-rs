// SPDX-License-Identifier: GPL-3.0-only
//! A fixed-capacity set for one context: the small registries of raw widget
//! pointers the widget layer keeps ("these text fields want a number pad").
//!
//! Same footing as [`LocalRing`](super::local_ring::LocalRing): `Cell`s
//! throughout, so it sits in a [`Core0`](super::local::Core0) static and
//! every operation is safe code.

use core::cell::Cell;

pub struct LocalSet<T: Copy + PartialEq, const N: usize> {
    /// The members are `items[..len]`, in no particular order.
    items: Cell<[T; N]>,
    len: Cell<usize>,
}

impl<T: Copy + PartialEq, const N: usize> LocalSet<T, N> {
    /// An empty set. `fill` is what the unused slots hold; it is never
    /// returned.
    pub const fn new(fill: T) -> Self {
        Self {
            items: Cell::new([fill; N]),
            len: Cell::new(0),
        }
    }

    fn members(&self) -> &[Cell<T>] {
        let items: &Cell<[T]> = &self.items;
        &items.as_slice_of_cells()[..self.len.get()]
    }

    pub fn contains(&self, value: T) -> bool {
        self.members().iter().any(|m| m.get() == value)
    }

    /// Add `value`. `false` means the set was full and `value` is not in
    /// it; adding a member again is `true` and changes nothing.
    pub fn insert(&self, value: T) -> bool {
        if self.contains(value) {
            return true;
        }
        let len = self.len.get();
        if len == N {
            return false;
        }
        let items: &Cell<[T]> = &self.items;
        items.as_slice_of_cells()[len].set(value);
        self.len.set(len + 1);
        true
    }

    /// Take `value` out. `true` if it was a member.
    pub fn remove(&self, value: T) -> bool {
        let members = self.members();
        match members.iter().position(|m| m.get() == value) {
            Some(i) => {
                members[i].set(members[members.len() - 1].get());
                self.len.set(members.len() - 1);
                true
            }
            None => false,
        }
    }

    pub fn clear(&self) {
        self.len.set(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_is_idempotent() {
        let s: LocalSet<usize, 4> = LocalSet::new(0);
        assert!(!s.contains(7));
        assert!(s.insert(7));
        assert!(s.insert(7));
        assert!(s.contains(7));
        assert!(s.remove(7));
        assert!(!s.contains(7));
        assert!(!s.remove(7));
    }

    #[test]
    fn full_set_refuses_new_members_only() {
        let s: LocalSet<usize, 2> = LocalSet::new(0);
        assert!(s.insert(1));
        assert!(s.insert(2));
        assert!(!s.insert(3));
        assert!(s.insert(2));
        assert!(!s.contains(3));
    }

    #[test]
    fn remove_keeps_the_others() {
        let s: LocalSet<usize, 4> = LocalSet::new(0);
        for v in [1, 2, 3] {
            assert!(s.insert(v));
        }
        assert!(s.remove(1));
        assert!(s.contains(2) && s.contains(3));
        assert!(s.remove(3));
        assert!(s.contains(2));
        assert!(s.insert(4));
        assert!(s.contains(4));
    }

    #[test]
    fn the_fill_value_is_not_a_member() {
        let s: LocalSet<usize, 4> = LocalSet::new(0);
        assert!(!s.contains(0));
        assert!(s.insert(5));
        s.clear();
        assert!(!s.contains(5));
        assert!(!s.contains(0));
    }
}
