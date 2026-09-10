// SPDX-License-Identifier: GPL-3.0-only
//! Native storage for the `JSONObject` / `JSONArray` SDK classes
//! (android-parity roadmap T2.6).
//!
//! A parsed document is a tree of [`Node`]s in one global [`pool::Pool`];
//! the Java wrappers hold nothing but an `int` node index, and every leaf
//! value is materialized into a Java object only on `get`. Native code
//! therefore never holds a JVM reference, so the pool needs no GC-root
//! provider — what it needs instead is to learn when a wrapper dies, which
//! is the `native_state_prune` hook: see the pool docs.
//!
//! Gated per board by the `has_json` key in board.toml (`cfg(has_json)`);
//! a board that leaves it off also drops the SDK classes from its embedded
//! framework (`build_support/board_cfg.rs`), so JSON costs it nothing.
//!
//! Everything here is pure `alloc` code with host unit tests; the JVM-facing
//! arms live in `native_handler/json.rs`.

pub mod parse;
pub mod pool;
pub mod serialize;

use alloc::vec::Vec;

/// Index of a node in the pool. `u16` like every other heap index in the
/// JVM; the pool caps itself well below that.
pub type NodeIdx = u16;

// Value kinds as the Java side sees them (`JSONObject.K_*` — keep in step).
pub const K_NULL: i32 = 0;
pub const K_BOOL: i32 = 1;
pub const K_INT: i32 = 2;
pub const K_LONG: i32 = 3;
pub const K_DOUBLE: i32 = 4;
pub const K_STRING: i32 = 5;
pub const K_OBJECT: i32 = 6;
pub const K_ARRAY: i32 = 7;

/// Nesting cap for both the parser and the serializer: at most this many
/// containers on any root-to-leaf path. A document is a tree of arbitrary
/// depth in principle; on a 16 KiB task stack it is not.
pub const MAX_DEPTH: usize = 32;

/// One JSON value. Object entries keep insertion order (Android's
/// `JSONObject` is a `LinkedHashMap`), and numbers keep the Java type the
/// parser assigned (`Integer` when it fits, else `Long`, else `Double`).
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Null,
    Bool(bool),
    Int(i32),
    Long(i64),
    Double(f64),
    Str(Vec<u8>),
    Object(Vec<(Vec<u8>, NodeIdx)>),
    Array(Vec<NodeIdx>),
}

impl Node {
    pub fn kind(&self) -> i32 {
        match self {
            Node::Null => K_NULL,
            Node::Bool(_) => K_BOOL,
            Node::Int(_) => K_INT,
            Node::Long(_) => K_LONG,
            Node::Double(_) => K_DOUBLE,
            Node::Str(_) => K_STRING,
            Node::Object(_) => K_OBJECT,
            Node::Array(_) => K_ARRAY,
        }
    }

    /// Bytes this node charges against the pool's payload budget: string
    /// contents and object keys. Kept in step by `Pool::object_put` /
    /// `object_remove`, which are the only places keys come and go.
    pub fn payload_bytes(&self) -> usize {
        match self {
            Node::Str(s) => s.len(),
            Node::Object(entries) => entries.iter().map(|(k, _)| k.len()).sum(),
            _ => 0,
        }
    }
}
