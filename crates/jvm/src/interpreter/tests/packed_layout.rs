// SPDX-License-Identifier: GPL-3.0-only
//! `new` over packed sections: the chain is followed by the superclass
//! index each table carries, across the app → framework step, and the
//! fields get their typed defaults from the kinds the packer recorded.
use super::asm::Asm;
use super::literals::section;
use super::*;
use crate::class_file::find_class;
use crate::names::c;
use crate::Jvm;

fn class(name: &str, sup: &str, fields: &[(&str, &str)]) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class(name);
    let sup = a.class(sup);
    for (n, d) in fields {
        a.field(n, d);
    }
    a.finish(0x0001, this, sup, &[], None)
}

fn loaded() -> Jvm {
    // Framework: Root(int x; long y) <- Mid(Object o). App: Leaf(float f;
    // double d; boolean z) extends Mid, and an enum-like class on
    // java/lang/Enum, which has no class file.
    let root = class("PRoot", c::java_lang_Object, &[("x", "I"), ("y", "J")]);
    let mid = class("PMid", "PRoot", &[("o", "Ljava/lang/Object;")]);
    let leaf = class("PLeaf", "PMid", &[("f", "F"), ("d", "D"), ("z", "Z")]);
    let color = class("PColor", c::java_lang_Enum, &[("rgb", "I")]);
    let mut jvm = Jvm::new();
    // Mid before Root: the index, not the order, finds the superclass.
    jvm.load_framework(section(&[mid, root])).unwrap();
    jvm.load_app(section(&[color, leaf])).unwrap();
    jvm
}

#[test]
fn the_chain_is_walked_by_superclass_index() {
    let jvm = loaded();
    let classes = jvm.classes();
    let at = |n: &str| find_class(classes, n.as_bytes()).unwrap();
    let (root, mid, leaf, color) = (at("PRoot"), at("PMid"), at("PLeaf"), at("PColor"));
    // In-section steps come from the table; the app → framework step is
    // the one lookup; Object and Enum have no class file.
    assert_eq!(classes[mid].link().super_idx(), Some(root));
    assert_eq!(classes[leaf].link().super_idx(), None);
    assert_eq!(classes.super_of(mid), Some(root));
    assert_eq!(classes.super_of(leaf), Some(mid));
    assert_eq!(classes.super_of(root), None);
    assert_eq!(classes.super_of(color), None);
    assert_eq!(helpers::instance_slot_count(classes, "PLeaf"), Some(8));
}

#[test]
fn new_lays_out_the_chain_root_first_with_typed_defaults() {
    let jvm = loaded();
    let classes = jvm.classes();
    let leaf = find_class(classes, b"PLeaf").unwrap();
    let mut objects = ObjectHeap::new();
    let by_index = objects.alloc_instance(leaf, classes).unwrap();
    let by_name = objects.alloc_with_defaults("PLeaf", classes).unwrap();
    for obj in [by_index, by_name] {
        assert_eq!(objects.class_name(obj), Some("PLeaf"));
        // Root: x @0, y @1-2. Mid: o @3. Leaf: f @4, d @5-6, z @7.
        assert_eq!(objects.get_field(obj, 0), Some(Value::Int(0)));
        assert_eq!(objects.get_field(obj, 1), Some(Value::Long(0)));
        assert_eq!(objects.get_field(obj, 3), Some(Value::Null));
        assert_eq!(objects.get_field(obj, 4), Some(Value::Float(0.0)));
        assert_eq!(objects.get_field(obj, 5), Some(Value::Double(0.0)));
        assert_eq!(objects.get_field(obj, 7), Some(Value::Int(0)));
        assert_eq!(objects.get_field(obj, 8), None);
        for (name, slot) in [("x", 0), ("y", 1), ("o", 3), ("f", 4), ("d", 5), ("z", 7)] {
            assert_eq!(helpers::field_slot(classes, "PLeaf", name), Some(slot));
        }
    }
    // A class on java/lang/Enum starts after Enum's two implicit slots.
    let color = find_class(classes, b"PColor").unwrap();
    let e = objects.alloc_instance(color, classes).unwrap();
    assert_eq!(objects.get_field(e, 2), Some(Value::Int(0)));
    assert_eq!(helpers::field_slot(classes, "PColor", "rgb"), Some(2));
    // A class with no fields anywhere on its chain writes nothing.
    let bare = class("PBare", c::java_lang_Object, &[]);
    let mut jvm2 = Jvm::new();
    jvm2.load_framework(section(&[bare])).unwrap();
    let classes2 = jvm2.classes();
    let b = objects.alloc_instance(0, classes2).unwrap();
    assert_eq!(objects.get_field(b, 0), None);
}
