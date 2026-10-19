// SPDX-License-Identifier: GPL-3.0-only
use alloc::vec::Vec;

use super::{
    double_to_str_buf, float_to_str_buf, int_to_decimal_buf, long_to_decimal_buf, reserve_fallible,
    Exhausted, ObjectHeap,
};

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
        if self.sb_bufs.len() >= crate::heap::REF_TOP {
            return None;
        }
        let idx = self.sb_bufs.len() as u16;
        reserve_fallible(&mut self.sb_bufs, 1).ok()?;
        self.sb_bufs.push(Some(Vec::new()));
        Some(idx)
    }

    /// Free a StringBuilder buffer slot (GC hook). No-op if `idx` is out of range.
    pub fn sb_free(&mut self, idx: u16) {
        if let Some(slot) = self.sb_bufs.get_mut(idx as usize) {
            *slot = None;
        }
    }

    /// Append raw bytes to the buffer at `idx`. [`Exhausted`] when the
    /// buffer cannot grow; nothing is appended then.
    pub fn sb_append_bytes(&mut self, idx: u16, bytes: &[u8]) -> Result<(), Exhausted> {
        if let Some(Some(buf)) = self.sb_bufs.get_mut(idx as usize) {
            reserve_fallible(buf, bytes.len())?;
            buf.extend_from_slice(bytes);
        }
        Ok(())
    }

    /// Append an integer in decimal to the buffer at `idx`.
    pub fn sb_append_int(&mut self, idx: u16, n: i32) -> Result<(), Exhausted> {
        let mut tmp = [0u8; 12];
        let s = int_to_decimal_buf(n, &mut tmp);
        self.sb_append_bytes(idx, s)
    }

    /// Append a long in decimal to the buffer at `idx`.
    pub fn sb_append_long(&mut self, idx: u16, n: i64) -> Result<(), Exhausted> {
        let mut tmp = [0u8; 21];
        let s = long_to_decimal_buf(n, &mut tmp);
        self.sb_append_bytes(idx, s)
    }

    /// Append a float to the buffer at `idx` (`Float.toString` layout).
    pub fn sb_append_float(&mut self, idx: u16, f: f32) -> Result<(), Exhausted> {
        let mut tmp = [0u8; 32];
        let s = float_to_str_buf(f, &mut tmp);
        self.sb_append_bytes(idx, s)
    }

    /// Append a double to the buffer at `idx` (`Double.toString` layout).
    pub fn sb_append_double(&mut self, idx: u16, d: f64) -> Result<(), Exhausted> {
        let mut tmp = [0u8; 32];
        let s = double_to_str_buf(d, &mut tmp);
        self.sb_append_bytes(idx, s)
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

    /// Insert `bytes` at byte offset `pos` of the buffer at `idx`:
    /// `Ok(false)` when `pos` is past the end (Java's
    /// `StringIndexOutOfBoundsException`), [`Exhausted`] when the buffer
    /// cannot grow; nothing is changed either way.
    pub fn sb_insert_bytes(
        &mut self,
        idx: u16,
        pos: usize,
        bytes: &[u8],
    ) -> Result<bool, Exhausted> {
        let Some(Some(buf)) = self.sb_bufs.get_mut(idx as usize) else {
            return Ok(true);
        };
        if pos > buf.len() {
            return Ok(false);
        }
        reserve_fallible(buf, bytes.len())?;
        let tail: Vec<u8> = buf.split_off(pos);
        buf.extend_from_slice(bytes);
        buf.extend_from_slice(&tail);
        Ok(true)
    }

    /// `setLength`: truncate the buffer at `idx` to `len` bytes, or pad it
    /// with NUL bytes up to `len`, as Java does.
    pub fn sb_set_len(&mut self, idx: u16, len: usize) -> Result<(), Exhausted> {
        if let Some(Some(buf)) = self.sb_bufs.get_mut(idx as usize) {
            if len > buf.len() {
                reserve_fallible(buf, len - buf.len())?;
            }
            buf.resize(len, 0);
        }
        Ok(())
    }

    /// `delete(start, end)`: remove bytes `start..min(end, len)`; `false`
    /// when `start` is past the end or past `end` (Java's bounds error),
    /// nothing removed then.
    pub fn sb_delete(&mut self, idx: u16, start: usize, end: usize) -> bool {
        let Some(Some(buf)) = self.sb_bufs.get_mut(idx as usize) else {
            return true;
        };
        let end = end.min(buf.len());
        if start > end {
            return false;
        }
        buf.drain(start..end);
        true
    }

    /// `setCharAt`: overwrite the byte at `pos`; `false` when out of bounds.
    pub fn sb_set_byte(&mut self, idx: u16, pos: usize, b: u8) -> bool {
        match self
            .sb_bufs
            .get_mut(idx as usize)
            .and_then(|s| s.as_mut())
            .and_then(|buf| buf.get_mut(pos))
        {
            Some(slot) => {
                *slot = b;
                true
            }
            None => false,
        }
    }

    /// `reverse`: the bytes in reverse order (a multi-byte character comes
    /// out byte-reversed, as a surrogate pair would on Android without the
    /// pair fix-up; the string model is bytes).
    pub fn sb_reverse(&mut self, idx: u16) {
        if let Some(Some(buf)) = self.sb_bufs.get_mut(idx as usize) {
            buf.reverse();
        }
    }

    /// Contents of the buffer at `idx` as a byte slice.
    pub fn sb_contents_slice(&self, idx: u16) -> &[u8] {
        self.sb_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map_or(&[], |b| b.as_slice())
    }
}
