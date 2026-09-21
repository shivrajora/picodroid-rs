// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── ArrayList / Collections tests ─────────────────────────────────────────

#[test]
fn arraylist_init_and_size() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_list(m::isEmpty, "()Z", &[list], &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn arraylist_add_and_get() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(10)], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(20)], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(m::get, d::I__Object, &[list, Value::Int(0)], &mut objects),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_list(m::get, d::I__Object, &[list, Value::Int(1)], &mut objects),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn arraylist_set_returns_old() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(1)], &mut objects).unwrap();
    // set(0, 99) returns the old value Int(1)
    assert_eq!(
        dispatch_list(
            m::set,
            d::I_Object__Object,
            &[list, Value::Int(0), Value::Int(99)],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_list(m::get, d::I__Object, &[list, Value::Int(0)], &mut objects),
        Ok(Some(Value::Int(99)))
    );
}

#[test]
fn arraylist_to_array_keeps_object_zero() {
    // The first object an executor allocates lives in slot 0; a round trip
    // through an Object[] must hand it back, not null.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let first = objects.alloc("Foo").unwrap();
    assert_eq!(first, 0);
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(
        m::add,
        d::Object__Z,
        &[list, Value::ObjectRef(first)],
        &mut objects,
    )
    .unwrap();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: d::__aObject,
        args: &[list],
        strings: &mut strings,
        objects: &mut objects,
        arrays: &mut arrays,
        upcall: None,
    };
    let arr = BuiltinHandler
        .dispatch(c::java_util_ArrayList, m::toArray, &mut ctx)
        .unwrap()
        .unwrap()
        .unwrap();
    let Value::ArrayRef(a) = arr else {
        panic!("expected ArrayRef");
    };
    let raw = arrays.load(a, 0).unwrap();
    assert_eq!(crate::array_heap::decode_ref(raw), Value::ObjectRef(0));
}

#[test]
fn arraylist_contains_matches_string_content() {
    // A literal and a runtime-built string with the same text are distinct
    // References; HashMap compared contents, ArrayList compared indices.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    let lit = Value::Reference(strings.intern(b"ab").unwrap());
    let dynamic = Value::Reference(strings.intern_dyn(b"ab").unwrap());
    assert_ne!(lit, dynamic);
    dispatch_list(m::add, d::Object__Z, &[list, lit], &mut objects).unwrap();
    let mut call = |m: &str, d: &str, args: &[Value], objects: &mut ObjectHeap| {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d,
            args,
            strings: &mut strings,
            objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m, &mut ctx)
            .unwrap()
    };
    let obj = d::Object__Z;
    assert_eq!(
        call(m::contains, obj, &[list, dynamic], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        call(m::remove, obj, &[list, dynamic], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        call(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn arraylist_index_bounds_throw_index_out_of_bounds() {
    // add(i, v) clamped (a negative index appended!), set(i, v) silently
    // returned null; get/remove were uncatchable hard errors.
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    for v in [1, 2] {
        dispatch_list(m::add, d::Object__Z, &[list, Value::Int(v)], &mut objects).unwrap();
    }
    let ioobe = |r: Result<Option<Value>, JvmError>, objects: &ObjectHeap, what: &str| {
        let Err(JvmError::Exception(idx)) = r else {
            panic!("{what}: {r:?}");
        };
        assert_eq!(
            objects.class_name(idx),
            Some(c::java_lang_IndexOutOfBoundsException),
            "{what}"
        );
    };
    let addi = d::I_Object__V;
    let r = dispatch_list(
        m::add,
        addi,
        &[list, Value::Int(5), Value::Int(9)],
        &mut objects,
    );
    ioobe(r, &objects, "add(5)");
    let r = dispatch_list(
        m::add,
        addi,
        &[list, Value::Int(-1), Value::Int(9)],
        &mut objects,
    );
    ioobe(r, &objects, "add(-1)");
    // add(size, v) is legal and appends.
    dispatch_list(
        m::add,
        addi,
        &[list, Value::Int(2), Value::Int(3)],
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(3)))
    );
    let seti = d::I_Object__Object;
    let r = dispatch_list(
        m::set,
        seti,
        &[list, Value::Int(9), Value::Int(0)],
        &mut objects,
    );
    ioobe(r, &objects, "set(9)");
    let r = dispatch_list(m::get, d::I__Object, &[list, Value::Int(9)], &mut objects);
    ioobe(r, &objects, "get(9)");
    let r = dispatch_list(m::get, d::I__Object, &[list, Value::Int(-1)], &mut objects);
    ioobe(r, &objects, "get(-1)");
    let r = dispatch_list(
        m::remove,
        d::I__Object,
        &[list, Value::Int(3)],
        &mut objects,
    );
    ioobe(r, &objects, "remove(3)");
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn arraylist_remove_object_overload() {
    // remove(Object) compiles to (Ljava/lang/Object;)Z; the arm demanded an
    // Int index and threw InvalidReference for it.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    let a = Value::Reference(strings.intern(b"a").unwrap());
    let b = Value::Reference(strings.intern(b"b").unwrap());
    let five = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(five, 0, Value::Int(5));
    let five2 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(five2, 0, Value::Int(5));
    for v in [a, b, Value::ObjectRef(five)] {
        dispatch_list(m::add, d::Object__Z, &[list, v], &mut objects).unwrap();
    }
    let mut call = |m: &str, d: &str, args: &[Value], objects: &mut ObjectHeap| {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d,
            args,
            strings: &mut strings,
            objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m, &mut ctx)
            .unwrap()
    };
    let rm = d::Object__Z;
    assert_eq!(
        call(m::remove, rm, &[list, a], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        call(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(2)))
    );
    assert_eq!(
        call(m::remove, rm, &[list, a], &mut objects),
        Ok(Some(Value::Int(0)))
    );
    // A different boxed Integer with the same value matches (equals semantics).
    assert_eq!(
        call(
            m::remove,
            rm,
            &[list, Value::ObjectRef(five2)],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        call(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    // The index overload still works and returns the element.
    assert_eq!(
        call(
            m::remove,
            d::I__Object,
            &[list, Value::Int(0)],
            &mut objects
        ),
        Ok(Some(b))
    );
}

#[test]
fn arraylist_remove() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(5)], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(
            m::remove,
            d::I__Object,
            &[list, Value::Int(0)],
            &mut objects
        ),
        Ok(Some(Value::Int(5)))
    );
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn arraylist_contains() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(7)], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(
            m::contains,
            d::Object__Z,
            &[list, Value::Int(7)],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_list(
            m::contains,
            d::Object__Z,
            &[list, Value::Int(8)],
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}
