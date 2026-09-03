// SPDX-License-Identifier: GPL-3.0-only
//! The button-edge queue between a GPIO interrupt and the UI task, shared by
//! every family.
//!
//! An edge interrupt has a few microseconds to record "pin N went low at
//! time T" and get out; the UI task drains those records at its own pace,
//! which can be hundreds of milliseconds later while it stalls on an
//! Activity transition. Every family used to write the same fixed ring for
//! that, with the same drop tally and the same "queue overflow" warning
//! (`docs/designs/porting-seam-2026-09.md` E2). This is that ring, once.
//!
//! # Contract
//!
//! One producer context and one consumer context. The producer is an
//! interrupt handler, or a task that has masked that interrupt for the
//! duration of [`GpioEventRing::enqueue`] — the RP family's `inject` does
//! exactly that. The consumer is the task that calls
//! [`GpioEventRing::drain`]. A simulator whose producers are host threads
//! wraps the ring in a `Mutex` and keeps the same type.
//!
//! Timestamps and wake-ups stay with the caller: the ring knows neither the
//! family's timer nor its semaphore. The capacity is `N - 1` — one slot is
//! the full/empty sentinel.
//!
//! thumbv6m has no atomic read-modify-write, so every counter here is a
//! single-writer load/store pair; nothing needs compare-and-swap.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use super::types::GpioEvent;

/// A fixed-capacity single-producer / single-consumer ring of button edges.
pub struct GpioEventRing<const N: usize> {
    slots: UnsafeCell<[GpioEvent; N]>,
    /// Next slot the producer writes. Producer-owned.
    head: AtomicUsize,
    /// Next slot the consumer reads. Consumer-owned.
    tail: AtomicUsize,
    /// Edges refused because the ring was full. Producer-owned tally.
    dropped: AtomicU32,
    /// The tally as last warned about. Consumer-owned.
    reported: AtomicU32,
}

// SAFETY: the producer writes `slots[head]` and then publishes `head` with
// Release; the consumer loads `head` with Acquire before it reads
// `slots[tail]`, and publishes `tail` with Release; the producer loads
// `tail` with Acquire before it treats a slot as free. Under the one
// producer / one consumer contract no slot is ever written and read at the
// same time.
unsafe impl<const N: usize> Sync for GpioEventRing<N> {}

impl<const N: usize> GpioEventRing<N> {
    pub const fn new() -> Self {
        Self {
            slots: UnsafeCell::new(
                [GpioEvent {
                    pin: 0,
                    rising: false,
                    t_us: 0,
                }; N],
            ),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicU32::new(0),
            reported: AtomicU32::new(0),
        }
    }

    /// Record an edge. `false` means the ring was full: the edge is dropped
    /// and tallied, and the next [`drain`](Self::drain) warns once.
    pub fn enqueue(&self, pin: u8, rising: bool, t_us: u32) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let next = (head + 1) % N;
        if next == self.tail.load(Ordering::Acquire) {
            let dropped = self.dropped.load(Ordering::Relaxed).wrapping_add(1);
            self.dropped.store(dropped, Ordering::Relaxed);
            return false;
        }
        // SAFETY: slot `head` is free (the consumer has published past it or
        // never reached it) and invisible to the consumer until `head` is
        // published below.
        unsafe {
            (*self.slots.get())[head] = GpioEvent { pin, rising, t_us };
        }
        self.head.store(next, Ordering::Release);
        true
    }

    /// The oldest edge, if any.
    ///
    /// Reports overflow here rather than in the interrupt: once per change of
    /// the drop tally, through the shared log, so a family does nothing to
    /// get the warning.
    pub fn drain(&self) -> Option<GpioEvent> {
        let dropped = self.dropped.load(Ordering::Relaxed);
        if dropped != self.reported.load(Ordering::Relaxed) {
            self.reported.store(dropped, Ordering::Relaxed);
            crate::pd_warn!(
                "gpio: event queue overflow — {} button edge(s) dropped since boot",
                dropped
            );
        }
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            return None;
        }
        // SAFETY: slot `tail` was written before the producer published a
        // `head` past it, which the Acquire load above observed.
        let ev = unsafe { (*self.slots.get())[tail] };
        self.tail.store((tail + 1) % N, Ordering::Release);
        Some(ev)
    }

    pub fn has_pending(&self) -> bool {
        self.tail.load(Ordering::Relaxed) != self.head.load(Ordering::Acquire)
    }

    /// Edges dropped since boot.
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl<const N: usize> Default for GpioEventRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(pin: u8) -> (u8, bool, u32) {
        // Distinct values per field, so a swapped field is a different event.
        (pin, pin % 2 == 0, 1000 + pin as u32)
    }

    #[test]
    fn edges_come_out_in_the_order_they_went_in() {
        let ring = GpioEventRing::<8>::new();
        for pin in 1..=5 {
            let (p, r, t) = ev(pin);
            assert!(ring.enqueue(p, r, t));
        }
        for pin in 1..=5 {
            let (p, r, t) = ev(pin);
            assert_eq!(
                ring.drain(),
                Some(GpioEvent {
                    pin: p,
                    rising: r,
                    t_us: t
                })
            );
        }
        assert_eq!(ring.drain(), None);
    }

    #[test]
    fn capacity_is_one_less_than_the_slot_count() {
        let ring = GpioEventRing::<8>::new();
        for i in 0..7 {
            assert!(ring.enqueue(i, false, 0), "slot {i} should be free");
        }
        // The eighth edge is refused and counted. A ring that used all N
        // slots would accept it and fail here.
        assert!(!ring.enqueue(7, false, 0));
        assert_eq!(ring.dropped(), 1);
        assert!(ring.has_pending());
    }

    #[test]
    fn a_drain_frees_a_slot_and_indices_wrap() {
        let ring = GpioEventRing::<4>::new();
        // Push and pop more edges than there are slots, so head and tail
        // wrap around several times; every edge must still come out intact.
        for round in 0..20u8 {
            assert!(ring.enqueue(round, round % 3 == 0, round as u32 * 7));
            assert!(ring.enqueue(round.wrapping_add(100), false, 1));
            assert_eq!(
                ring.drain(),
                Some(GpioEvent {
                    pin: round,
                    rising: round % 3 == 0,
                    t_us: round as u32 * 7
                })
            );
            assert_eq!(ring.drain().map(|e| e.pin), Some(round.wrapping_add(100)));
        }
        assert!(!ring.has_pending());
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn dropped_edges_are_tallied_and_do_not_disturb_the_queued_ones() {
        let ring = GpioEventRing::<3>::new();
        assert!(ring.enqueue(1, false, 10));
        assert!(ring.enqueue(2, true, 20));
        assert!(!ring.enqueue(3, false, 30));
        assert!(!ring.enqueue(4, false, 40));
        assert_eq!(ring.dropped(), 2);
        assert_eq!(ring.drain().map(|e| e.pin), Some(1));
        assert_eq!(ring.drain().map(|e| e.pin), Some(2));
        assert_eq!(ring.drain(), None);
        // Room again after draining.
        assert!(ring.enqueue(5, false, 50));
        assert_eq!(ring.drain().map(|e| e.pin), Some(5));
    }

    #[test]
    fn a_fresh_ring_is_empty() {
        let ring = GpioEventRing::<16>::default();
        assert!(!ring.has_pending());
        assert_eq!(ring.drain(), None);
        assert_eq!(ring.dropped(), 0);
    }
}
