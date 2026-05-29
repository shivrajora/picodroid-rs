// SPDX-License-Identifier: GPL-3.0-only
//! Single source of truth for the five `[jvm]` board.toml tunables — default
//! values and accepted ranges. Included via `#[path = "..."]` from both
//! `jvm/build.rs` (for the three JVM-crate knobs) and
//! `platforms/rp/build.rs::emit_jvm_config` (for all five, including the two
//! platform-side knobs).
//!
//! Centralising these prevents the two build scripts from drifting — a regression
//! that would silently mismatch what the JVM compiles with and what the platform
//! layer thinks it told the JVM. See
//! `website/src/content/docs/reference/jvm-tunables.md` for the canonical guide.

// `jvm/build.rs` only reads the three JVM-side constants; suppress the
// resulting "unused const" warnings on the two platform-side ones when
// included from that build script.
#![allow(dead_code)]

/// Default + accepted range for one `[jvm]` tunable.
pub struct JvmTunable {
    pub default: u32,
    pub min: u32,
    pub max: u32,
}

impl JvmTunable {
    pub const fn new(default: u32, min: u32, max: u32) -> Self {
        Self { default, min, max }
    }
}

// JVM-crate knobs (consumed by jvm/build.rs).
pub const GC_ALLOC_THRESHOLD: JvmTunable = JvmTunable::new(256, 16, 8192);
pub const SLOT_CHUNK_SHIFT: JvmTunable = JvmTunable::new(6, 3, 8);
pub const INLINE_ARRAY_DATA: JvmTunable = JvmTunable::new(8, 0, 32);

// Platform-side knobs (consumed by platforms/rp/build.rs::emit_jvm_config).
pub const ACTIVITY_STACK_DEPTH: JvmTunable = JvmTunable::new(8, 1, 32);
pub const PENDING_OP_QUEUE: JvmTunable = JvmTunable::new(8, 1, 64);
