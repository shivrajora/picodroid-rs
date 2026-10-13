// SPDX-License-Identifier: GPL-3.0-only
//! `JSONObject` / `JSONArray` natives over the node pool in `crate::json`.
//!
//! The Java side owns every type decision (boxing, coercion, `put`'s
//! `instanceof` ladder), so each arm here is one typed primitive on one
//! node: create, bind, parse, read a leaf, link a child, unlink, serialize.
//! Nothing in this file holds a JVM reference past the call, and every node
//! a call creates is either linked into a live tree or bound to the wrapper
//! that asked for it before the call returns — the pool's liveness rule.
//!
//! Status codes returned to Java for the mutators: `0` ok, `-1` pool
//! exhausted, `-2` the link would make a container reach itself
//! (`JSONObject.ST_*`).
use crate::shrink_names::{ch, mh};
use pico_jvm::NativeKey;

use alloc::{string::String, vec::Vec};

use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

use crate::json::{
    self,
    pool::{with_pool, Pool, PoolError},
    Node, NodeIdx,
};

const ST_OK: i32 = 0;
const ST_EXHAUSTED: i32 = -1;
const ST_CYCLE: i32 = -2;

pub fn dispatch(
    key: NativeKey,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if key.class != ch::picodroid_json_JSONObject {
        return None;
    }
    let r = match key.method {
        mh::nativeNewObject => new_container(ctx, Node::Object(Vec::new())),
        mh::nativeNewArray => new_container(ctx, Node::Array(Vec::new())),
        mh::nativeBind => bind(ctx),
        mh::nativeParse => parse(ctx),
        mh::nativeLastError => last_error(ctx),
        mh::nativeKind => node_int(ctx, |p, n| p.kind(n)),
        mh::nativeLength => node_int(ctx, |p, n| p.length(n) as i32),
        mh::nativeChild => child(ctx),
        mh::nativeChildAt => child_at(ctx),
        mh::nativeKeyAt => key_at(ctx),
        mh::nativeBoolValue => node_int(ctx, |p, n| match p.get(n) {
            Some(Node::Bool(b)) => *b as i32,
            _ => 0,
        }),
        mh::nativeIntValue => node_int(ctx, |p, n| match p.get(n) {
            Some(Node::Int(v)) => *v,
            Some(Node::Long(v)) => *v as i32,
            Some(Node::Double(d)) => *d as i32,
            _ => 0,
        }),
        mh::nativeLongValue => node_value(ctx, |p, n| {
            Value::Long(match p.get(n) {
                Some(Node::Int(v)) => *v as i64,
                Some(Node::Long(v)) => *v,
                Some(Node::Double(d)) => *d as i64,
                _ => 0,
            })
        }),
        mh::nativeDoubleValue => node_value(ctx, |p, n| {
            Value::Double(match p.get(n) {
                Some(Node::Int(v)) => *v as f64,
                Some(Node::Long(v)) => *v as f64,
                Some(Node::Double(d)) => *d,
                _ => 0.0,
            })
        }),
        mh::nativeStringValue => string_value(ctx),
        mh::nativePutNull => put(ctx, |_| Ok(Node::Null)),
        mh::nativePutBool => put(ctx, |a| Ok(Node::Bool(as_int(a.get(2))? != 0))),
        mh::nativePutInt => put(ctx, |a| Ok(Node::Int(as_int(a.get(2))?))),
        mh::nativePutLong => put(ctx, |a| Ok(Node::Long(as_long(a.get(2))?))),
        mh::nativePutDouble => put(ctx, |a| Ok(Node::Double(as_double(a.get(2))?))),
        mh::nativePutString => put_string(ctx),
        mh::nativePutNode => put_node(ctx),
        mh::nativeSetNull => set(ctx, |_| Ok(Node::Null)),
        mh::nativeSetBool => set(ctx, |a| Ok(Node::Bool(as_int(a.get(2))? != 0))),
        mh::nativeSetInt => set(ctx, |a| Ok(Node::Int(as_int(a.get(2))?))),
        mh::nativeSetLong => set(ctx, |a| Ok(Node::Long(as_long(a.get(2))?))),
        mh::nativeSetDouble => set(ctx, |a| Ok(Node::Double(as_double(a.get(2))?))),
        mh::nativeSetString => set_string(ctx),
        mh::nativeSetNode => set_node(ctx),
        mh::nativeRemove => remove(ctx),
        mh::nativeRemoveAt => remove_at(ctx),
        mh::nativeToString => to_string(ctx),
        mh::nativeQuote => quote(ctx),
        mh::nativePoolNodes => Ok(Some(Value::Int(with_pool(|p| p.node_count() as i32)))),
        _ => return None,
    };
    Some(r)
}

// ── creation / binding ─────────────────────────────────────────────────────

/// `nativeNewObject(self)` / `nativeNewArray(self)`: allocate and bind in one
/// step, so the fresh node is rooted before Java can allocate again.
fn new_container(ctx: &mut NativeContext<'_>, node: Node) -> Result<Option<Value>, JvmError> {
    let slot = as_obj(ctx.args.first())?;
    let idx = with_pool(|p| match p.alloc(node) {
        Ok(i) => {
            p.bind(slot, i);
            i as i32
        }
        Err(_) => ST_EXHAUSTED,
    });
    ctx.objects.charge_alloc_events(1);
    Ok(Some(Value::Int(idx)))
}

/// `nativeBind(self, node)`: a child wrapper announcing itself.
fn bind(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let slot = as_obj(ctx.args.first())?;
    let node = node_arg(ctx.args.get(1))?;
    with_pool(|p| {
        if p.get(node).is_some() {
            p.bind(slot, node);
        }
    });
    Ok(None)
}

/// `nativeParse(self, text, wantArray)`: root index, or `-1` with the
/// message parked for `nativeLastError`.
fn parse(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let slot = as_obj(ctx.args.first())?;
    let text = str_arg(ctx, 1)?;
    let want_array = as_int(ctx.args.get(2))? != 0;
    let (idx, created) = with_pool(|p| {
        let before = p.node_count();
        let idx = match json::parse::parse(p, &text, Some(want_array)) {
            Ok(root) => {
                p.bind(slot, root);
                root as i32
            }
            Err(e) => {
                p.set_last_error(e.0);
                -1
            }
        };
        (idx, p.node_count().saturating_sub(before))
    });
    // Every node is native storage the GC pacer cannot see; charge it as
    // the allocations it stands in for, or a parse loop outruns collection.
    ctx.objects
        .charge_alloc_events(created.min(u16::MAX as usize) as u16);
    Ok(Some(Value::Int(idx)))
}

fn last_error(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let msg = with_pool(|p| p.last_error().map(String::from))
        .unwrap_or_else(|| String::from("JSON error"));
    ret_string(ctx, msg.into_bytes())
}

// ── reads ──────────────────────────────────────────────────────────────────
//
// The per-kind work below (`node_int`, `node_value`, `put`, `set`) takes a
// `fn` pointer, not a generic closure: every closure passed is
// non-capturing, and a generic parameter gave each one its own copy of the
// argument decoding and the pool call.

fn node_int(
    ctx: &mut NativeContext<'_>,
    f: fn(&Pool, NodeIdx) -> i32,
) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    Ok(Some(Value::Int(with_pool(|p| f(p, node)))))
}

fn node_value(
    ctx: &mut NativeContext<'_>,
    f: fn(&Pool, NodeIdx) -> Value,
) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    Ok(Some(with_pool(|p| f(p, node))))
}

/// `nativeChild(node, name)`: the child's index or `-1`.
fn child(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let key = str_arg(ctx, 1)?;
    let idx = with_pool(|p| p.object_get(node, &key).map(|i| i as i32).unwrap_or(-1));
    Ok(Some(Value::Int(idx)))
}

/// `nativeChildAt(node, index)`: the item's index or `-1`.
fn child_at(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let index = as_int(ctx.args.get(1))?;
    let idx = with_pool(|p| {
        usize::try_from(index)
            .ok()
            .and_then(|i| p.array_get(node, i))
            .map(|i| i as i32)
            .unwrap_or(-1)
    });
    Ok(Some(Value::Int(idx)))
}

/// `nativeKeyAt(node, index)`: the key as a new Java string, or `null`.
fn key_at(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let index = as_int(ctx.args.get(1))?;
    let key = with_pool(|p| {
        usize::try_from(index)
            .ok()
            .and_then(|i| p.key_at(node, i))
            .map(<[u8]>::to_vec)
    });
    match key {
        Some(k) => ret_string(ctx, k),
        None => Ok(Some(Value::Null)),
    }
}

/// `nativeStringValue(node)`: a string leaf materialized, `null` otherwise.
fn string_value(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let s = with_pool(|p| match p.get(node) {
        Some(Node::Str(s)) => Some(s.clone()),
        _ => None,
    });
    match s {
        Some(s) => ret_string(ctx, s),
        None => Ok(Some(Value::Null)),
    }
}

// ── object mutation: nativePut*(node, name, value) ─────────────────────────

fn put(
    ctx: &mut NativeContext<'_>,
    leaf: fn(&[Value]) -> Result<Node, JvmError>,
) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let key = str_arg(ctx, 1)?;
    let value = leaf(ctx.args)?;
    ctx.objects.charge_alloc_events(1);
    Ok(Some(Value::Int(with_pool(|p| {
        link_leaf(p, node, &key, None, value)
    }))))
}

fn put_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let key = str_arg(ctx, 1)?;
    let value = str_arg(ctx, 2)?;
    ctx.objects.charge_alloc_events(1);
    Ok(Some(Value::Int(with_pool(|p| {
        link_leaf(p, node, &key, None, Node::Str(value))
    }))))
}

fn put_node(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let key = str_arg(ctx, 1)?;
    let child = node_arg(ctx.args.get(2))?;
    Ok(Some(Value::Int(with_pool(|p| {
        status(p.object_put(node, &key, child))
    }))))
}

// ── array mutation: nativeSet*(node, index, value); index < 0 appends ──────

fn set(
    ctx: &mut NativeContext<'_>,
    leaf: fn(&[Value]) -> Result<Node, JvmError>,
) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let index = array_index(ctx.args.get(1))?;
    let value = leaf(ctx.args)?;
    ctx.objects.charge_alloc_events(1);
    Ok(Some(Value::Int(with_pool(|p| {
        link_leaf(p, node, b"", index, value)
    }))))
}

fn set_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let index = array_index(ctx.args.get(1))?;
    let value = str_arg(ctx, 2)?;
    ctx.objects.charge_alloc_events(1);
    Ok(Some(Value::Int(with_pool(|p| {
        link_leaf(p, node, b"", index, Node::Str(value))
    }))))
}

fn set_node(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let index = array_index(ctx.args.get(1))?;
    let child = node_arg(ctx.args.get(2))?;
    Ok(Some(Value::Int(with_pool(|p| {
        status(p.array_set(node, index, child))
    }))))
}

/// Allocate `value` and link it: under `key` when the container is an
/// object, at `index` (`None` appends) when it is an array. A link that
/// fails frees the leaf again so nothing leaks until the next prune.
fn link_leaf(
    p: &mut Pool,
    container: NodeIdx,
    key: &[u8],
    index: Option<usize>,
    value: Node,
) -> i32 {
    let leaf = match p.alloc(value) {
        Ok(i) => i,
        Err(_) => return ST_EXHAUSTED,
    };
    let linked = match p.get(container) {
        Some(Node::Object(_)) => p.object_put(container, key, leaf),
        Some(Node::Array(_)) => p.array_set(container, index, leaf),
        _ => Err(PoolError::Invalid),
    };
    if linked.is_err() {
        p.free_node(leaf);
    }
    status(linked)
}

fn status(r: Result<(), PoolError>) -> i32 {
    match r {
        Ok(()) => ST_OK,
        Err(PoolError::Cycle) => ST_CYCLE,
        Err(PoolError::Exhausted) | Err(PoolError::Invalid) => ST_EXHAUSTED,
    }
}

// ── unlink ─────────────────────────────────────────────────────────────────

fn remove(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let key = str_arg(ctx, 1)?;
    with_pool(|p| {
        p.object_remove(node, &key);
    });
    Ok(None)
}

fn remove_at(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let index = as_int(ctx.args.get(1))?;
    with_pool(|p| {
        if let Ok(i) = usize::try_from(index) {
            p.array_remove(node, i);
        }
    });
    Ok(None)
}

// ── serialization ──────────────────────────────────────────────────────────

/// `nativeToString(node, indent)`: the document, or `null` past the depth cap.
fn to_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let node = node_arg(ctx.args.first())?;
    let indent = as_int(ctx.args.get(1))?.max(0) as usize;
    let out = with_pool(|p| json::serialize::to_bytes(p, node, indent));
    match out {
        Some(bytes) => ret_string(ctx, bytes),
        None => Ok(Some(Value::Null)),
    }
}

fn quote(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let data = str_arg(ctx, 0)?;
    let mut out = Vec::with_capacity(data.len() + 2);
    json::serialize::quote_into(&data, &mut out);
    ret_string(ctx, out)
}

// ── arg / return helpers ───────────────────────────────────────────────────

fn as_obj(v: Option<&Value>) -> Result<u16, JvmError> {
    match v {
        Some(Value::ObjectRef(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_int(v: Option<&Value>) -> Result<i32, JvmError> {
    match v {
        Some(Value::Int(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_long(v: Option<&Value>) -> Result<i64, JvmError> {
    match v {
        Some(Value::Long(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_double(v: Option<&Value>) -> Result<f64, JvmError> {
    match v {
        Some(Value::Double(d)) => Ok(*d),
        _ => Err(JvmError::InvalidReference),
    }
}

/// A node index argument; Java only ever passes indices the pool handed out.
fn node_arg(v: Option<&Value>) -> Result<NodeIdx, JvmError> {
    u16::try_from(as_int(v)?).map_err(|_| JvmError::InvalidReference)
}

/// An array index argument; negative means "append".
fn array_index(v: Option<&Value>) -> Result<Option<usize>, JvmError> {
    Ok(usize::try_from(as_int(v)?).ok())
}

/// Copy of the `String` argument at `i`. Java guards against `null`
/// before every call, so a non-string here is a wiring bug, not an NPE.
fn str_arg(ctx: &mut NativeContext<'_>, i: usize) -> Result<Vec<u8>, JvmError> {
    match ctx.args.get(i) {
        Some(Value::Reference(idx)) => ctx
            .strings
            .resolve(*idx)
            .map(|s| s.as_bytes().to_vec())
            .ok_or(JvmError::InvalidReference),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Hand `bytes` (valid UTF-8 by construction) to Java as a new string.
fn ret_string(ctx: &mut NativeContext<'_>, bytes: Vec<u8>) -> Result<Option<Value>, JvmError> {
    ctx.strings
        .intern_dyn_owned(bytes)
        .map(|i| Some(Value::Reference(i)))
        .ok_or(JvmError::StackOverflow)
}
