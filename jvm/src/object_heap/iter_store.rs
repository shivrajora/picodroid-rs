// SPDX-License-Identifier: GPL-3.0-only
use super::ObjectHeap;

/// Source collection type for an iterator.
pub enum IterSource {
    /// Iterating over an ArrayList's list_buf at this index.
    List(u16),
    /// Iterating over a HashMap's map_buf keys at this index.
    MapKeys(u16),
    /// Iterating over a HashMap's map_buf values at this index.
    MapValues(u16),
}

/// State for a live Java Iterator object.
pub struct IteratorState {
    pub source: IterSource,
    pub position: usize,
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
