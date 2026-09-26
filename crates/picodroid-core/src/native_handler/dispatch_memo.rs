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
//! (claudeusage D4). This table remembers, per site, the module that
//! claimed it or that none did, so the second call goes straight to that
//! module or straight back to the builtins.
//!
//! A row is keyed by the interpreter's [`SiteKey`] for the invoke — the
//! constant-pool site and the receiver's class, which between them fix the
//! `(class, method)` pair the handler is asked about — plus the superclass
//! step the interpreter is re-walking (it asks about the receiver's own
//! class first, then each superclass in turn under the same site). Two
//! `u32`s and two bytes, the same on every target; the keys used to be the
//! names' addresses, 24 B a row on the 64-bit simulator against 16 on the
//! device (M8, docs/parity-audit.md). A call with no site — a handler
//! driven outside the interpreter, or an upcall keyed by a hash — takes the
//! full walk, as every call did before the memo existed.
//!
//! Class indices and heap class ids are per app run, so `boot` bumps
//! [`next_app_generation`] per run and a memo stamped with an older
//! generation empties itself on its next lookup (the background workers'
//! handlers outlive a run).
//!
//! Semantics are unchanged because no two modules claim the same pair: the
//! `method_tables.rs` cross-check rejects a row listed under two modules, so
//! trying the remembered module first cannot pre-empt an earlier one. A
//! remembered module that declines after all (a guard it did not have when
//! the row was written) falls back to the full walk, which rewrites the row.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicU32, Ordering};
use pico_jvm::SiteKey;

/// Rows for the main handler: direct-mapped, a UI build step touches a few
/// dozen distinct pairs. Allocated once per handler on the heap (768 B):
/// the Java-thread handlers live on 8 KB task stacks.
pub(super) const MAIN_ROWS: usize = 64;
/// Rows for a Java thread's or a pool worker's handler: those dispatch a
/// handful of natives (a poll loop, a preference write), and an app with two
/// threads and four workers was paying 6 KB for six main-sized tables
/// (claudeusage gaps roadmap H7). A miss costs the chain walk, nothing else.
pub(super) const WORKER_ROWS: usize = 16;

/// "No module claimed this pair": the builtins or `NoSuchMethod` follow.
pub(super) const NONE: u8 = u8::MAX;

#[derive(Clone, Copy)]
struct Row {
    site: u32,
    recv: u32,
    depth: u8,
    module: u8,
}

// Loosen only with a parity-audit update: the same bytes on every target.
const _: () = assert!(core::mem::size_of::<Row>() == 12);

/// `site == 0` is an empty row: no site the interpreter builds is 0.
const EMPTY: Row = Row {
    site: 0,
    recv: 0,
    depth: 0,
    module: NONE,
};

/// Bumped once per app run by `boot`, before that run's handler exists.
static GENERATION: AtomicU32 = AtomicU32::new(0);

/// A new app run is starting: every memo filled before this is stale.
pub fn next_app_generation() {
    // A load and a store rather than `fetch_add`: the Cortex-M0+ has no
    // atomic read-modify-write, and `boot` is the one writer (as
    // `packages::RUN_GENERATION`).
    let next = GENERATION.load(Ordering::Relaxed).wrapping_add(1);
    GENERATION.store(next, Ordering::Relaxed);
}

pub(super) struct DispatchMemo {
    /// `None` when the heap refused the table: dispatch then walks the chain
    /// every time, as it did before the memo existed.
    rows: Option<Box<[Row]>>,
    /// The app run the rows belong to.
    generation: u32,
}

impl DispatchMemo {
    /// `rows` is a power of two: the slot index is a mask.
    pub(super) fn new(rows: usize) -> Self {
        debug_assert!(rows.is_power_of_two());
        let mut v = alloc::vec::Vec::new();
        let rows = if v.try_reserve_exact(rows).is_ok() {
            v.resize(rows, EMPTY);
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
    fn slot(rows: usize, key: SiteKey, depth: u8) -> usize {
        let mut h = key.site.wrapping_mul(0x9E37_79B1);
        h ^= (key.recv ^ ((depth as u32) << 24))
            .rotate_left(13)
            .wrapping_mul(0x85EB_CA77);
        h ^= h >> 15;
        (h.wrapping_mul(0x2C1B_3C6D) >> 16) as usize & (rows - 1)
    }

    /// The module remembered for `(key, depth)`: `Some(NONE)` for a pair
    /// no module claims, `None` for a pair not seen, evicted, or from an
    /// earlier app run.
    #[inline]
    pub(super) fn get(&mut self, key: SiteKey, depth: u8) -> Option<u8> {
        let generation = GENERATION.load(Ordering::Relaxed);
        if generation != self.generation {
            self.generation = generation;
            if let Some(rows) = self.rows.as_mut() {
                rows.fill(EMPTY);
            }
            return None;
        }
        let rows = self.rows.as_ref()?;
        let row = &rows[Self::slot(rows.len(), key, depth)];
        let hit = row.site == key.site && row.recv == key.recv && row.depth == depth;
        hit.then_some(row.module)
    }

    #[inline]
    pub(super) fn set(&mut self, key: SiteKey, depth: u8, module: u8) {
        let Some(rows) = self.rows.as_mut() else {
            return;
        };
        let slot = Self::slot(rows.len(), key, depth);
        rows[slot] = Row {
            site: key.site,
            recv: key.recv,
            depth,
            module,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(ci: usize, cp: u16) -> SiteKey {
        SiteKey::cp(ci, cp)
    }

    #[test]
    fn remembers_by_site_receiver_and_depth() {
        let mut memo = DispatchMemo::new(MAIN_ROWS);
        let k = site(3, 17).with_recv(SiteKey::recv_object(4));
        assert_eq!(memo.get(k, 0), None);
        memo.set(k, 0, 5);
        assert_eq!(memo.get(k, 0), Some(5));
        // Another receiver class at the same site is another row.
        assert_eq!(memo.get(k.with_recv(SiteKey::recv_object(5)), 0), None);
        // A deeper superclass step is another row.
        assert_eq!(memo.get(k, 1), None);
        memo.set(k, 1, NONE);
        assert_eq!(memo.get(k, 1), Some(NONE));
        assert_eq!(memo.get(k, 0), Some(5));
        // The same CP index in another class is another site.
        assert_eq!(
            memo.get(site(4, 17).with_recv(SiteKey::recv_object(4)), 0),
            None
        );
    }

    #[test]
    fn a_new_app_run_empties_the_memo() {
        let mut memo = DispatchMemo::new(WORKER_ROWS);
        let k = site(1, 2);
        memo.set(k, 0, 3);
        assert_eq!(memo.get(k, 0), Some(3));
        next_app_generation();
        assert_eq!(memo.get(k, 0), None);
        memo.set(k, 0, 4);
        assert_eq!(memo.get(k, 0), Some(4));
    }

    #[test]
    fn eviction_is_silent() {
        let mut memo = DispatchMemo::new(WORKER_ROWS);
        let keys: Vec<SiteKey> = (0..200u16)
            .map(|i| site(usize::from(i % 7), 1 + i).with_recv(SiteKey::recv_object(i % 3)))
            .collect();
        for (i, k) in keys.iter().enumerate() {
            memo.set(*k, (i % 2) as u8, (i % 7) as u8);
        }
        // Every row either answers what was stored last for that key or
        // nothing; never another key's module.
        for (i, k) in keys.iter().enumerate() {
            if let Some(m) = memo.get(*k, (i % 2) as u8) {
                assert_eq!(m, (i % 7) as u8);
            }
        }
    }

    #[test]
    fn a_refused_table_answers_nothing() {
        let mut memo = DispatchMemo {
            rows: None,
            generation: GENERATION.load(Ordering::Relaxed),
        };
        let k = site(1, 1);
        memo.set(k, 0, 2);
        assert_eq!(memo.get(k, 0), None);
    }
}
