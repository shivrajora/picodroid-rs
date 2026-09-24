// SPDX-License-Identifier: GPL-3.0-only
//! Collection operations the interpreter must run itself because they call
//! back into Java: `sort` with a `Comparator`, and `contains` / `indexOf` /
//! `remove` / map lookups on keys whose class overrides `equals`.

use super::{helpers, Executor};
use crate::names::{c, d, m};
use crate::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};
use alloc::vec::Vec;

/// A builtin-collection operation whose answer depends on key equality
/// (see `Executor::equals_aware_collection_op`).
#[derive(Clone, Copy)]
pub(super) enum CollOp {
    MapGet,
    MapGetOrDefault,
    MapContainsKey,
    MapRemove,
    MapPut,
    SetAdd,
    SetContains,
    SetRemove,
    ListContains,
    ListRemove,
}

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    /// Sort a builtin `ArrayList` under a Java `Comparator`, one
    /// [`Self::invoke_java`] upcall per comparison.
    pub(super) fn sort_list_with_comparator(
        &mut self,
        frames: &mut Vec<Frame>,
        args: &[Value],
    ) -> Result<(), JvmError> {
        let recv = args.first().copied().unwrap_or(Value::Null);
        let Value::ObjectRef(obj_idx) = recv else {
            return Err(JvmError::InvalidReference);
        };
        // A null comparator is natural ordering (`Comparable.compareTo`),
        // as the JDK reads it; `insertion_sort` upcalls accordingly.
        let cmp = args.get(1).copied().unwrap_or(Value::Null);
        let Some(Value::Int(buf)) = self.objects.get_field(obj_idx, 0) else {
            return Err(JvmError::InvalidReference);
        };
        // The list and the comparator are reachable only from this function's
        // Rust locals for the whole sort — `op_invoke` popped them off the
        // operand stack before dispatching here. Without rooting them, a
        // collection triggered by the comparator would sweep the list and
        // `list_free` the backing buffer out from under the loop.
        let mark = self.gc_state.push_shadow_roots(&[recv, cmp]);
        let r = self.insertion_sort(frames, buf as u16, cmp);
        self.gc_state.truncate_shadow_roots(mark);
        r
    }

    /// Insertion sort, deliberately, rather than the merge sort
    /// `Arrays.sort(Object[], Comparator)` uses: no auxiliary buffer means no
    /// second heap object to root, and the list is in a valid partially-sorted
    /// state between every comparison — so an exception escaping the
    /// comparator leaves a well-formed list rather than a half-merged one.
    /// O(n²) is fine at the list sizes an embedded screen holds; revisit if a
    /// caller ever sorts more than a screenful.
    pub(super) fn insertion_sort(
        &mut self,
        frames: &mut Vec<Frame>,
        buf_idx: u16,
        cmp: Value,
    ) -> Result<(), JvmError> {
        const COMPARE: &str = m::compare;
        const COMPARE_DESC: &str = d::Object_Object__I;
        let len = self.objects.list_len(buf_idx);
        for i in 1..len {
            let mut j = i;
            while j > 0 {
                let (Some(prev), Some(cur)) = (
                    self.objects.list_get(buf_idx, j - 1),
                    self.objects.list_get(buf_idx, j),
                ) else {
                    return Err(JvmError::InvalidReference);
                };
                let ord = if matches!(cmp, Value::Null) {
                    // Natural ordering: `prev.compareTo(cur)` — a Java
                    // override, or a builtin's native arm (String, the
                    // boxes); an unboxed element (a test's, or a Kotlin
                    // primitive list's) is ordered directly. A null element
                    // has no order, as in Java.
                    match (prev, cur) {
                        (Value::Int(a), Value::Int(b)) => Some(Value::Int(a.cmp(&b) as i32)),
                        (Value::Long(a), Value::Long(b)) => Some(Value::Int(a.cmp(&b) as i32)),
                        (Value::Null, _) => {
                            return Err(self.runtime_fault(c::java_lang_NullPointerException));
                        }
                        _ => self.invoke_java(frames, prev, m::compareTo, d::Object__I, &[cur])?,
                    }
                } else {
                    self.invoke_java(frames, cmp, COMPARE, COMPARE_DESC, &[prev, cur])?
                };
                let Some(Value::Int(ord)) = ord else {
                    return Err(JvmError::InvalidReference);
                };
                if ord <= 0 {
                    break;
                }
                // Re-read across the upcall rather than reusing `prev`/`cur`:
                // the comparator ran arbitrary Java, which may have collected
                // (compacting the store) or mutated the list itself. A shrunk
                // list surfaces as `None` here rather than a bad write.
                let (Some(prev), Some(cur)) = (
                    self.objects.list_get(buf_idx, j - 1),
                    self.objects.list_get(buf_idx, j),
                ) else {
                    return Err(JvmError::InvalidReference);
                };
                self.objects.list_set(buf_idx, j - 1, cur);
                self.objects.list_set(buf_idx, j, prev);
                j -= 1;
            }
        }
        Ok(())
    }

    /// Does `v` name an object whose class defines its own `equals(Object)`?
    pub(super) fn has_java_equals(&mut self, v: Value) -> bool {
        let Value::ObjectRef(idx) = v else {
            return false;
        };
        let Some(class) = self.objects.class_name(idx) else {
            return false;
        };
        match helpers::find_method_walking_cached(
            &mut self.class_objects.resolve,
            self.classes,
            class,
            m::equals,
            d::Object__Z,
        ) {
            Some((ci, mi)) => self.classes[ci].methods()[mi].code_offset != 0,
            None => false,
        }
    }

    /// `probe.equals(candidate)`: identity first, then the override.
    pub(super) fn user_equals(
        &mut self,
        frames: &mut Vec<Frame>,
        probe: Value,
        candidate: Value,
    ) -> Result<bool, JvmError> {
        if probe == candidate {
            return Ok(true);
        }
        if matches!(candidate, Value::Null) {
            return Ok(false);
        }
        match self.invoke_java(frames, probe, m::equals, d::Object__Z, &[candidate])? {
            Some(Value::Int(b)) => Ok(b != 0),
            _ => Ok(false),
        }
    }

    /// The `HashMap` / `HashSet` / `ArrayList` operations whose answer
    /// depends on key equality, run with the probe's own `equals(Object)`.
    /// `Ok(None)` when the call is not one of them or the probe has no
    /// override, so the ordinary native arm serves it.
    pub(super) fn equals_aware_collection_op(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
        frames: &mut Vec<Frame>,
    ) -> Result<Option<Option<Value>>, JvmError> {
        let is_map = class_name == c::java_util_HashMap || class_name == c::java_util_LinkedHashMap;
        let is_set = class_name == c::java_util_HashSet || class_name == c::java_util_LinkedHashSet;
        let op = match method_name {
            m::get if is_map => CollOp::MapGet,
            m::getOrDefault if is_map => CollOp::MapGetOrDefault,
            m::containsKey if is_map => CollOp::MapContainsKey,
            m::remove if is_map => CollOp::MapRemove,
            m::put if is_map => CollOp::MapPut,
            m::add if is_set => CollOp::SetAdd,
            m::contains if is_set => CollOp::SetContains,
            m::remove if is_set => CollOp::SetRemove,
            m::contains if class_name == c::java_util_ArrayList => CollOp::ListContains,
            m::remove if class_name == c::java_util_ArrayList && descriptor == d::Object__Z => {
                CollOp::ListRemove
            }
            _ => return Ok(None),
        };
        let probe = args.get(1).copied().unwrap_or(Value::Null);
        if !self.has_java_equals(probe) {
            return Ok(None);
        }
        let Some(Value::ObjectRef(recv)) = args.first().copied() else {
            return Ok(None);
        };
        let Some(Value::Int(buf)) = self.objects.get_field(recv, 0) else {
            return Ok(None);
        };
        let buf = buf as u16;
        // The receiver, probe and value live only in `args` for the whole
        // search — root them across the upcalls, as `sort` does.
        let mark = self.gc_state.push_shadow_roots(args);
        let r = self.equals_aware_collection_op_inner(frames, op, buf, probe, args);
        self.gc_state.truncate_shadow_roots(mark);
        r.map(Some)
    }

    pub(super) fn equals_aware_collection_op_inner(
        &mut self,
        frames: &mut Vec<Frame>,
        op: CollOp,
        buf: u16,
        probe: Value,
        args: &[Value],
    ) -> Result<Option<Value>, JvmError> {
        let is_list = matches!(op, CollOp::ListContains | CollOp::ListRemove);
        // One upcall per candidate; the length is re-read each round because
        // the override ran arbitrary Java.
        let mut found: Option<usize> = None;
        let mut pos = 0usize;
        loop {
            let candidate = if is_list {
                if pos >= self.objects.list_len(buf) {
                    break;
                }
                self.objects.list_get(buf, pos)
            } else {
                if pos >= self.objects.map_len(buf) {
                    break;
                }
                self.objects.map_entry_at(buf, pos).map(|(k, _)| k)
            };
            let Some(candidate) = candidate else {
                break;
            };
            if self.user_equals(frames, probe, candidate)? {
                found = Some(pos);
                break;
            }
            pos += 1;
        }
        let value_arg = args.get(2).copied().unwrap_or(Value::Null);
        let value_at = |ex: &Self, p: usize| {
            ex.objects
                .map_entry_at(buf, p)
                .map(|(_, v)| v)
                .unwrap_or(Value::Null)
        };
        let result = match op {
            CollOp::MapGet => found.map(|p| value_at(self, p)).unwrap_or(Value::Null),
            CollOp::MapGetOrDefault => found.map(|p| value_at(self, p)).unwrap_or(value_arg),
            CollOp::MapContainsKey | CollOp::SetContains | CollOp::ListContains => {
                Value::Int(found.is_some() as i32)
            }
            CollOp::MapRemove => match found {
                Some(p) => {
                    let old = value_at(self, p);
                    self.objects.map_remove_at(buf, p);
                    old
                }
                None => Value::Null,
            },
            CollOp::MapPut => match found {
                Some(p) => self
                    .objects
                    .map_set_value_at(buf, p, value_arg)
                    .unwrap_or(Value::Null),
                None => {
                    if self.objects.map_push(buf, probe, value_arg).is_err() {
                        return Err(self.runtime_fault(c::java_lang_OutOfMemoryError));
                    }
                    Value::Null
                }
            },
            CollOp::SetAdd => match found {
                Some(_) => Value::Int(0),
                None => {
                    if self.objects.map_push(buf, probe, Value::Int(1)).is_err() {
                        return Err(self.runtime_fault(c::java_lang_OutOfMemoryError));
                    }
                    Value::Int(1)
                }
            },
            CollOp::SetRemove => match found {
                Some(p) => {
                    self.objects.map_remove_at(buf, p);
                    Value::Int(1)
                }
                None => Value::Int(0),
            },
            CollOp::ListRemove => match found {
                Some(p) => {
                    self.objects.list_remove(buf, p);
                    Value::Int(1)
                }
                None => Value::Int(0),
            },
        };
        Ok(Some(result))
    }
}
