// SPDX-License-Identifier: GPL-3.0-only
use alloc::vec::Vec;

use super::{float_to_str_buf, int_to_decimal_buf, long_to_decimal_buf, ObjectHeap};

impl ObjectHeap {
    // ── StringBuilder / sb_bufs ──────────────────────────────────────────────
    //
    // One buffer per StringBuilder instance, addressed by a slot index the
    // instance keeps in field 0 — the same shape `list_bufs` uses for
    // ArrayList. Until 2026-08 this was a single LIFO stack shared by every
    // builder, which silently interleaved two concurrently-alive builders
    // (`a.append(x); b.append(y)` both landed in `b`) and aliased across
    // threads. Keying on the instance is what makes each builder independent.

    /// Allocate a buffer for a new StringBuilder, returning its slot index.
    /// Reuses a `None` slot (freed by GC) before growing the backing Vec.
    pub fn sb_alloc(&mut self) -> Option<u16> {
        if let Some(idx) = self.sb_bufs.iter().position(|s| s.is_none()) {
            self.sb_bufs[idx] = Some(Vec::new());
            return Some(idx as u16);
        }
        let idx = self.sb_bufs.len() as u16;
        self.sb_bufs.push(Some(Vec::new()));
        Some(idx)
    }

    /// Free a StringBuilder buffer slot (GC hook). No-op if `idx` is out of range.
    pub fn sb_free(&mut self, idx: u16) {
        if let Some(slot) = self.sb_bufs.get_mut(idx as usize) {
            *slot = None;
        }
    }

    /// Append raw bytes to the buffer at `idx`.
    pub fn sb_append_bytes(&mut self, idx: u16, bytes: &[u8]) {
        if let Some(Some(buf)) = self.sb_bufs.get_mut(idx as usize) {
            buf.extend_from_slice(bytes);
        }
    }

    /// Append an integer in decimal to the buffer at `idx`.
    pub fn sb_append_int(&mut self, idx: u16, n: i32) {
        let mut tmp = [0u8; 12];
        let s = int_to_decimal_buf(n, &mut tmp);
        self.sb_append_bytes(idx, s);
    }

    /// Append a long in decimal to the buffer at `idx`.
    pub fn sb_append_long(&mut self, idx: u16, n: i64) {
        let mut tmp = [0u8; 21];
        let s = long_to_decimal_buf(n, &mut tmp);
        self.sb_append_bytes(idx, s);
    }

    /// Append a float to the buffer at `idx`.
    /// Formats as `[-]integer.fraction` with up to 6 significant decimal digits.
    pub fn sb_append_float(&mut self, idx: u16, f: f32) {
        let mut tmp = [0u8; 32];
        let s = float_to_str_buf(f, &mut tmp);
        self.sb_append_bytes(idx, s);
    }

    /// Current length in bytes of the buffer at `idx`.
    pub fn sb_len(&self, idx: u16) -> usize {
        self.sb_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map_or(0, |b| b.len())
    }

    /// Byte at `pos` in the buffer at `idx`, or `None` if out of bounds.
    pub fn sb_char_at(&self, idx: u16, pos: usize) -> Option<u8> {
        self.sb_bufs.get(idx as usize)?.as_ref()?.get(pos).copied()
    }

    /// Contents of the buffer at `idx` as a byte slice.
    pub fn sb_contents_slice(&self, idx: u16) -> &[u8] {
        self.sb_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map_or(&[], |b| b.as_slice())
    }
}
