// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── Iterator native method tests ─────────────────────────────────────────

#[test]
fn iterator_arraylist_empty() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();

    // Create iterator via ArrayList.iterator()
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // hasNext should be false immediately
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_arraylist_basic() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(10)], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(20)], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(30)], &mut objects).unwrap();

    // Create iterator
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // Iterate: hasNext/next cycle
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(30)))
    );
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_arraylist_single() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(42)], &mut objects).unwrap();

    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(42)))
    );
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_next_past_end() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();

    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // next() on empty iterator should error
    assert!(dispatch_iter(m::next, d::__Object, &[iter], &mut objects).is_err());
}

#[test]
fn iterator_hashmap_keys() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(2), Value::Int(20)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // keySet()
    let keyset = dispatch_map(m::keySet, d::__Set, &[map], &mut strings, &mut objects)
        .unwrap()
        .unwrap();

    // keySet().iterator()
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[keyset],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_HashMap_KeySet, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // Collect keys
    let mut keys = alloc::vec::Vec::new();
    while dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects)
        .unwrap()
        .unwrap()
        == Value::Int(1)
    {
        let k = dispatch_iter(m::next, d::__Object, &[iter], &mut objects)
            .unwrap()
            .unwrap();
        keys.push(k);
    }
    assert_eq!(keys.len(), 2);
    // Keys should be Int(1) and Int(2) (order not guaranteed, but our impl preserves insertion order)
    assert!(keys.contains(&Value::Int(1)));
    assert!(keys.contains(&Value::Int(2)));
}

#[test]
fn hashmap_key_and_value_views_answer_contains() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    let keyset = dispatch_map(m::keySet, d::__Set, &[map], &mut strings, &mut objects)
        .unwrap()
        .unwrap();
    let values = dispatch_map(
        m::values,
        d::__Collection,
        &[map],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    let mut arrays = ArrayHeap::new();
    let mut probe = |class: &str, view: Value, needle: Value| -> Value {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::Object__Z,
            args: &[view, needle],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(class, m::contains, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        probe(c::java_util_HashMap_KeySet, keyset, Value::Int(1)),
        Value::Int(1)
    );
    assert_eq!(
        probe(c::java_util_HashMap_KeySet, keyset, Value::Int(10)),
        Value::Int(0)
    );
    assert_eq!(
        probe(c::java_util_HashMap_Values, values, Value::Int(10)),
        Value::Int(1)
    );
    assert_eq!(
        probe(c::java_util_HashMap_Values, values, Value::Int(1)),
        Value::Int(0)
    );
}

#[test]
fn iterator_hashmap_values() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(2), Value::Int(20)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // values()
    let vals = dispatch_map(
        m::values,
        d::__Collection,
        &[map],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();

    // values().iterator()
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[vals],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_HashMap_Values, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    let mut values = alloc::vec::Vec::new();
    while dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects)
        .unwrap()
        .unwrap()
        == Value::Int(1)
    {
        let v = dispatch_iter(m::next, d::__Object, &[iter], &mut objects)
            .unwrap()
            .unwrap();
        values.push(v);
    }
    assert_eq!(values.len(), 2);
    assert!(values.contains(&Value::Int(10)));
    assert!(values.contains(&Value::Int(20)));
}

// ── bugbash S6: Iterator.remove and fail-fast iteration ───────────────────

#[test]
fn iterator_remove_removes_the_last_returned_element() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    for v in [10, 20, 30] {
        dispatch_list(m::add, d::Object__Z, &[list, Value::Int(v)], &mut objects).unwrap();
    }
    let iter = make_list_iterator(&mut objects, list);
    // remove() before next() is IllegalStateException.
    let r = dispatch_iter(m::remove, "()V", &[iter], &mut objects);
    let Err(JvmError::Exception(e)) = r else {
        panic!("{r:?}");
    };
    assert_eq!(
        objects.class_name(e),
        Some(c::java_lang_IllegalStateException)
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(10)))
    );
    dispatch_iter(m::remove, "()V", &[iter], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(2)))
    );
    // Iteration continues over the survivors.
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(30)))
    );
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_detects_concurrent_modification() {
    // Removing through the collection mid-iteration used to silently skip
    // every other element; java.util fails fast in next().
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    for v in [1, 2, 3, 4] {
        dispatch_list(m::add, d::Object__Z, &[list, Value::Int(v)], &mut objects).unwrap();
    }
    let iter = make_list_iterator(&mut objects, list);
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    dispatch_list(
        m::remove,
        d::I__Object,
        &[list, Value::Int(0)],
        &mut objects,
    )
    .unwrap();
    let r = dispatch_iter(m::next, d::__Object, &[iter], &mut objects);
    let Err(JvmError::Exception(e)) = r else {
        panic!("{r:?}");
    };
    assert_eq!(
        objects.class_name(e),
        Some(c::java_util_ConcurrentModificationException)
    );
}
