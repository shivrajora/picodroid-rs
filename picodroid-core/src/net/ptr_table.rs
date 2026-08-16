// SPDX-License-Identifier: GPL-3.0-only
//! Fixed-capacity pointer ↔ Java `handle` (i32) table with slot reuse.
//!
//! Backs [`socket_table`](super::socket_table) and
//! [`http_table`](super::http_table). One implementation for every pointer
//! width: the old 32-bit arms handed the raw pointer to Java as the handle,
//! which made close-then-use a dangling dereference into the network stack
//! (device-only, sim-invisible — the same hazard class as the pre-
//! generational LVGL handle_table), and the old 64-bit arms never reused
//! slots, so a create/close loop exhausted the table after N sockets.
//!
//! Handles are `slot + 1` (0 is never a valid handle). A freed slot may be
//! handed out again, so a stale Java object can reach a *newer* socket —
//! the same aliasing contract as POSIX file descriptors, which is what
//! `java.net` runs on; a null lookup (closed, not yet reused) surfaces as
//! the catchable `SocketException("Socket is closed")`.
//!
//! Not locked: all callers run on the JVM task (natives dispatch there),
//! matching the access pattern the per-width tables already assumed.

use core::cell::UnsafeCell;
use core::ffi::c_void;

pub(super) struct PtrTable<const N: usize> {
    slots: UnsafeCell<[*mut c_void; N]>,
}

// SAFETY: accessed only from the JVM task (see module docs).
unsafe impl<const N: usize> Sync for PtrTable<N> {}

impl<const N: usize> PtrTable<N> {
    pub const fn new() -> Self {
        Self {
            slots: UnsafeCell::new([core::ptr::null_mut(); N]),
        }
    }

    /// Store `ptr` in a free slot and return its handle, or 0 when `ptr`
    /// is null or the table is full (0 never resolves in [`Self::lookup`]).
    pub fn register(&self, ptr: *mut c_void) -> i32 {
        if ptr.is_null() {
            return 0;
        }
        let slots = unsafe { &mut *self.slots.get() };
        for (i, slot) in slots.iter_mut().enumerate() {
            if slot.is_null() {
                *slot = ptr;
                return (i + 1) as i32;
            }
        }
        0
    }

    /// Resolve a handle; null when it was never registered or was removed.
    pub fn lookup(&self, id: i32) -> *mut c_void {
        let slots = unsafe { &*self.slots.get() };
        match id_to_slot::<N>(id) {
            Some(i) => slots[i],
            None => core::ptr::null_mut(),
        }
    }

    /// Free a handle's slot, making it available for reuse.
    pub fn remove(&self, id: i32) {
        let slots = unsafe { &mut *self.slots.get() };
        if let Some(i) = id_to_slot::<N>(id) {
            slots[i] = core::ptr::null_mut();
        }
    }
}

fn id_to_slot<const N: usize>(id: i32) -> Option<usize> {
    if id >= 1 && (id as usize) <= N {
        Some(id as usize - 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(v: usize) -> *mut c_void {
        v as *mut c_void
    }

    #[test]
    fn register_lookup_remove_roundtrip() {
        let t: PtrTable<4> = PtrTable::new();
        let h = t.register(p(0x1000));
        assert_ne!(h, 0);
        assert_eq!(t.lookup(h), p(0x1000));
        t.remove(h);
        assert!(t.lookup(h).is_null());
    }

    #[test]
    fn slots_are_reused_after_remove() {
        let t: PtrTable<2> = PtrTable::new();
        for i in 0..10 {
            let h = t.register(p(0x2000 + i));
            assert_ne!(h, 0, "slot not reused on iteration {i}");
            t.remove(h);
        }
    }

    #[test]
    fn full_table_returns_zero_and_zero_never_resolves() {
        let t: PtrTable<2> = PtrTable::new();
        assert_ne!(t.register(p(0x1)), 0);
        assert_ne!(t.register(p(0x2)), 0);
        assert_eq!(t.register(p(0x3)), 0);
        assert!(t.lookup(0).is_null());
        assert!(t.lookup(-1).is_null());
        assert!(t.lookup(3).is_null());
    }

    #[test]
    fn null_pointer_is_rejected() {
        let t: PtrTable<2> = PtrTable::new();
        assert_eq!(t.register(core::ptr::null_mut()), 0);
    }
}
