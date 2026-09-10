// SPDX-License-Identifier: GPL-3.0-only
//! Build-time JVM tunables.
//!
//! Each constant is sourced from a `PICODROID_JVM_*` env var by
//! [`build.rs`](../../build.rs) and written into `$OUT_DIR/tunables.rs`,
//! which is `include!`'d below. Defaults reproduce the original hardcoded
//! values, so any build that does not set the env vars (e.g. `cargo build`
//! invoked directly without the picodroid wrapper scripts) compiles
//! bit-for-bit identically to the pre-tunables crate.
//!
//! The values come from each board's `board.toml` `[jvm]` section, exported
//! into the environment by [`scripts/lib.sh::apply_jvm_env`]. Per-const
//! purpose, range, and trade-off summaries live on each `pub const` below
//! (also visible in `cargo doc`).
//!
//! Canonical guide — schema table, tuning workflow with `perfbench`, and
//! worked recipes for heap- vs CPU-constrained boards:
//! <https://shivrajora.github.io/picodroid-rs/reference/jvm-tunables/>
//!
//! [`scripts/lib.sh::apply_jvm_env`]: https://github.com/shivrajora/picodroid-rs/blob/main/scripts/lib.sh

include!(concat!(env!("OUT_DIR"), "/tunables.rs"));

// Defence-in-depth range checks against a corrupted generated file.
const _: () = assert!(GC_THRESHOLD >= 16, "GC_THRESHOLD must be >= 16");
const _: () = assert!(
    CHUNK_SHIFT >= 3 && CHUNK_SHIFT <= 8,
    "CHUNK_SHIFT must be 3..=8"
);
const _: () = assert!(INLINE_DATA <= 32, "INLINE_DATA must be <= 32");

#[cfg(test)]
mod tests {
    //! End-to-end propagation guards for `jvm/build.rs`.
    //!
    //! Each test asserts that when no `PICODROID_JVM_*` override was set at
    //! build time (the normal case for `./scripts/test.sh`, which runs through
    //! a board with no `[jvm]` block), the compiled `pub const` equals the
    //! documented default. Catches:
    //!   * Silent regressions where `jvm/build.rs` emits the wrong literal.
    //!   * Default drift between `build_support/jvm_defaults.rs` and the
    //!     documented value in the rustdoc / markdown guide.
    //!   * Off-by-one in `read_env_u32`'s `Err(_) => return spec.default` path.
    //!
    //! When an override *is* active at compile time (e.g. a board's `[jvm]`
    //! block exported a `PICODROID_JVM_*` value via `scripts/lib.sh`), the
    //! test early-returns: the override path is exercised by the integration
    //! flow (`./scripts/sim.sh --app perfbench --board <override-board>`),
    //! not by these unit tests.

    use super::*;

    #[test]
    fn default_gc_threshold_is_256() {
        if option_env!("PICODROID_JVM_GC_ALLOC_THRESHOLD").is_some() {
            return;
        }
        assert_eq!(GC_THRESHOLD, 256);
    }

    #[test]
    fn default_chunk_shift_is_6_with_derived_size_and_mask() {
        if option_env!("PICODROID_JVM_SLOT_CHUNK_SHIFT").is_some() {
            return;
        }
        assert_eq!(CHUNK_SHIFT, 6);
        assert_eq!(CHUNK_SIZE, 64);
        assert_eq!(CHUNK_MASK, 63);
    }

    #[test]
    fn default_inline_data_is_8() {
        if option_env!("PICODROID_JVM_INLINE_ARRAY_DATA").is_some() {
            return;
        }
        assert_eq!(INLINE_DATA, 8);
    }
}
