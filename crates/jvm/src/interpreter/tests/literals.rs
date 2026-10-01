// SPDX-License-Identifier: GPL-3.0-only
//! String constants through a section's literal pool: `ldc` pushes a row
//! number, nothing is interned, and a constant two class sets both spell
//! is still one object.
use super::asm::Asm;
use super::*;
use crate::class_objects::ClassObjectCache;
use crate::names::c;
use crate::Jvm;
use alloc::vec;
use class_link::{string_hash, ClassSection};

/// A class with one `String` constant per entry of `strings`, in order;
/// returns the class bytes and each constant's CP index.
fn class_with(name: &str, strings: &[&str]) -> (&'static [u8], Vec<u16>) {
    let mut a = Asm::new();
    let this = a.class(name);
    let obj = a.class(c::java_lang_Object);
    let idx = strings.iter().map(|s| a.string(s)).collect();
    (a.finish(0x0001, this, obj, &[], None), idx)
}

/// A packed section over `classes`, leaked at a 4-aligned address as a
/// flash image would be.
pub(super) fn section(classes: &[&[u8]]) -> ClassSection<'static> {
    let bytes = class_link::build::build_section(classes).expect("section");
    // Three spare bytes, then the copy starts at the first 4-aligned one.
    let buf = vec![0u8; bytes.len() + 3].leak();
    let skip = buf.as_ptr().align_offset(4);
    let data = &mut buf[skip..skip + bytes.len()];
    data.copy_from_slice(&bytes);
    let data: &'static [u8] = data;
    let s = ClassSection::parse(data).expect("parse");
    s.validate().expect("validate");
    s
}

struct Loaded {
    jvm: Jvm,
    strings: StringTable,
    objects: ObjectHeap,
    class_objects: ClassObjectCache,
    fw_idx: Vec<u16>,
    app_idx: Vec<u16>,
}

fn load(fw: &[&str], app: &[&str]) -> Loaded {
    let (fw_class, fw_idx) = class_with("LitFw", fw);
    let (app_class, app_idx) = class_with("LitApp", app);
    let mut jvm = Jvm::new();
    jvm.load_framework(section(&[fw_class])).unwrap();
    jvm.load_app(section(&[app_class])).unwrap();
    let mut strings = StringTable::new();
    let (f, a) = jvm.literal_pools();
    strings.set_literal_pools(f, a);
    Loaded {
        jvm,
        strings,
        objects: ObjectHeap::new(),
        class_objects: ClassObjectCache::new(),
        fw_idx,
        app_idx,
    }
}

impl Loaded {
    fn ldc(&mut self, class: usize, cp: u16) -> u16 {
        let classes = self.jvm.classes();
        match helpers::resolve_ldc(
            &classes[class],
            classes,
            &mut self.strings,
            &mut self.objects,
            &mut self.class_objects,
            cp,
        ) {
            Ok(Value::Reference(r)) => r,
            other => panic!("ldc of a String gave {other:?}"),
        }
    }
}

#[test]
fn ldc_of_a_pooled_constant_interns_nothing() {
    let mut l = load(&["hello", "fw only", "hello"], &["world"]);
    let (a, b, again) = (l.fw_idx[0], l.fw_idx[1], l.fw_idx[2]);
    let hello = l.ldc(0, a);
    assert!(l.strings.is_literal(hello));
    assert_eq!(l.strings.resolve(hello), Some("hello"));
    assert_eq!(l.strings.resolve_static(hello), Some("hello"));
    // Two entries spelling one string are one row, so one reference.
    assert_eq!(l.ldc(0, again), hello);
    let fw_only = l.ldc(0, b);
    assert_ne!(fw_only, hello);
    assert_eq!(l.strings.resolve(fw_only), Some("fw only"));
    let world = l.ldc(1, l.app_idx[0]);
    assert_eq!(l.strings.resolve(world), Some("world"));
    // The app's pool sits above the framework's, both at the top of the range.
    assert!(world > fw_only && fw_only > hello);
    // Nothing went into the table, before or after dynamic strings exist.
    assert_eq!(l.strings.total_len(), 0);
    let dynamic = l.strings.intern_dyn(b"hel+lo").unwrap();
    assert!(!l.strings.is_literal(dynamic));
    assert_eq!(l.ldc(0, a), hello);
    assert_eq!(l.strings.total_len(), 1);
    // A literal is never collected and never a dynamic entry.
    assert!(!l.strings.is_dyn_live(hello));
    l.strings.free_dyn(hello);
    assert_eq!(l.strings.resolve(hello), Some("hello"));
}

#[test]
fn a_constant_both_sets_spell_is_one_object() {
    let mut l = load(&["shared", "fw"], &["app", "shared"]);
    let fw_shared = l.ldc(0, l.fw_idx[0]);
    let app_shared = l.ldc(1, l.app_idx[1]);
    let fw_other = l.ldc(0, l.fw_idx[1]);
    let app_other = l.ldc(1, l.app_idx[0]);
    assert_ne!(fw_shared, app_shared, "a row in each pool");
    assert!(l.strings.same_literal(fw_shared, app_shared));
    assert!(l.strings.same_literal(app_shared, fw_shared));
    assert!(l.strings.same_literal(fw_shared, fw_shared));
    assert!(!l.strings.same_literal(fw_shared, fw_other));
    assert!(!l.strings.same_literal(fw_other, app_other));
    assert!(l.strings.content_eq(fw_shared, app_shared));
    // A dynamic string with the same text is a different object.
    let dynamic = l.strings.intern_dyn(b"shared").unwrap();
    assert!(!l.strings.same_literal(dynamic, fw_shared));
    assert!(l.strings.content_eq(dynamic, fw_shared));
}

#[test]
fn a_literals_hash_is_its_pool_rows() {
    let mut l = load(&["polygenelubricants", ""], &[]);
    let s = l.ldc(0, l.fw_idx[0]);
    let empty = l.ldc(0, l.fw_idx[1]);
    assert_eq!(
        l.strings.literal_hash(s),
        Some(string_hash(b"polygenelubricants") as i32)
    );
    // Java's value for that string: the hash is `String.hashCode()`.
    assert_eq!(l.strings.literal_hash(s), Some(i32::MIN));
    assert_eq!(l.strings.literal_hash(empty), Some(0));
    let dynamic = l.strings.intern_dyn(b"x").unwrap();
    assert_eq!(l.strings.literal_hash(dynamic), None);
}

#[test]
fn a_class_outside_a_section_still_interns() {
    // `ClassFile::parse` links the class alone: no pool, no literal ids.
    let (class, idx) = class_with("LitLoose", &["loose"]);
    let classes = vec![ClassFile::parse(class).unwrap()];
    assert_eq!(
        classes[0].cp_string_literal(idx[0]),
        Some(class_link::LIT_NONE)
    );
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut class_objects = ClassObjectCache::new();
    let v = helpers::resolve_ldc(
        &classes[0],
        crate::class_file::Classes::linear(&classes),
        &mut strings,
        &mut objects,
        &mut class_objects,
        idx[0],
    );
    let Ok(Value::Reference(r)) = v else {
        panic!("{v:?}")
    };
    assert!(!strings.is_literal(r));
    assert_eq!(strings.resolve(r), Some("loose"));
    assert_eq!(strings.total_len(), 1);
}

#[test]
fn the_table_cannot_grow_into_the_literals() {
    let mut l = load(&["a"], &["b"]);
    let lit = l.ldc(0, l.fw_idx[0]);
    // Fill the table up to the pool's floor; the next intern is refused
    // rather than handed a literal's reference.
    let mut last = 0;
    while let Some(r) = l.strings.intern_dyn(b"") {
        assert!(r < lit);
        last = r;
    }
    assert_eq!(last + 1, lit);
    assert_eq!(l.strings.resolve(lit), Some("a"));
}
