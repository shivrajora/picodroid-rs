// SPDX-License-Identifier: GPL-3.0-only
use super::ObjectHeap;

/// Source collection type for an iterator.
#[derive(Clone, Copy)]
pub enum IterSource {
    /// Iterating over an ArrayList's list_buf at this index.
    List(u16),
    /// Iterating over a HashMap's map_buf keys at this index.
    MapKeys(u16),
    /// Iterating over a HashMap's map_buf values at this index.
    MapValues(u16),
    /// Iterating over a HashMap's map_buf entries at this index, each
    /// yielded as a fresh two-field `java/util/Map$Entry` (key, value).
    MapEntries(u16),
}

/// State for a live Java Iterator object.
pub struct IteratorState {
    pub source: IterSource,
    pub position: usize,
    /// The collection (or map view) object this iterator was taken from.
    /// The GC marks it while the iterator is live, so `for (x in temp())`
    /// keeps the temporary's buffer — and the references inside it — alive
    /// for the whole loop even though nothing else holds the collection.
    pub owner: u16,
    /// Backing length at creation, maintained by this iterator's own
    /// `remove()`. java.util iterators are fail-fast: `next()` compares this
    /// against the live length and throws ConcurrentModificationException on
    /// a mismatch — without it, mutating the source mid-loop silently
    /// skipped or repeated elements (bugbash S6).
    pub expected_len: usize,
    /// Index of the element the last `next()` returned — what `remove()`
    /// removes. `None` before the first `next()` and after each `remove()`
    /// (Java's IllegalStateException contract).
    pub last_returned: Option<usize>,
}

impl ObjectHeap {
    // ── Iterator state ──────────────────────────────────────────────────────

    /// Associate an iterator state with an existing heap object.
    pub fn iter_register(&mut self, obj_idx: u16, state: IteratorState) {
        self.iter_states.push((obj_idx, state));
    }

    /// Look up the iterator state for an object, if any.
    pub fn iter_get(&self, obj_idx: u16) -> Option<&IteratorState> {
        self.iter_states
            .iter()
            .find(|(idx, _)| *idx == obj_idx)
            .map(|(_, state)| state)
    }

    /// Look up the iterator state mutably (to advance position).
    pub fn iter_get_mut(&mut self, obj_idx: u16) -> Option<&mut IteratorState> {
        self.iter_states
            .iter_mut()
            .find(|(idx, _)| *idx == obj_idx)
            .map(|(_, state)| state)
    }

    /// Remove the iterator state for an object (called from GC sweep).
    pub fn iter_free(&mut self, obj_idx: u16) {
        self.iter_states.retain(|(idx, _)| *idx != obj_idx);
    }
}
