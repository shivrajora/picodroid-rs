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
//! into the environment by `scripts/lib.sh::resolve_board`. See
//! `platforms/rp/build.rs::emit_jvm_config` for the schema, ranges, and
//! per-knob CPU↔memory trade-off documentation.

include!(concat!(env!("OUT_DIR"), "/tunables.rs"));

// Defence-in-depth range checks against a corrupted generated file.
const _: () = assert!(GC_THRESHOLD >= 16, "GC_THRESHOLD must be >= 16");
const _: () = assert!(
    CHUNK_SHIFT >= 3 && CHUNK_SHIFT <= 8,
    "CHUNK_SHIFT must be 3..=8"
);
const _: () = assert!(INLINE_DATA <= 32, "INLINE_DATA must be <= 32");
