// SPDX-License-Identifier: GPL-3.0-only
//! A fixed-capacity FIFO for one context: the queue an LVGL callback fills
//! and a native drains, both on JVM tasks.
//!
//! Not for anything an interrupt handler or a second core touches — that is
//! [`GpioEventRing`](crate::hal::event_ring::GpioEventRing), whose head and
//! tail are atomics. Here they are `Cell`s, so a ring sits in a
//! [`Core0`](super::local::Core0) static and every operation is safe code.

use core::cell::Cell;

/// `N` slots, of which `N - 1` hold elements: one stays empty so that
/// `head == tail` can only mean "empty".
pub struct LocalRing<T: Copy, const N: usize> {
    slots: Cell<[T; N]>,
    /// Next slot to write.
    head: Cell<usize>,
    /// Next slot to read.
    tail: Cell<usize>,
}

impl<T: Copy, const N: usize> LocalRing<T, N> {
    /// An empty ring. `fill` is what the unused slots hold; it is never
    /// returned.
    pub const fn new(fill: T) -> Self {
        Self {
            slots: Cell::new([fill; N]),
            head: Cell::new(0),
            tail: Cell::new(0),
        }
    }

    fn slots(&self) -> &[Cell<T>] {
        let slots: &Cell<[T]> = &self.slots;
        slots.as_slice_of_cells()
    }

    /// Append `value`. `false` means the ring was full and `value` was
    /// dropped: the elements already queued are the ones that stay.
    pub fn push(&self, value: T) -> bool {
        let head = self.head.get();
        let next = (head + 1) % N;
        if next == self.tail.get() {
            return false;
        }
        self.slots()[head].set(value);
        self.head.set(next);
        true
    }

    /// Take the oldest element.
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.get();
        if tail == self.head.get() {
            return None;
        }
        let value = self.slots()[tail].get();
        self.tail.set((tail + 1) % N);
        Some(value)
    }

    /// Visit the queued elements, oldest first, without taking them.
    pub fn for_each(&self, f: &mut dyn FnMut(T)) {
        let mut i = self.tail.get();
        while i != self.head.get() {
            f(self.slots()[i].get());
            i = (i + 1) % N;
        }
    }

    /// Whether any queued element satisfies `pred`.
    pub fn any(&self, mut pred: impl FnMut(T) -> bool) -> bool {
        let mut i = self.tail.get();
        while i != self.head.get() {
            if pred(self.slots()[i].get()) {
                return true;
            }
            i = (i + 1) % N;
        }
        false
    }

    /// Forget every queued element. The slots keep their old contents,
    /// which nothing can reach.
    pub fn clear(&self) {
        self.head.set(0);
        self.tail.set(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_order() {
        let r: LocalRing<u32, 4> = LocalRing::new(0);
        assert_eq!(r.pop(), None);
        assert!(r.push(1));
        assert!(r.push(2));
        assert_eq!(r.pop(), Some(1));
        assert!(r.push(3));
        assert_eq!(r.pop(), Some(2));
        assert_eq!(r.pop(), Some(3));
        assert_eq!(r.pop(), None);
    }

    #[test]
    fn holds_n_minus_one_and_drops_the_newest() {
        let r: LocalRing<u32, 4> = LocalRing::new(0);
        assert!(r.push(1));
        assert!(r.push(2));
        assert!(r.push(3));
        assert!(!r.push(4));
        assert_eq!(r.pop(), Some(1));
        assert_eq!(r.pop(), Some(2));
        assert_eq!(r.pop(), Some(3));
        assert_eq!(r.pop(), None);
    }

    #[test]
    fn wraps_many_times() {
        let r: LocalRing<u32, 3> = LocalRing::new(0);
        for i in 0..20 {
            assert!(r.push(i));
            assert!(r.push(i + 100));
            assert_eq!(r.pop(), Some(i));
            assert_eq!(r.pop(), Some(i + 100));
        }
        assert_eq!(r.pop(), None);
    }

    #[test]
    fn walks_without_taking() {
        let r: LocalRing<u32, 4> = LocalRing::new(0);
        // Move the window so the walk has to wrap.
        assert!(r.push(9));
        assert!(r.push(9));
        r.pop();
        r.pop();
        assert!(r.push(1));
        assert!(r.push(2));
        assert!(r.push(3));
        let mut seen = Vec::new();
        r.for_each(&mut |v| seen.push(v));
        assert_eq!(seen, vec![1, 2, 3]);
        assert!(r.any(|v| v == 2));
        assert!(!r.any(|v| v == 9));
        assert_eq!(r.pop(), Some(1));
    }

    #[test]
    fn clear_empties() {
        let r: LocalRing<u32, 4> = LocalRing::new(0);
        assert!(r.push(1));
        assert!(r.push(2));
        r.clear();
        assert_eq!(r.pop(), None);
        assert!(!r.any(|_| true));
        assert!(r.push(7));
        assert_eq!(r.pop(), Some(7));
    }
}
