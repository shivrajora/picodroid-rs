// SPDX-License-Identifier: GPL-3.0-only
//! Fuzz targets for every parser that takes untrusted bytes
//! (docs/designs/fuzzing-2026-10.md).
//!
//! Each target is a plain `fn(&[u8])` that panics on a finding. Two drivers
//! call them:
//!
//! - `fuzz/` (cargo-fuzz, nightly) wraps each one in `fuzz_target!` and lets
//!   libFuzzer search; `scripts/fuzz.sh` runs it by hand and every night.
//! - `tests/replay.rs` feeds every file under `fuzz/regress/<target>/` —
//!   seeds and minimized reproducers of past findings — through the same
//!   function on the pinned stable toolchain, in `scripts/test.sh` and so in
//!   CI. A fixed finding stays fixed without nightly anywhere near CI.
//!
//! "Does not panic" is the floor, not the point. The bugs that matter in a
//! parser that guards the device are acceptances: an image that passes the
//! install-time check and then trips something that trusted the check. So
//! most targets assert a property that ties the check to what is later read
//! without one, or a round trip through the writer.

pub mod budget;
pub mod class;
pub mod http;
pub mod install;
pub mod interp;
pub mod json;
pub mod papk;
pub mod pdb;
pub mod records;
pub mod tls;

/// A target: any bytes in, a panic on a finding.
pub type Target = fn(&[u8]);

/// Every target, by the name its `fuzz/fuzz_targets/<name>.rs` shim and its
/// `fuzz/regress/<name>/` directory carry.
pub const TARGETS: &[(&str, Target)] = &[
    ("papk", papk::papk),
    ("papk_prefix", papk::papk_prefix),
    ("papk_roundtrip", papk::papk_roundtrip),
    ("class_section", class::class_section),
    ("link_class", class::link_class),
    ("bytecode_check", class::bytecode_check),
    ("interp", interp::interp),
    ("pdb_decode", pdb::pdb_decode),
    ("install", install::install),
    ("http_head", http::http_head),
    ("http_chunked", http::http_chunked),
    ("json", json::json),
    ("der", tls::der),
    ("tls_server", tls::tls_server),
    ("records", records::records),
];

/// A cursor that turns fuzz bytes into structured choices, for the targets
/// that build an input rather than parse one. Running out is not an error:
/// every read past the end yields zeros, so a short input is still a valid
/// (small) structure and libFuzzer's mutations stay meaningful.
pub struct Bytes<'a> {
    data: &'a [u8],
}

impl<'a> Bytes<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn u8(&mut self) -> u8 {
        match self.data.split_first() {
            Some((&b, rest)) => {
                self.data = rest;
                b
            }
            None => 0,
        }
    }

    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.u8(), self.u8()])
    }

    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes([self.u8(), self.u8(), self.u8(), self.u8()])
    }

    /// A number in `0..n` (`0` when `n` is 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            self.u16() as usize % n
        }
    }

    /// Up to `max` bytes, the length chosen by the input.
    pub fn take(&mut self, max: usize) -> &'a [u8] {
        let n = self.below(max + 1).min(self.data.len());
        let (head, rest) = self.data.split_at(n);
        self.data = rest;
        head
    }

    /// Everything left.
    pub fn rest(&mut self) -> &'a [u8] {
        core::mem::take(&mut self.data)
    }
}
