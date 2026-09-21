// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── Enum native method tests ─────────────────────────────────────────────

#[test]
fn enum_init_name_ordinal() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);

    let name = dispatch_enum(m::name, d::__String, &[red], &mut strings, &mut objects)
        .unwrap()
        .unwrap();
    let Value::Reference(idx) = name else {
        panic!("expected Reference");
    };
    assert_eq!(strings.resolve(idx), Some("RED"));

    assert_eq!(
        dispatch_enum(m::ordinal, "()I", &[red], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn enum_to_string() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let green = make_enum_instance(&mut objects, &mut strings, b"GREEN", 1);

    let result = dispatch_enum(
        m::toString,
        d::__String,
        &[green],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    let Value::Reference(idx) = result else {
        panic!("expected Reference");
    };
    assert_eq!(strings.resolve(idx), Some("GREEN"));
}

#[test]
fn enum_equals_same() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);

    assert_eq!(
        dispatch_enum(
            m::equals,
            d::Object__Z,
            &[red, red],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn enum_equals_different() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);
    let green = make_enum_instance(&mut objects, &mut strings, b"GREEN", 1);

    assert_eq!(
        dispatch_enum(
            m::equals,
            d::Object__Z,
            &[red, green],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn enum_compare_to() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);
    let blue = make_enum_instance(&mut objects, &mut strings, b"BLUE", 2);

    // RED(0).compareTo(BLUE(2)) = -2
    assert_eq!(
        dispatch_enum(
            m::compareTo,
            d::Enum__I,
            &[red, blue],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(-2)))
    );
    // BLUE(2).compareTo(RED(0)) = 2
    assert_eq!(
        dispatch_enum(
            m::compareTo,
            d::Enum__I,
            &[blue, red],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(2)))
    );
}
