// SPDX-License-Identifier: GPL-3.0-only
//! `org.json` on the device: a capped node pool fed by whatever an app
//! downloaded, then edited through `JSONObject` / `JSONArray` calls.

use std::collections::BTreeSet;

use pd_json::parse::parse;
use pd_json::pool::{Pool, PoolError, MAX_PAYLOAD_BYTES};
use pd_json::serialize::to_bytes;
use pd_json::{Node, NodeIdx, MAX_DEPTH};

use crate::Bytes;

/// Input: a byte choosing the root the caller asked for (`JSONObject`,
/// `JSONArray`, any), then the text.
///
/// A refused document frees every node it allocated. An accepted one
/// serializes to text that parses back to a tree that serializes the same:
/// the serializer's output is a fixed point.
pub fn json(data: &[u8]) {
    let mut b = Bytes::new(data);
    let want = [Some(false), Some(true), None][b.below(3)];
    let text = b.rest();

    let mut pool = Pool::new();
    let root = match parse(&mut pool, text, want) {
        Ok(root) => root,
        Err(_) => {
            assert_eq!(pool.node_count(), 0, "a refused document leaked nodes");
            assert_eq!(pool.payload_bytes(), 0, "a refused document leaked bytes");
            return;
        }
    };
    let Some(once) = to_bytes(&pool, root, 0) else {
        return;
    };
    let mut again = Pool::new();
    let root2 = parse(&mut again, &once, want)
        .unwrap_or_else(|e| panic!("serializer output does not parse: {e:?}"));
    let twice = to_bytes(&again, root2, 0).expect("re-serializes");
    assert!(
        once == twice,
        "serialize -> parse -> serialize is not a fixed point:\n{}\n{}",
        String::from_utf8_lossy(&once),
        String::from_utf8_lossy(&twice)
    );
}

/// The harness's own view of the pool: which nodes it allocated and has
/// not freed, and which heap slots it bound. Everything else (the edges,
/// the payload) is read back from the pool and re-derived independently.
struct Shadow {
    live: Vec<NodeIdx>,
    bound: Vec<(u16, NodeIdx)>,
}

impl Shadow {
    fn pick(&self, b: &mut Bytes<'_>) -> Option<NodeIdx> {
        if self.live.is_empty() {
            None
        } else {
            Some(self.live[b.below(self.live.len())])
        }
    }

    /// A live node, or (one time in eight) any index at all, so the dead
    /// and out-of-range paths are reached too.
    fn pick_or_garbage(&self, b: &mut Bytes<'_>) -> NodeIdx {
        let garbage = b.u8().is_multiple_of(8);
        match self.pick(b) {
            Some(n) if !garbage => n,
            _ => b.u16(),
        }
    }

    fn forget(&mut self, idx: NodeIdx) {
        self.live.retain(|&n| n != idx);
    }
}

fn children(pool: &Pool, idx: NodeIdx) -> Vec<NodeIdx> {
    match pool.get(idx) {
        Some(Node::Object(entries)) => entries.iter().map(|(_, c)| *c).collect(),
        Some(Node::Array(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// Nodes reachable from `roots`, by a traversal independent of the pool's.
fn reachable(pool: &Pool, roots: impl Iterator<Item = NodeIdx>) -> BTreeSet<NodeIdx> {
    let mut seen = BTreeSet::new();
    let mut work: Vec<NodeIdx> = roots.collect();
    while let Some(i) = work.pop() {
        if seen.insert(i) {
            work.extend(children(pool, i));
        }
    }
    seen
}

/// Depth of the tree under `root` in containers, or `None` on a cycle.
fn depth(pool: &Pool, root: NodeIdx, path: &mut Vec<NodeIdx>) -> Option<usize> {
    if path.contains(&root) {
        return None;
    }
    let kids = children(pool, root);
    if !matches!(pool.get(root), Some(Node::Object(_) | Node::Array(_))) {
        return Some(0);
    }
    path.push(root);
    let mut deepest = 0;
    for k in kids {
        deepest = deepest.max(depth(pool, k, path)? + 1);
    }
    path.pop();
    Some(deepest)
}

/// The invariants that hold between any two operations.
fn check(pool: &Pool, shadow: &Shadow) {
    assert_eq!(pool.node_count(), shadow.live.len(), "node count drifted");
    let mut payload = 0usize;
    for &n in &shadow.live {
        let node = pool
            .get(n)
            .unwrap_or_else(|| panic!("live node {n} is gone"));
        payload += node.payload_bytes();
        for c in children(pool, n) {
            assert!(pool.get(c).is_some(), "node {n} links to dead node {c}");
        }
    }
    assert_eq!(pool.payload_bytes(), payload, "payload bytes drifted");
    assert!(payload <= MAX_PAYLOAD_BYTES);
    for &n in &shadow.live {
        assert!(
            depth(pool, n, &mut Vec::new()).is_some(),
            "node {n} reaches itself"
        );
        for c in children(pool, n) {
            assert!(!pool.reaches(c, n), "node {n} reaches itself via {c}");
        }
    }
}

/// Input: an optional seed document, then a script of `JSONObject` /
/// `JSONArray` calls: put, remove, set, append, allocate, bind a wrapper,
/// collect (prune), free, clear.
///
/// After every call the pool's node count and payload equal what its own
/// nodes add up to, no node reaches itself (the cycle check is what keeps
/// `toString` from recursing for ever on the device), and a link that the
/// pool refused left the tree unchanged. A prune frees exactly the nodes a
/// traversal from the surviving wrappers cannot reach. Every bound root
/// still serializes and re-parses to the same text unless it is nested
/// past `MAX_DEPTH`.
pub fn json_ops(data: &[u8]) {
    let mut b = Bytes::new(data);
    let mut pool = Pool::new();
    let mut shadow = Shadow {
        live: Vec::new(),
        bound: Vec::new(),
    };

    let seed = b.take(256);
    if let Ok(root) = parse(&mut pool, seed, None) {
        // Every node the parser allocated, reachable or not: a duplicate key
        // (`{"a":1,"a":2}`) leaves its first value allocated but unlinked
        // until the next prune, as `object_put` replaces the entry in place.
        // A fresh pool numbers them 0..count.
        shadow.live = (0..pool.node_count() as NodeIdx).collect();
        assert!(shadow.live.iter().all(|&n| pool.get(n).is_some()));
        shadow.bound.push((0, root));
        pool.bind(0, root);
    } else {
        assert_eq!(pool.node_count(), 0, "a refused document leaked nodes");
    }
    check(&pool, &shadow);

    let mut ops = 0;
    while !b.is_empty() && ops < 256 {
        ops += 1;
        match b.u8() % 11 {
            0 | 1 => {
                // Allocate a leaf or a container.
                let node = match b.u8() % 8 {
                    0 => Node::Null,
                    1 => Node::Bool(b.u8() & 1 == 1),
                    2 => Node::Int(b.u32() as i32),
                    3 => Node::Long(b.u64() as i64),
                    // `JSONObject.put(double)` throws on NaN and infinity
                    // ("Forbidden numeric value"), as Android's does, and
                    // the parser refuses them, so the pool never holds one.
                    4 => {
                        let d = f64::from_bits(b.u64());
                        Node::Double(if d.is_finite() { d } else { 0.5 })
                    }
                    5 => Node::Str(b.take(64).to_vec()),
                    6 => Node::Object(Vec::new()),
                    _ => Node::Array(Vec::new()),
                };
                match pool.alloc(node) {
                    Ok(idx) => shadow.live.push(idx),
                    Err(e) => assert_eq!(e, PoolError::Exhausted, "alloc failed with {e:?}"),
                }
            }
            2 | 3 => {
                // object.put(key, child)
                let obj = shadow.pick_or_garbage(&mut b);
                let child = shadow.pick_or_garbage(&mut b);
                let key = b.take(16).to_vec();
                let before = pool.length(obj);
                let had = pool.object_get(obj, &key);
                let cycle =
                    child == obj || reachable(&pool, core::iter::once(child)).contains(&obj);
                let valid =
                    pool.get(child).is_some() && matches!(pool.get(obj), Some(Node::Object(_)));
                match pool.object_put(obj, &key, child) {
                    Ok(()) => {
                        assert!(
                            valid && !cycle,
                            "linked a {} child",
                            if cycle { "cyclic" } else { "dead" }
                        );
                        assert_eq!(pool.object_get(obj, &key), Some(child));
                        assert_eq!(pool.length(obj), before + usize::from(had.is_none()));
                    }
                    Err(PoolError::Invalid) => {
                        assert!(!valid, "refused a valid link as Invalid")
                    }
                    Err(PoolError::Cycle) => assert!(cycle, "refused an acyclic link as Cycle"),
                    Err(PoolError::Exhausted) => {
                        assert!(had.is_none() && valid && !cycle);
                        assert_eq!(pool.object_get(obj, &key), None);
                    }
                }
                if !valid || cycle {
                    assert_eq!(pool.length(obj), before, "a refused put changed the object");
                    assert_eq!(pool.object_get(obj, &key), had);
                }
            }
            4 => {
                // object.remove(key)
                let obj = shadow.pick_or_garbage(&mut b);
                let key = b.take(16).to_vec();
                let had = pool.object_get(obj, &key);
                let before = pool.length(obj);
                let removed = pool.object_remove(obj, &key);
                assert_eq!(removed, had);
                assert_eq!(pool.object_get(obj, &key), None);
                assert_eq!(pool.length(obj), before - usize::from(had.is_some()));
            }
            5 | 6 => {
                // array.put(index?, child)
                let arr = shadow.pick_or_garbage(&mut b);
                let child = shadow.pick_or_garbage(&mut b);
                let index = (b.u8() & 1 == 1).then(|| b.below(40));
                let before = pool.length(arr);
                let cycle =
                    child == arr || reachable(&pool, core::iter::once(child)).contains(&arr);
                let valid =
                    pool.get(child).is_some() && matches!(pool.get(arr), Some(Node::Array(_)));
                let count_before = pool.node_count();
                match pool.array_set(arr, index, child) {
                    Ok(()) => {
                        assert!(valid && !cycle);
                        let at = index.unwrap_or(before);
                        assert_eq!(pool.array_get(arr, at), Some(child));
                        assert_eq!(pool.length(arr), before.max(at + 1));
                        // Padding nulls are nodes the harness did not allocate.
                        for i in before..at {
                            let pad = pool.array_get(arr, i).expect("padding");
                            assert_eq!(pool.get(pad), Some(&Node::Null));
                            shadow.live.push(pad);
                        }
                    }
                    Err(PoolError::Invalid) => assert!(!valid),
                    Err(PoolError::Cycle) => assert!(cycle),
                    Err(PoolError::Exhausted) => {
                        // Padding may have landed before the cap hit.
                        let now = pool.length(arr);
                        for i in before..now {
                            shadow.live.push(pool.array_get(arr, i).unwrap());
                        }
                        assert_eq!(pool.node_count(), count_before + (now - before));
                    }
                }
                if !valid || cycle {
                    assert_eq!(pool.length(arr), before, "a refused set changed the array");
                }
            }
            7 => {
                // array.remove(index)
                let arr = shadow.pick_or_garbage(&mut b);
                let index = b.below(40);
                let before = pool.length(arr);
                let had = pool.array_get(arr, index);
                assert_eq!(pool.array_remove(arr, index), had);
                assert_eq!(pool.length(arr), before - usize::from(had.is_some()));
            }
            8 => {
                // A wrapper binds to the node it was just given. (Only a
                // live node: the Java side binds straight after alloc or
                // parse, and the pool's mark bitmap is sized to its table.)
                let slot = b.u8() as u16;
                let Some(node) = shadow.pick(&mut b) else {
                    continue;
                };
                pool.bind(slot, node);
                shadow.bound.retain(|(s, _)| *s != slot);
                shadow.bound.push((slot, node));
                assert_eq!(pool.registry_len(), shadow.bound.len());
            }
            9 => {
                // GC: some wrappers die, then prune.
                let mask = b.u8();
                let alive = |slot: u16| (mask >> (slot % 8)) & 1 == 1;
                shadow.bound.retain(|(s, _)| alive(*s));
                let roots = shadow.bound.iter().map(|(_, n)| *n);
                let keep = reachable(&pool, roots);
                pool.prune(&alive);
                assert_eq!(pool.registry_len(), shadow.bound.len());
                let live: BTreeSet<NodeIdx> = shadow.live.iter().copied().collect();
                // Bound dead indices root nothing; otherwise the survivors
                // are exactly the reachable live nodes.
                let expect: BTreeSet<NodeIdx> = keep.intersection(&live).copied().collect();
                for &n in &live {
                    assert_eq!(
                        pool.get(n).is_some(),
                        expect.contains(&n),
                        "prune {} node {n}",
                        if expect.contains(&n) {
                            "swept a reachable"
                        } else {
                            "kept an unreachable"
                        }
                    );
                }
                shadow.live = expect.into_iter().collect();
            }
            _ => {
                if b.u8().is_multiple_of(16) {
                    pool.clear();
                    shadow.live.clear();
                    shadow.bound.clear();
                } else if let Some(n) = shadow.pick(&mut b) {
                    // The parser's rollback path: free one node outright.
                    // Anything still linking to it now dangles, which the
                    // pool tolerates (`get` says None) but the harness's
                    // own walk must not: unlink it first.
                    unlink_everywhere(&mut pool, &shadow, n);
                    pool.free_node(n);
                    shadow.forget(n);
                }
            }
        }
        check(&pool, &shadow);
    }

    // Every bound root still prints and re-reads the same.
    for &(_, root) in &shadow.bound {
        if pool.get(root).is_none() {
            continue;
        }
        let d = depth(&pool, root, &mut Vec::new()).expect("acyclic");
        match to_bytes(&pool, root, 0) {
            None => assert!(d >= MAX_DEPTH, "depth {d} refused to serialize"),
            Some(once) => {
                assert!(d < MAX_DEPTH, "depth {d} serialized");
                let mut again = Pool::new();
                let want = match pool.get(root) {
                    Some(Node::Object(_)) => Some(false),
                    Some(Node::Array(_)) => Some(true),
                    _ => None,
                };
                let root2 = parse(&mut again, &once, want)
                    .unwrap_or_else(|e| panic!("serializer output does not parse: {e:?}"));
                let twice = to_bytes(&again, root2, 0).expect("re-serializes");
                assert_eq!(once, twice, "not a fixed point");
            }
        }
    }
}

/// Remove every link to `target` from the live containers, so freeing it
/// leaves no dangling edge for the harness's own traversal to trip on.
fn unlink_everywhere(pool: &mut Pool, shadow: &Shadow, target: NodeIdx) {
    for &n in &shadow.live {
        loop {
            let key = match pool.get(n) {
                Some(Node::Object(entries)) => entries
                    .iter()
                    .find(|(_, c)| *c == target)
                    .map(|(k, _)| k.clone()),
                _ => None,
            };
            match key {
                Some(k) => {
                    pool.object_remove(n, &k);
                }
                None => break,
            }
        }
        loop {
            let pos = match pool.get(n) {
                Some(Node::Array(items)) => items.iter().position(|c| *c == target),
                _ => None,
            };
            match pos {
                Some(p) => {
                    pool.array_remove(n, p);
                }
                None => break,
            }
        }
    }
}
