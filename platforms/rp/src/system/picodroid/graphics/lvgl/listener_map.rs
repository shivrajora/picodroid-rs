// SPDX-License-Identifier: GPL-3.0-only
//! Fixed-capacity `raw lv_obj_t* -> Java obj_ref` map shared by every widget
//! listener registry in this module tree.
//!
//! These maps are GC roots (`native_handler::gc_visit_roots`): an entry keeps
//! the Java `View` — and everything its listener lambda captures, typically
//! the whole Activity — alive. Before this type existed the registries were
//! append-only arrays, so a destroyed Activity's widgets stayed GC-pinned
//! until an allocator coincidence reused the `lv_obj` address (2026-07-23
//! stress-run PEM-2), a recycled address silently took an update path that
//! never re-attached the freed LVGL trampoline, and a full map silently
//! dropped registrations.
//!
//! The contract now is: every map entry is removed by an `LV_EVENT_DELETE`
//! callback attached to the widget (see `number_picker::picker_delete_cb`,
//! the original model), so [`PtrMap::upsert`] returning [`Upsert::Updated`]
//! proves the widget is still alive and its callbacks intact, and a reused
//! address always takes the `Inserted` path that (re)attaches callbacks.
//! Overflow is reported by the caller via [`warn_full`], never silent.

/// Outcome of [`PtrMap::upsert`]; tells the caller whether LVGL callbacks
/// must be attached (`Inserted`), are provably already attached (`Updated`),
/// or the registration was dropped (`Full` — report it with [`warn_full`]).
#[must_use]
pub enum Upsert {
    Inserted,
    Updated,
    Full,
}

pub struct PtrMap<const N: usize> {
    /// `(raw lv_obj_t*, Java obj_ref)`; slots at `len..` are zeroed.
    entries: [(usize, u16); N],
    len: usize,
}

impl<const N: usize> PtrMap<N> {
    pub const fn new() -> Self {
        Self {
            entries: [(0, 0); N],
            len: 0,
        }
    }

    pub fn upsert(&mut self, ptr: usize, obj_ref: u16) -> Upsert {
        for entry in &mut self.entries[..self.len] {
            if entry.0 == ptr {
                entry.1 = obj_ref;
                return Upsert::Updated;
            }
        }
        if self.len < N {
            self.entries[self.len] = (ptr, obj_ref);
            self.len += 1;
            Upsert::Inserted
        } else {
            Upsert::Full
        }
    }

    pub fn lookup(&self, ptr: usize) -> Option<u16> {
        self.entries[..self.len]
            .iter()
            .find(|e| e.0 == ptr)
            .map(|e| e.1)
    }

    /// Swap-remove every entry for `ptr`, zeroing the vacated tail slot so a
    /// stale obj_ref can never linger past `len` (the GC visitor would still
    /// root it otherwise). No-op if `ptr` is not present.
    pub fn remove(&mut self, ptr: usize) {
        let mut i = 0;
        while i < self.len {
            if self.entries[i].0 == ptr {
                self.entries[i] = self.entries[self.len - 1];
                self.entries[self.len - 1] = (0, 0);
                self.len -= 1;
            } else {
                i += 1;
            }
        }
    }

    /// Visit every registered Java obj_ref as a GC root.
    pub fn visit(&self, visit: &mut dyn FnMut(u16)) {
        for &(_, r) in &self.entries[..self.len] {
            if r != 0 {
                visit(r);
            }
        }
    }

    /// Wholesale clear (between-app-run reset). Zeroes every slot, not just
    /// `len` — the visitor must never see a stale ref regardless of caller
    /// ordering.
    pub fn reset(&mut self) {
        self.entries = [(0, 0); N];
        self.len = 0;
    }
}

/// Access a `static mut PtrMap` through a raw pointer without materializing
/// a reference to the static itself (`static_mut_refs`) or the inline
/// `*&raw` pattern (`clippy::deref_addrof`): call as
/// `map_mut(&raw mut MAP).upsert(..)`. Sound for the widget maps because
/// they are only touched from the single UI task.
#[inline]
pub unsafe fn map_mut<const N: usize>(map: *mut PtrMap<N>) -> &'static mut PtrMap<N> {
    unsafe { &mut *map }
}

/// Shared-access counterpart of [`map_mut`].
#[inline]
pub unsafe fn map_ref<const N: usize>(map: *const PtrMap<N>) -> &'static PtrMap<N> {
    unsafe { &*map }
}

/// Report a dropped registration ([`Upsert::Full`]). A full map means clicks
/// or callbacks silently die, so this must be loud on both targets. The
/// defmt arm is compiled out of host-test builds (this file is re-included
/// via a `#[cfg(test)] #[path]` alias in main.rs, where no defmt global
/// logger exists to link against).
#[cfg_attr(test, allow(unused_variables))]
pub fn warn_full(map_name: &'static str) {
    #[cfg(feature = "sim")]
    println!("[lvgl] listener map '{map_name}' full — registration dropped");
    #[cfg(all(not(feature = "sim"), not(test)))]
    defmt::warn!(
        "lvgl: listener map '{=str}' full — registration dropped",
        map_name
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs<const N: usize>(m: &PtrMap<N>) -> Vec<u16> {
        let mut v = Vec::new();
        m.visit(&mut |r| v.push(r));
        v.sort_unstable();
        v
    }

    #[test]
    fn upsert_insert_update_full() {
        let mut m: PtrMap<2> = PtrMap::new();
        assert!(matches!(m.upsert(0x10, 1), Upsert::Inserted));
        assert!(matches!(m.upsert(0x20, 2), Upsert::Inserted));
        assert!(matches!(m.upsert(0x10, 3), Upsert::Updated));
        assert_eq!(m.lookup(0x10), Some(3));
        assert!(matches!(m.upsert(0x30, 4), Upsert::Full));
        assert_eq!(m.lookup(0x30), None);
    }

    #[test]
    fn remove_zeroes_vacated_tail_slot() {
        let mut m: PtrMap<4> = PtrMap::new();
        let _ = m.upsert(0x10, 1);
        let _ = m.upsert(0x20, 2);
        let _ = m.upsert(0x30, 3);
        m.remove(0x10); // tail (0x30) swaps into slot 0; old tail slot zeroed
        assert_eq!(m.lookup(0x30), Some(3));
        assert_eq!(m.lookup(0x10), None);
        assert_eq!(refs(&m), vec![2, 3]);
        // The vacated slot must not resurface as a phantom root.
        assert_eq!(m.entries[2], (0, 0));
    }

    #[test]
    fn remove_absent_is_noop_and_reuse_reinserts() {
        let mut m: PtrMap<2> = PtrMap::new();
        let _ = m.upsert(0x10, 1);
        m.remove(0x99);
        assert_eq!(m.lookup(0x10), Some(1));
        // Delete then reuse of the same address takes the Inserted path
        // (callbacks get re-attached by the caller).
        m.remove(0x10);
        assert!(matches!(m.upsert(0x10, 7), Upsert::Inserted));
        assert_eq!(m.lookup(0x10), Some(7));
    }

    #[test]
    fn reset_clears_every_slot() {
        let mut m: PtrMap<2> = PtrMap::new();
        let _ = m.upsert(0x10, 1);
        let _ = m.upsert(0x20, 2);
        m.reset();
        assert_eq!(refs(&m), Vec::<u16>::new());
        assert_eq!(m.entries, [(0, 0); 2]);
    }
}
