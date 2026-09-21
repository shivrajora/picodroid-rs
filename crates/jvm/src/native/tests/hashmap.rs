// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── HashMap native method tests ──────────────────────────────────────────

#[test]
fn hashmap_init_and_size() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_map(m::isEmpty, "()Z", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn hashmap_put_and_get() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    // put(1, 10), put(2, 20), put(3, 30)
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
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(3), Value::Int(30)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(2)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(3)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(30)))
    );
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn hashmap_put_overwrite() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    // put(1, 10) returns null (no previous)
    assert_eq!(
        dispatch_map(
            m::put,
            d::Object_Object__Object,
            &[map, Value::Int(1), Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
    // put(1, 99) returns old value 10
    assert_eq!(
        dispatch_map(
            m::put,
            d::Object_Object__Object,
            &[map, Value::Int(1), Value::Int(99)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn hashmap_get_missing() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(42)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
}

#[test]
fn hashmap_remove() {
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
    assert_eq!(
        dispatch_map(
            m::remove,
            d::Object__Object,
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_remove_missing() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    assert_eq!(
        dispatch_map(
            m::remove,
            d::Object__Object,
            &[map, Value::Int(99)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
}

#[test]
fn hashmap_contains_key() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(5), Value::Int(50)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            m::containsKey,
            d::Object__Z,
            &[map, Value::Int(5)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_map(
            m::containsKey,
            d::Object__Z,
            &[map, Value::Int(6)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_contains_value() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(42)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            m::containsValue,
            d::Object__Z,
            &[map, Value::Int(42)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_map(
            m::containsValue,
            d::Object__Z,
            &[map, Value::Int(99)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_clear() {
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
    dispatch_map(m::clear, "()V", &[map], &mut strings, &mut objects).unwrap();
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_get_or_default() {
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
    // Key present: returns value
    assert_eq!(
        dispatch_map(
            m::getOrDefault,
            d::Object_Object__Object,
            &[map, Value::Int(1), Value::Int(-1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    // Key absent: returns default
    assert_eq!(
        dispatch_map(
            m::getOrDefault,
            d::Object_Object__Object,
            &[map, Value::Int(99), Value::Int(-1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(-1)))
    );
}

#[test]
fn hashmap_integer_keys() {
    // Test with boxed Integer objects as keys (wrapper equality via field 0)
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    // Create two Integer(42) objects at different heap slots
    let int1 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int1, 0, Value::Int(42));
    let int2 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int2, 0, Value::Int(42));
    assert_ne!(int1, int2); // different heap slots

    // put with int1 as key
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::ObjectRef(int1), Value::Int(100)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // get with int2 as key — should find it via wrapper equality
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::ObjectRef(int2)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(100)))
    );
}

#[test]
fn hashmap_string_keys() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    let key_a = Value::Reference(strings.intern(m::alpha.as_bytes()).unwrap());
    let key_b = Value::Reference(strings.intern(b"beta").unwrap());

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, key_a, Value::Int(1)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, key_b, Value::Int(2)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, key_a],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, key_b],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn hashmap_null_key() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Null, Value::Int(77)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Null],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(77)))
    );
}

#[test]
fn hashmap_null_value() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    // put(1, null)
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Null],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    // containsKey should return true
    assert_eq!(
        dispatch_map(
            m::containsKey,
            d::Object__Z,
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    // get returns Null (same as "not found"), but containsKey distinguishes
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
}

// ── HashSet native method tests ──────────────────────────────────────────

#[test]
fn hashset_add_contains_remove() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);

    // add(10) returns true (was absent)
    assert_eq!(
        dispatch_set(
            m::add,
            d::Object__Z,
            &[set, Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_set(
            m::contains,
            d::Object__Z,
            &[set, Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_set(m::size, "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );

    // remove(10) returns true (was present)
    assert_eq!(
        dispatch_set(
            m::remove,
            d::Object__Z,
            &[set, Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_set(m::size, "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashset_add_duplicate() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);

    dispatch_set(
        m::add,
        d::Object__Z,
        &[set, Value::Int(5)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    // Second add returns false (was already present)
    assert_eq!(
        dispatch_set(
            m::add,
            d::Object__Z,
            &[set, Value::Int(5)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_set(m::size, "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn hashset_iterator_visits_every_element() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);
    for v in [7, 3, 7, 9] {
        dispatch_set(
            m::add,
            d::Object__Z,
            &[set, Value::Int(v)],
            &mut strings,
            &mut objects,
        )
        .unwrap();
    }
    let iter = dispatch_set(
        m::iterator,
        d::__Iterator,
        &[set],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    let mut seen = alloc::vec::Vec::new();
    while dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects)
        .unwrap()
        .unwrap()
        == Value::Int(1)
    {
        seen.push(
            dispatch_iter(m::next, d::__Object, &[iter], &mut objects)
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(seen.len(), 3);
    for v in [3, 7, 9] {
        assert!(seen.contains(&Value::Int(v)));
    }
}

#[test]
fn hashset_clear() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);

    dispatch_set(
        m::add,
        d::Object__Z,
        &[set, Value::Int(1)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_set(
        m::add,
        d::Object__Z,
        &[set, Value::Int(2)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_set(m::clear, "()V", &[set], &mut strings, &mut objects).unwrap();
    assert_eq!(
        dispatch_set(m::size, "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_set(m::isEmpty, "()Z", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

// ── Regression: integer-key map then string-key map ──────────────────────

#[test]
fn hashmap_int_then_string_keys_shared_heap() {
    // Reproduces the sim bug: creating a HashMap with Integer keys, using
    // StringBuilder, then creating a second HashMap with string keys fails
    // to find the string keys.
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();

    // Map 1: Integer keys
    let m1 = Value::ObjectRef(objects.alloc(c::java_util_HashMap).unwrap());
    dispatch_map("<init>", "()V", &[m1], &mut strings, &mut objects).unwrap();

    // Integer.valueOf(1) — alloc Integer, set field 0
    let int1 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int1, 0, Value::Int(1));
    let int10 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int10, 0, Value::Int(10));

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[m1, Value::ObjectRef(int1), Value::ObjectRef(int10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // Verify m1.get works
    let int1b = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int1b, 0, Value::Int(1));
    let result = dispatch_map(
        m::get,
        d::Object__Object,
        &[m1, Value::ObjectRef(int1b)],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(result, Value::ObjectRef(int10));

    // StringBuilder usage (simulating "v1=" + v1)
    let _sb = objects.alloc(c::java_lang_StringBuilder).unwrap();
    let sb_buf = objects.sb_alloc().unwrap();
    objects.sb_append_bytes(sb_buf, b"v1=");
    objects.sb_append_int(sb_buf, 10);
    let sb_bytes = objects.sb_contents_slice(sb_buf).to_vec();
    let _str_idx = strings.intern_dyn(&sb_bytes).unwrap();

    // Map 2: String keys
    let m2 = Value::ObjectRef(objects.alloc(c::java_util_HashMap).unwrap());
    dispatch_map("<init>", "()V", &[m2], &mut strings, &mut objects).unwrap();

    let hello = Value::Reference(strings.intern(b"hello").unwrap());
    let world = Value::Reference(strings.intern(b"world").unwrap());

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[m2, hello, world],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // This should find "hello" → "world"
    let result = dispatch_map(
        m::get,
        d::Object__Object,
        &[m2, hello],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(result, world);
}

// ── entrySet / views / LinkedHash* aliases / toArray / append(null) ────────

#[test]
fn entry_set_iterates_key_value_pairs() {
    let mut cx = StrCtx::new();
    let map = new_map(&mut cx, c::java_util_HashMap);
    let k1 = cx.intern(b"one");
    let k2 = cx.intern(b"two");
    let put = d::Object_Object__Object;
    dispatch_on(
        &mut cx,
        c::java_util_HashMap,
        m::put,
        put,
        &[map, k1, Value::Int(1)],
    )
    .unwrap();
    dispatch_on(
        &mut cx,
        c::java_util_HashMap,
        m::put,
        put,
        &[map, k2, Value::Int(2)],
    )
    .unwrap();

    let view = dispatch_on(&mut cx, c::java_util_HashMap, m::entrySet, d::__Set, &[map])
        .unwrap()
        .unwrap();
    let Value::ObjectRef(vi) = view else {
        panic!("entrySet returned {view:?}");
    };
    assert_eq!(
        cx.objects.class_name(vi),
        Some(c::java_util_HashMap_EntrySet)
    );
    assert_eq!(
        dispatch_on(
            &mut cx,
            c::java_util_HashMap_EntrySet,
            m::size,
            "()I",
            &[view]
        )
        .unwrap(),
        Some(Value::Int(2))
    );
    let it = dispatch_on(
        &mut cx,
        c::java_util_HashMap_EntrySet,
        m::iterator,
        d::__Iterator,
        &[view],
    )
    .unwrap()
    .unwrap();

    let mut pairs: alloc::vec::Vec<(Value, Value)> = alloc::vec::Vec::new();
    while dispatch_on(&mut cx, c::java_util_Iterator, m::hasNext, "()Z", &[it]).unwrap()
        == Some(Value::Int(1))
    {
        let e = dispatch_on(&mut cx, c::java_util_Iterator, m::next, OBJ_DESC, &[it])
            .unwrap()
            .unwrap();
        let Value::ObjectRef(ei) = e else {
            panic!("next returned {e:?}");
        };
        assert_eq!(cx.objects.class_name(ei), Some(c::java_util_Map_Entry));
        let k = dispatch_on(&mut cx, c::java_util_Map_Entry, m::getKey, OBJ_DESC, &[e])
            .unwrap()
            .unwrap();
        let v = dispatch_on(&mut cx, c::java_util_Map_Entry, m::getValue, OBJ_DESC, &[e])
            .unwrap()
            .unwrap();
        pairs.push((k, v));
    }
    assert_eq!(pairs.len(), 2);
    assert!(pairs.contains(&(k1, Value::Int(1))));
    assert!(pairs.contains(&(k2, Value::Int(2))));
    // Past the end.
    assert!(dispatch_on(&mut cx, c::java_util_Iterator, m::next, OBJ_DESC, &[it]).is_err());
}

#[test]
fn key_set_and_values_views_pick_their_source_by_class() {
    let mut cx = StrCtx::new();
    let map = new_map(&mut cx, c::java_util_HashMap);
    let k = cx.intern(b"k");
    let put = d::Object_Object__Object;
    dispatch_on(
        &mut cx,
        c::java_util_HashMap,
        m::put,
        put,
        &[map, k, Value::Int(9)],
    )
    .unwrap();
    for (method, class, expect) in [
        (m::keySet, c::java_util_HashMap_KeySet, k),
        (m::values, c::java_util_HashMap_Values, Value::Int(9)),
    ] {
        let view = dispatch_on(&mut cx, c::java_util_HashMap, method, d::__Set, &[map])
            .unwrap()
            .unwrap();
        let it = dispatch_on(&mut cx, class, m::iterator, d::__Iterator, &[view])
            .unwrap()
            .unwrap();
        assert_eq!(
            dispatch_on(&mut cx, c::java_util_Iterator, m::next, OBJ_DESC, &[it]).unwrap(),
            Some(expect)
        );
        assert_eq!(
            dispatch_on(&mut cx, class, m::size, "()I", &[view]).unwrap(),
            Some(Value::Int(1))
        );
    }
}

#[test]
fn linked_hash_map_and_set_alias_the_hash_dispatchers() {
    let mut cx = StrCtx::new();
    let map = new_map(&mut cx, c::java_util_LinkedHashMap);
    let k = cx.intern(b"k");
    let put = d::Object_Object__Object;
    dispatch_on(
        &mut cx,
        c::java_util_LinkedHashMap,
        m::put,
        put,
        &[map, k, Value::Int(5)],
    )
    .unwrap();
    assert_eq!(
        dispatch_on(
            &mut cx,
            c::java_util_LinkedHashMap,
            m::get,
            d::Object__Object,
            &[map, k]
        )
        .unwrap(),
        Some(Value::Int(5))
    );
    assert_eq!(
        dispatch_on(&mut cx, c::java_util_LinkedHashMap, m::size, "()I", &[map]).unwrap(),
        Some(Value::Int(1))
    );

    let set = new_map(&mut cx, c::java_util_LinkedHashSet);
    let add = d::Object__Z;
    assert_eq!(
        dispatch_on(&mut cx, c::java_util_LinkedHashSet, m::add, add, &[set, k]).unwrap(),
        Some(Value::Int(1))
    );
    assert_eq!(
        dispatch_on(&mut cx, c::java_util_LinkedHashSet, m::add, add, &[set, k]).unwrap(),
        Some(Value::Int(0))
    );
    assert_eq!(
        dispatch_on(
            &mut cx,
            c::java_util_LinkedHashSet,
            m::contains,
            add,
            &[set, k]
        )
        .unwrap(),
        Some(Value::Int(1))
    );
}

#[test]
fn to_array_copies_every_reference_kind() {
    let mut cx = StrCtx::new();
    let list = Value::ObjectRef(cx.objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_on(&mut cx, c::java_util_ArrayList, "<init>", "()V", &[list]).unwrap();
    let s = cx.intern(b"s");
    let o = Value::ObjectRef(cx.objects.alloc("O").unwrap());
    let arr = Value::ArrayRef(cx.arrays.alloc(crate::array_heap::ATYPE_INT, 1).unwrap());
    for v in [s, o, Value::Null, arr] {
        dispatch_on(
            &mut cx,
            c::java_util_ArrayList,
            m::add,
            d::Object__Z,
            &[list, v],
        )
        .unwrap();
    }
    let out = dispatch_on(
        &mut cx,
        c::java_util_ArrayList,
        m::toArray,
        d::aObject__aObject,
        &[list, Value::Null],
    )
    .unwrap()
    .unwrap();
    let Value::ArrayRef(ai) = out else {
        panic!("toArray returned {out:?}");
    };
    assert_eq!(cx.arrays.atype(ai), Some(crate::array_heap::ATYPE_REF));
    assert_eq!(cx.arrays.length(ai), Some(4));
    for (i, v) in [s, o, Value::Null, arr].into_iter().enumerate() {
        assert_eq!(
            crate::array_heap::decode_ref(cx.arrays.load(ai, i).unwrap()),
            v
        );
    }
}

#[test]
fn append_null_and_value_of_object_on_string_or_null() {
    let mut cx = StrCtx::new();
    let sb = Value::ObjectRef(cx.objects.alloc(c::java_lang_StringBuilder).unwrap());
    dispatch_on(&mut cx, c::java_lang_StringBuilder, "<init>", "()V", &[sb]).unwrap();
    dispatch_on(
        &mut cx,
        c::java_lang_StringBuilder,
        m::append,
        d::Object__StringBuilder,
        &[sb, Value::Null],
    )
    .unwrap();
    let s = dispatch_on(
        &mut cx,
        c::java_lang_StringBuilder,
        m::toString,
        d::__String,
        &[sb],
    )
    .unwrap()
    .unwrap();
    assert_eq!(cx.resolve(s), "null");

    let ab = cx.intern(b"ab");
    let desc = d::Object__String;
    assert_eq!(
        dispatch_on(&mut cx, c::java_lang_String, m::valueOf, desc, &[ab]).unwrap(),
        Some(ab)
    );
    let n = dispatch_on(
        &mut cx,
        c::java_lang_String,
        m::valueOf,
        desc,
        &[Value::Null],
    )
    .unwrap()
    .unwrap();
    assert_eq!(cx.resolve(n), "null");
}
