// SPDX-License-Identifier: GPL-3.0-only
//! Native storage for the `JSONObject` / `JSONArray` SDK classes
//! (android-parity roadmap T2.6).
//!
//! The tree, parser and serializer are the `pd-json` crate, re-exported here
//! unchanged. What this module adds is the one global [`pool::Pool`] the
//! natives share and the scheduler-atomic section that guards it. Native code
//! never holds a JVM reference, so the pool needs no GC-root provider — what
//! it needs instead is to learn when a wrapper dies, which is the
//! `native_state_prune` hook: see the `pd_json::pool` docs.
//!
//! Gated per board by the `has_json` key in board.toml (`cfg(has_json)`);
//! a board that leaves it off also drops the SDK classes from its embedded
//! framework (`build_support/board_cfg.rs`), so JSON costs it nothing.
//!
//! The JVM-facing arms live in `native_handler/json.rs`.

pub use pd_json::{
    parse, serialize, Node, NodeIdx, K_ARRAY, K_BOOL, K_DOUBLE, K_INT, K_LONG, K_NULL, K_OBJECT,
    K_STRING, MAX_DEPTH,
};

pub mod pool {
    //! The global pool. JSON natives run on any JVM task, so every access
    //! goes through [`with_pool`], which holds an `AtomicSection` (scheduler
    //! suspended) for the duration — the same discipline as `monitor_store`.

    use core::cell::UnsafeCell;

    use pico_jvm::atomic_section::AtomicSection;

    pub use pd_json::pool::*;

    struct PoolCell(UnsafeCell<Pool>);

    // SAFETY: every access goes through `with_pool`, which holds an
    // `AtomicSection` for the whole closure — see the module docs.
    unsafe impl Sync for PoolCell {}

    static POOL: PoolCell = PoolCell(UnsafeCell::new(Pool::new()));

    /// Run `f` against the global pool inside a scheduler-atomic section.
    /// Never nest calls: the closure holds the one `&mut`.
    pub fn with_pool<R>(f: impl FnOnce(&mut Pool) -> R) -> R {
        let _atomic = AtomicSection::enter();
        // SAFETY: the section keeps every other JVM task off the CPU, and
        // callers never nest `with_pool`, so this is the only live reference.
        let pool = unsafe { &mut *POOL.0.get() };
        f(pool)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::json::{Node, K_INT};

        #[test]
        fn global_pool_round_trips() {
            with_pool(|p| p.clear());
            let n = with_pool(|p| p.alloc(Node::Int(5)).unwrap());
            assert_eq!(with_pool(|p| p.kind(n)), K_INT);
            with_pool(|p| p.clear());
        }
    }
}
