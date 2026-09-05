// SPDX-License-Identifier: GPL-3.0-only
//! The global JSON node pool.
//!
//! # Ownership: wrappers are the roots
//!
//! Every live Java `JSONObject` / `JSONArray` is *bound* to a node: the
//! registry maps the wrapper's object-heap slot to its node index. A node is
//! live while it is reachable from a bound node (an object's entries, an
//! array's items) — that is what gives Android's identity semantics for
//! free: `parent.put("c", child)` links the child's node, and a later
//! `child.put(...)` is visible through the parent. The same node may hang
//! under two parents (a DAG); a node reaching itself is refused at link
//! time, since Android's own `toString` would overflow on it.
//!
//! # Slots are recycled
//!
//! The registry key is a heap slot index and the collector reuses slots, so
//! an entry left behind by a dead wrapper would be inherited by whatever
//! object lands in that slot next. The interpreter therefore calls
//! [`Pool::prune`] straight after every collection — through
//! `NativeMethodHandler::native_state_prune`, before any allocation can
//! recycle a slot — and the pool drops the dead wrappers' bindings, marks
//! from the survivors and sweeps every unmarked node. Nothing else frees a
//! node: `remove` merely unlinks it, so the wrapper a caller boxed *before*
//! unlinking keeps it alive (the `remove` Javadoc's ordering rule).
//!
//! # Budget
//!
//! Native `Vec`s are invisible to the JVM's allocation pacing, so the pool
//! caps itself ([`MAX_NODES`], [`MAX_PAYLOAD_BYTES`]) and a parse or put
//! that would exceed the cap fails cleanly instead of exhausting the RTOS
//! heap under the JVM.
//!
//! # Concurrency
//!
//! JSON natives run on any JVM task, so every access goes through
//! [`with_pool`], which holds an `AtomicSection` (scheduler suspended) for
//! the duration — the same discipline as `monitor_store`. Nothing inside
//! blocks; a parse of a few KB is comparable to one GC.

use alloc::{string::String, vec, vec::Vec};
use core::cell::UnsafeCell;

use pico_jvm::atomic_section::AtomicSection;

use super::{Node, NodeIdx};

/// Most live nodes at once. Bounds the node table (`Option<Node>` is a few
/// words on ARM) independently of string contents.
pub const MAX_NODES: usize = 2048;
/// Most string / key bytes held at once.
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolError {
    /// A cap was hit; the tree is unchanged.
    Exhausted,
    /// Linking the child would make the container reach itself.
    Cycle,
    /// The index names no node, or a node of the wrong kind.
    Invalid,
}

pub struct Pool {
    nodes: Vec<Option<Node>>,
    free: Vec<NodeIdx>,
    /// `(object-heap slot of the Java wrapper, node it is bound to)`.
    registry: Vec<(u16, NodeIdx)>,
    live: usize,
    payload: usize,
    last_error: Option<String>,
}

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

impl Default for Pool {
    fn default() -> Self {
        Self::new()
    }
}

impl Pool {
    pub const fn new() -> Self {
        Pool {
            nodes: Vec::new(),
            free: Vec::new(),
            registry: Vec::new(),
            live: 0,
            payload: 0,
            last_error: None,
        }
    }

    /// Store `node`, reusing a swept slot before growing the table.
    pub fn alloc(&mut self, node: Node) -> Result<NodeIdx, PoolError> {
        let bytes = node.payload_bytes();
        if self.live >= MAX_NODES || self.payload + bytes > MAX_PAYLOAD_BYTES {
            return Err(PoolError::Exhausted);
        }
        let idx = match self.free.pop() {
            Some(i) => {
                self.nodes[i as usize] = Some(node);
                i
            }
            None => {
                if self.nodes.len() >= u16::MAX as usize {
                    return Err(PoolError::Exhausted);
                }
                self.nodes.push(Some(node));
                (self.nodes.len() - 1) as NodeIdx
            }
        };
        self.live += 1;
        self.payload += bytes;
        Ok(idx)
    }

    /// Release one node (not its children). The parser uses it to roll back
    /// a failed document; everything else waits for [`Pool::prune`].
    pub fn free_node(&mut self, idx: NodeIdx) {
        if let Some(slot) = self.nodes.get_mut(idx as usize) {
            if let Some(node) = slot.take() {
                self.live -= 1;
                self.payload -= node.payload_bytes();
                self.free.push(idx);
            }
        }
    }

    pub fn get(&self, idx: NodeIdx) -> Option<&Node> {
        self.nodes.get(idx as usize).and_then(|s| s.as_ref())
    }

    pub fn get_mut(&mut self, idx: NodeIdx) -> Option<&mut Node> {
        self.nodes.get_mut(idx as usize).and_then(|s| s.as_mut())
    }

    /// The node's `K_*` kind, or `-1` for a dead index.
    pub fn kind(&self, idx: NodeIdx) -> i32 {
        self.get(idx).map(Node::kind).unwrap_or(-1)
    }

    /// Entry count of an object, item count of an array, `0` otherwise.
    pub fn length(&self, idx: NodeIdx) -> usize {
        match self.get(idx) {
            Some(Node::Object(entries)) => entries.len(),
            Some(Node::Array(items)) => items.len(),
            _ => 0,
        }
    }

    pub fn object_get(&self, obj: NodeIdx, key: &[u8]) -> Option<NodeIdx> {
        match self.get(obj) {
            Some(Node::Object(entries)) => entries
                .iter()
                .find(|(k, _)| k.as_slice() == key)
                .map(|(_, child)| *child),
            _ => None,
        }
    }

    pub fn key_at(&self, obj: NodeIdx, index: usize) -> Option<&[u8]> {
        match self.get(obj) {
            Some(Node::Object(entries)) => entries.get(index).map(|(k, _)| k.as_slice()),
            _ => None,
        }
    }

    pub fn array_get(&self, arr: NodeIdx, index: usize) -> Option<NodeIdx> {
        match self.get(arr) {
            Some(Node::Array(items)) => items.get(index).copied(),
            _ => None,
        }
    }

    /// Link `child` under `key`, replacing an existing entry in place (so
    /// key order is stable) or appending.
    pub fn object_put(
        &mut self,
        obj: NodeIdx,
        key: &[u8],
        child: NodeIdx,
    ) -> Result<(), PoolError> {
        self.check_link(obj, child)?;
        let existing = match self.get(obj) {
            Some(Node::Object(entries)) => entries.iter().position(|(k, _)| k.as_slice() == key),
            _ => return Err(PoolError::Invalid),
        };
        if existing.is_none() && self.payload + key.len() > MAX_PAYLOAD_BYTES {
            return Err(PoolError::Exhausted);
        }
        let Some(Node::Object(entries)) = self.get_mut(obj) else {
            return Err(PoolError::Invalid);
        };
        match existing {
            Some(pos) => entries[pos].1 = child,
            None => {
                entries.push((key.to_vec(), child));
                self.payload += key.len();
            }
        }
        Ok(())
    }

    /// Unlink `key`, returning the child it held (still allocated until the
    /// next prune, so a wrapper bound to it stays valid).
    pub fn object_remove(&mut self, obj: NodeIdx, key: &[u8]) -> Option<NodeIdx> {
        let Some(Node::Object(entries)) = self.get_mut(obj) else {
            return None;
        };
        let pos = entries.iter().position(|(k, _)| k.as_slice() == key)?;
        let (k, child) = entries.remove(pos);
        self.payload -= k.len();
        Some(child)
    }

    /// Put `child` at `index`; `None` appends; an index past the end pads
    /// with `Null` nodes first (Android's `JSONArray.put(int, Object)`).
    pub fn array_set(
        &mut self,
        arr: NodeIdx,
        index: Option<usize>,
        child: NodeIdx,
    ) -> Result<(), PoolError> {
        self.check_link(arr, child)?;
        let len = match self.get(arr) {
            Some(Node::Array(items)) => items.len(),
            _ => return Err(PoolError::Invalid),
        };
        let index = index.unwrap_or(len);
        let mut len = len;
        while len < index {
            let pad = self.alloc(Node::Null)?;
            let Some(Node::Array(items)) = self.get_mut(arr) else {
                return Err(PoolError::Invalid);
            };
            items.push(pad);
            len += 1;
        }
        let Some(Node::Array(items)) = self.get_mut(arr) else {
            return Err(PoolError::Invalid);
        };
        if index == items.len() {
            items.push(child);
        } else {
            items[index] = child;
        }
        Ok(())
    }

    /// Unlink the item at `index`, shifting the rest down; the child stays
    /// allocated until the next prune.
    pub fn array_remove(&mut self, arr: NodeIdx, index: usize) -> Option<NodeIdx> {
        let Some(Node::Array(items)) = self.get_mut(arr) else {
            return None;
        };
        if index >= items.len() {
            return None;
        }
        Some(items.remove(index))
    }

    fn check_link(&self, container: NodeIdx, child: NodeIdx) -> Result<(), PoolError> {
        if self.get(child).is_none() {
            return Err(PoolError::Invalid);
        }
        if child == container || self.reaches(child, container) {
            return Err(PoolError::Cycle);
        }
        Ok(())
    }

    /// Whether `target` is reachable from `from` through object entries and
    /// array items.
    pub fn reaches(&self, from: NodeIdx, target: NodeIdx) -> bool {
        let mut marks = vec![0u8; self.nodes.len().div_ceil(8)];
        let mut work: Vec<NodeIdx> = vec![from];
        while let Some(i) = work.pop() {
            if i == target {
                return true;
            }
            if mark(&mut marks, i) {
                continue;
            }
            self.push_children(i, &mut work);
        }
        false
    }

    fn push_children(&self, i: NodeIdx, work: &mut Vec<NodeIdx>) {
        match self.get(i) {
            Some(Node::Object(entries)) => work.extend(entries.iter().map(|(_, c)| *c)),
            Some(Node::Array(items)) => work.extend(items.iter().copied()),
            _ => {}
        }
    }

    /// Bind the Java wrapper in heap slot `slot` to `node`. A slot binds
    /// once per wrapper lifetime; a rebind (impossible unless the prune was
    /// skipped) replaces the stale entry rather than duplicating it.
    pub fn bind(&mut self, slot: u16, node: NodeIdx) {
        match self.registry.iter_mut().find(|(s, _)| *s == slot) {
            Some(entry) => entry.1 = node,
            None => self.registry.push((slot, node)),
        }
    }

    /// Drop the bindings of wrappers the collector freed (`live(slot)` is
    /// false), then sweep every node no surviving wrapper can reach.
    pub fn prune(&mut self, live: &dyn Fn(u16) -> bool) {
        self.registry.retain(|(slot, _)| live(*slot));
        let mut marks = vec![0u8; self.nodes.len().div_ceil(8)];
        let mut work: Vec<NodeIdx> = self.registry.iter().map(|(_, n)| *n).collect();
        while let Some(i) = work.pop() {
            if mark(&mut marks, i) {
                continue;
            }
            self.push_children(i, &mut work);
        }
        for i in 0..self.nodes.len() {
            if self.nodes[i].is_some() && !is_marked(&marks, i as NodeIdx) {
                self.free_node(i as NodeIdx);
            }
        }
    }

    /// Forget everything (JVM heap reset before a new app).
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.free.clear();
        self.registry.clear();
        self.live = 0;
        self.payload = 0;
        self.last_error = None;
    }

    pub fn node_count(&self) -> usize {
        self.live
    }

    pub fn payload_bytes(&self) -> usize {
        self.payload
    }

    pub fn registry_len(&self) -> usize {
        self.registry.len()
    }

    /// Message of the last failed parse / put, for the Java side to wrap in
    /// its `JSONException`.
    pub fn set_last_error(&mut self, msg: String) {
        self.last_error = Some(msg);
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

/// Set the mark bit for `i`; returns whether it was already set.
fn mark(marks: &mut [u8], i: NodeIdx) -> bool {
    let (byte, bit) = ((i / 8) as usize, i % 8);
    let was = marks[byte] & (1 << bit) != 0;
    marks[byte] |= 1 << bit;
    was
}

fn is_marked(marks: &[u8], i: NodeIdx) -> bool {
    marks[(i / 8) as usize] & (1 << (i % 8)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj_with(pool: &mut Pool, entries: &[(&str, Node)]) -> NodeIdx {
        let obj = pool.alloc(Node::Object(Vec::new())).unwrap();
        for (k, v) in entries {
            let child = pool.alloc(v.clone()).unwrap();
            pool.object_put(obj, k.as_bytes(), child).unwrap();
        }
        obj
    }

    #[test]
    fn put_get_remove_keep_insertion_order() {
        let mut pool = Pool::new();
        let obj = obj_with(&mut pool, &[("b", Node::Int(1)), ("a", Node::Int(2))]);
        assert_eq!(pool.key_at(obj, 0), Some(&b"b"[..]));
        assert_eq!(pool.key_at(obj, 1), Some(&b"a"[..]));
        let replacement = pool.alloc(Node::Int(3)).unwrap();
        pool.object_put(obj, b"b", replacement).unwrap();
        assert_eq!(pool.key_at(obj, 0), Some(&b"b"[..]));
        assert_eq!(
            pool.get(pool.object_get(obj, b"b").unwrap()),
            Some(&Node::Int(3))
        );
        assert_eq!(pool.length(obj), 2);
        assert!(pool.object_remove(obj, b"b").is_some());
        assert_eq!(pool.length(obj), 1);
        assert_eq!(pool.object_get(obj, b"b"), None);
        assert_eq!(pool.payload_bytes(), 1);
    }

    #[test]
    fn array_set_pads_with_null_and_appends() {
        let mut pool = Pool::new();
        let arr = pool.alloc(Node::Array(Vec::new())).unwrap();
        let v = pool.alloc(Node::Int(7)).unwrap();
        pool.array_set(arr, Some(2), v).unwrap();
        assert_eq!(pool.length(arr), 3);
        assert_eq!(
            pool.kind(pool.array_get(arr, 0).unwrap()),
            super::super::K_NULL
        );
        assert_eq!(
            pool.get(pool.array_get(arr, 2).unwrap()),
            Some(&Node::Int(7))
        );
        let w = pool.alloc(Node::Bool(true)).unwrap();
        pool.array_set(arr, None, w).unwrap();
        assert_eq!(pool.length(arr), 4);
        let first = pool.array_get(arr, 0).unwrap();
        assert_eq!(pool.array_remove(arr, 0), Some(first));
        assert_eq!(pool.length(arr), 3);
        assert_eq!(pool.array_remove(arr, 9), None);
    }

    #[test]
    fn prune_sweeps_unreachable_and_keeps_shared_subtrees() {
        let mut pool = Pool::new();
        let doc_a = obj_with(&mut pool, &[("x", Node::Int(1))]);
        let doc_b = obj_with(&mut pool, &[("y", Node::Int(2))]);
        let shared = obj_with(&mut pool, &[("z", Node::Str(b"zz".to_vec()))]);
        pool.object_put(doc_a, b"s", shared).unwrap();
        pool.object_put(doc_b, b"s", shared).unwrap();
        pool.bind(10, doc_a);
        pool.bind(11, doc_b);
        let before = pool.node_count();
        // Wrapper in slot 10 dies: doc_a and its own leaf go, `shared` stays.
        pool.prune(&|slot| slot == 11);
        assert_eq!(pool.node_count(), before - 2);
        assert!(pool.get(shared).is_some());
        assert_eq!(pool.registry_len(), 1);
        // Everything dies.
        pool.prune(&|_| false);
        assert_eq!(pool.node_count(), 0);
        assert_eq!(pool.payload_bytes(), 0);
        // Freed slots are reused before the table grows.
        let table_len = pool.nodes.len();
        pool.alloc(Node::Null).unwrap();
        assert_eq!(pool.nodes.len(), table_len);
    }

    #[test]
    fn linking_a_node_into_itself_is_refused() {
        let mut pool = Pool::new();
        let outer = obj_with(&mut pool, &[]);
        let inner = obj_with(&mut pool, &[]);
        pool.object_put(outer, b"in", inner).unwrap();
        assert_eq!(
            pool.object_put(inner, b"back", outer),
            Err(PoolError::Cycle)
        );
        assert_eq!(pool.object_put(outer, b"me", outer), Err(PoolError::Cycle));
        let arr = pool.alloc(Node::Array(Vec::new())).unwrap();
        pool.array_set(arr, None, outer).unwrap();
        assert_eq!(pool.array_set(arr, None, arr), Err(PoolError::Cycle));
        assert_eq!(pool.object_put(inner, b"arr", arr), Err(PoolError::Cycle));
        assert_eq!(
            pool.object_put(inner, b"dead", 999),
            Err(PoolError::Invalid)
        );
    }

    #[test]
    fn caps_fail_cleanly() {
        let mut pool = Pool::new();
        let big = vec![b'a'; MAX_PAYLOAD_BYTES + 1];
        assert_eq!(pool.alloc(Node::Str(big)), Err(PoolError::Exhausted));
        assert_eq!(pool.node_count(), 0);
        for _ in 0..MAX_NODES {
            pool.alloc(Node::Null).unwrap();
        }
        assert_eq!(pool.alloc(Node::Null), Err(PoolError::Exhausted));
        pool.clear();
        assert_eq!(pool.node_count(), 0);
        pool.alloc(Node::Null).unwrap();
    }

    #[test]
    fn global_pool_round_trips() {
        with_pool(|p| p.clear());
        let n = with_pool(|p| p.alloc(Node::Int(5)).unwrap());
        assert_eq!(with_pool(|p| p.kind(n)), super::super::K_INT);
        with_pool(|p| p.clear());
    }
}
