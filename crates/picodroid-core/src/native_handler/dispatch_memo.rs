// SPDX-License-Identifier: GPL-3.0-only
//! Which sub-dispatcher serves a native, remembered per call site.
//!
//! `PicodroidNativeHandler::dispatch` offers every native to a chain of a
//! dozen module dispatchers, each a `match` over `(class, method)` string
//! pairs, and then to its own arms. A `TextView.setText` therefore compares
//! its names against several hundred arms before the graphics module claims
//! it, and a `String.length()` — served by the JVM's builtin handler *after*
//! this one declines — walks the whole chain every time. On the RP2350 that
//! walk, fetched from XIP flash, was a ~60 µs floor under every native call
//! (claudeusage D4). This table remembers, per `(class, method)` pair, the
//! module that claimed it or that none did, so the second call goes straight
//! to that module or straight back to the builtins.
//!
//! Keys are the names' addresses: the interpreter passes constant-pool bytes
//! of a loaded class or a `names::c` / `m` constant, fixed for as long as
//! the app runs. Two names at one address with one length are one name, so
//! a hit needs no byte comparison; what a hit cannot tell is an address
//! reused by the *next* app's class data, which only a handler outliving an
//! app run could see (the background workers loop across runs). `boot`
//! therefore bumps [`next_app_generation`] per run and a memo stamped with
//! an older generation empties itself on its next lookup.
//!
//! Semantics are unchanged because no two modules claim the same pair: the
//! `method_tables.rs` cross-check rejects a row listed under two modules, so
//! trying the remembered module first cannot pre-empt an earlier one. A
//! remembered module that declines after all (a guard it did not have when
//! the row was written) falls back to the full walk, which rewrites the row.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU32, Ordering};

/// Direct-mapped, `SLOTS` rows; a UI build step touches a few dozen distinct
/// pairs. Allocated once per handler on the heap (about 1 KB on a 32-bit
/// target): the Java-thread handlers live on 8 KB task stacks.
const SLOTS: usize = 64;

/// "No module claimed this pair": the builtins or `NoSuchMethod` follow.
pub(super) const NONE: u8 = u8::MAX;

#[derive(Clone, Copy)]
struct Row {
    class: *const u8,
    method: *const u8,
    class_len: u16,
    method_len: u16,
    module: u8,
}

const EMPTY: Row = Row {
    class: core::ptr::null(),
    method: core::ptr::null(),
    class_len: 0,
    method_len: 0,
    module: NONE,
};

/// Bumped once per app run by `boot`, before that run's handler exists.
static GENERATION: AtomicU32 = AtomicU32::new(0);

/// A new app run is starting: every memo filled before this is stale.
pub fn next_app_generation() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

pub(super) struct DispatchMemo {
    /// `None` when the heap refused the table: dispatch then walks the chain
    /// every time, as it did before the memo existed.
    rows: Option<Box<[Row]>>,
    /// The app run the rows belong to.
    generation: u32,
}

impl DispatchMemo {
    pub(super) fn new() -> Self {
        let mut v = alloc::vec::Vec::new();
        let rows = if v.try_reserve_exact(SLOTS).is_ok() {
            v.resize(SLOTS, EMPTY);
            Some(v.into_boxed_slice())
        } else {
            None
        };
        Self {
            rows,
            generation: GENERATION.load(Ordering::Relaxed),
        }
    }

    #[inline(always)]
    fn slot(class: &str, method: &str) -> usize {
        let c = class.as_ptr() as usize;
        let m = method.as_ptr() as usize;
        // The names live in flash or the class store, so the low bits are
        // alignment; the method's address spreads the class's.
        ((c >> 2) ^ (m >> 2).wrapping_mul(0x9E37_79B1usize)) & (SLOTS - 1)
    }

    /// The module remembered for `(class, method)`: `Some(NONE)` for a pair
    /// no module claims, `None` for a pair not seen, evicted, or from an
    /// earlier app run.
    #[inline]
    pub(super) fn get(&mut self, class: &str, method: &str) -> Option<u8> {
        let generation = GENERATION.load(Ordering::Relaxed);
        if generation != self.generation {
            self.generation = generation;
            if let Some(rows) = self.rows.as_mut() {
                rows.fill(EMPTY);
            }
            return None;
        }
        let row = &self.rows.as_ref()?[Self::slot(class, method)];
        let hit = core::ptr::eq(row.class, class.as_ptr())
            && core::ptr::eq(row.method, method.as_ptr())
            && row.class_len as usize == class.len()
            && row.method_len as usize == method.len();
        hit.then_some(row.module)
    }

    #[inline]
    pub(super) fn set(&mut self, class: &str, method: &str, module: u8) {
        let Some(rows) = self.rows.as_mut() else {
            return;
        };
        // A `&str` is at most as long as `u16` on the boards; a longer name
        // on the host would key a row it can never hit, which is harmless.
        let (Ok(class_len), Ok(method_len)) =
            (u16::try_from(class.len()), u16::try_from(method.len()))
        else {
            return;
        };
        rows[Self::slot(class, method)] = Row {
            class: class.as_ptr(),
            method: method.as_ptr(),
            class_len,
            method_len,
            module,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_by_address_and_forgets_on_reuse_of_length_only() {
        let mut memo = DispatchMemo::new();
        let class = "picodroid/widget/TextView";
        let method = "setText";
        assert_eq!(memo.get(class, method), None);
        memo.set(class, method, 5);
        assert_eq!(memo.get(class, method), Some(5));
        // Same bytes at another address: not the same site.
        let other = String::from(class);
        assert_eq!(memo.get(&other, method), None);
        memo.set(class, method, NONE);
        assert_eq!(memo.get(class, method), Some(NONE));
    }

    #[test]
    fn a_new_app_run_empties_the_memo() {
        let mut memo = DispatchMemo::new();
        memo.set("a/B", "m", 3);
        assert_eq!(memo.get("a/B", "m"), Some(3));
        next_app_generation();
        assert_eq!(memo.get("a/B", "m"), None);
        memo.set("a/B", "m", 4);
        assert_eq!(memo.get("a/B", "m"), Some(4));
    }

    #[test]
    fn eviction_is_silent() {
        let mut memo = DispatchMemo::new();
        let names: Vec<String> = (0..200).map(|i| format!("class/{i}")).collect();
        for (i, n) in names.iter().enumerate() {
            memo.set(n, "m", (i % 7) as u8);
        }
        // Every row either answers what was stored last for that pair or
        // nothing; never another pair's module.
        for (i, n) in names.iter().enumerate() {
            if let Some(m) = memo.get(n, "m") {
                assert_eq!(m, (i % 7) as u8);
            }
        }
    }
}
