// SPDX-License-Identifier: GPL-3.0-only
//! Builder ↔ reader ↔ validator round trips over three fixtures: two
//! hand-assembled classes (the JVM's own `Base` and `Caller`) and a javac
//! class that exercises every region.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use crate::build::{build_section, class_index, link_bytes, link_class};
use crate::fixture::FIXTURE;
use crate::*;

// Class "Base" extends Object, method speak()I { iconst_1; ireturn }.
static BASE: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00, 0x02, 0x01, 0x00, 0x04,
    b'B', b'a', b's', b'e', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l',
    b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x01, 0x00, 0x05, b's', b'p', b'e',
    b'a', b'k', 0x01, 0x00, 0x03, b'(', b')', b'I', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', 0x00,
    0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x05, 0x00,
    0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00,
    0x02, 0x04, 0xAC, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// Class "Caller", static m(LBase;)I { aload_0; invokevirtual #8 Base.speak()I; ireturn }.
static CALLER: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, 0x07, 0x00, 0x02, 0x01, 0x00, 0x06,
    b'C', b'a', b'l', b'l', b'e', b'r', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a',
    b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x01, 0x00, 0x01, b'm',
    0x01, 0x00, 0x09, b'(', b'L', b'B', b'a', b's', b'e', b';', b')', b'I', 0x01, 0x00, 0x04, b'C',
    b'o', b'd', b'e', 0x0A, 0x00, 0x09, 0x00, 0x0A, 0x07, 0x00, 0x0B, 0x0C, 0x00, 0x0C, 0x00, 0x0D,
    0x01, 0x00, 0x04, b'B', b'a', b's', b'e', 0x01, 0x00, 0x05, b's', b'p', b'e', b'a', b'k', 0x01,
    0x00, 0x03, b'(', b')', b'I', 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x11, 0x00,
    0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x2A, 0xB6, 0x00, 0x08, 0xAC, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00,
];

fn linked<'a>(class: &'a [u8], words: &'a [u16]) -> Linked<'a> {
    Linked {
        class,
        link: Link::new(words).expect("header"),
    }
}

/// A copy of `bytes` at an 8-byte aligned address (a `Vec<u8>` promises
/// nothing), as the section reader requires.
fn aligned(bytes: &[u8]) -> Vec<u64> {
    let mut v = vec![0u64; bytes.len().div_ceil(8)];
    // SAFETY: `v` holds at least `bytes.len()` bytes; a `u8` view of `u64`s.
    let dst = unsafe { core::slice::from_raw_parts_mut(v.as_mut_ptr().cast::<u8>(), bytes.len()) };
    dst.copy_from_slice(bytes);
    v
}

fn as_bytes(v: &[u64], len: usize) -> &[u8] {
    // SAFETY: `v` holds at least `len` bytes.
    unsafe { core::slice::from_raw_parts(v.as_ptr().cast::<u8>(), len) }
}

#[test]
fn base_links_and_validates() {
    let w = link_class(BASE).unwrap();
    let l = linked(BASE, &w);
    l.link.validate(BASE).unwrap();
    assert_eq!(l.link.total_words() % 2, 0);
    assert_eq!(l.link.class_len(), BASE.len());
    assert_eq!(l.name(), Some(&b"Base"[..]));
    assert_eq!(l.link.name_hash(), name_hash(b"Base"));
    // The superclass is stored as written, Object included; the runtime
    // recognises Object by its hash.
    assert_eq!(l.super_name(), Some(&b"java/lang/Object"[..]));
    assert_eq!(l.link.super_hash(), name_hash(b"java/lang/Object"));
    assert_eq!(l.link.cp_count(), 8);
    assert_eq!(l.link.fields_len(), 0);
    assert_eq!(l.link.statics_len(), 0);
    assert_eq!(l.link.ifaces_len(), 0);
    assert_eq!(l.link.methods_len(), 1);
    assert_eq!(l.link.mrefs_len(), 0);
    assert_eq!(l.link.bsm_off(), 0);
    assert_eq!(l.link.access_flags(), 1);
    let m = &l.link.methods()[0];
    assert_eq!(l.method_name(m), Some(&b"speak"[..]));
    assert_eq!(l.method_descriptor(m), Some(&b"()I"[..]));
    assert_eq!(m.sig_hash(), sig_hash(b"speak", b"()I"));
    assert_eq!(m.access_flags, 1);
    assert_eq!(m.lnt_offset, 0);
    assert!(m.has_code());
    assert_eq!(l.method_max_stack(m), 1);
    assert_eq!(l.method_max_locals(m), 2);
    assert_eq!(l.method_code_len(m), 2);
    assert_eq!(l.method_code(m), &[0x04, 0xAC]);
    assert_eq!(l.link.method_index(m), Some(0));
    // Every constant-pool word is the entry's data offset, one past its tag.
    for i in 1..l.link.cp_count() {
        let off = l.link.cp_offset(i).unwrap();
        assert_eq!(BASE[off - 1], l.link.cp_tag(i).unwrap());
    }
    assert_eq!(l.utf8(2), Some(&b"Base"[..]));
    assert_eq!(l.cp_class_name(3), Some(&b"java/lang/Object"[..]));
    assert_eq!(l.utf8(1), None);
    assert_eq!(l.link.cp_tag(8), None);
}

#[test]
fn a_methodref_gets_a_descriptor() {
    let w = link_class(CALLER).unwrap();
    let l = linked(CALLER, &w);
    l.link.validate(CALLER).unwrap();
    assert_eq!(l.link.mrefs_len(), 1);
    assert_eq!(l.link.cp_tag(8), Some(10));
    let d = l.link.methodref_desc(8).expect("descriptor");
    assert_eq!(d.argc(), 0);
    assert!(!d.is_interface());
    let off = d.cp_off as usize;
    assert_eq!(CALLER[off - 1], 10);
    assert_eq!(&CALLER[off..off + 4], &[0, 9, 0, 10]);
    assert_eq!(l.link.cp_offset(8), Some(off));
    assert_eq!(
        l.cp_member_ref(8),
        Some((&b"Base"[..], &b"speak"[..], &b"()I"[..]))
    );
    assert_eq!(l.cp_name_and_type(10), Some((&b"speak"[..], &b"()I"[..])));
    assert!(l.link.methodref_desc(9).is_none());
    assert!(l.link.methodref_desc(0).is_none());
    // The static method: one parameter, `max_locals` 1.
    let m = &l.link.methods()[0];
    assert_eq!(m.access_flags, 0x0008);
    assert_eq!(l.method_max_stack(m), 2);
    assert_eq!(l.method_max_locals(m), 1);
    assert_eq!(l.method_code(m), &[0x2A, 0xB6, 0x00, 0x08, 0xAC]);
}

#[test]
fn the_javac_fixture_covers_every_region() {
    let w = link_class(FIXTURE).unwrap();
    let l = linked(FIXTURE, &w);
    l.link.validate(FIXTURE).unwrap();
    assert_eq!(l.name(), Some(&b"Fixture"[..]));
    assert_eq!(l.link.fields_len(), 2);
    assert_eq!(l.link.statics_len(), 2);
    assert_eq!(l.link.ifaces_len(), 2);
    assert!(l.link.mrefs_len() >= 3, "{}", l.link.mrefs_len());
    assert_ne!(l.link.bsm_off(), 0, "string concat needs BootstrapMethods");

    let iface_names: Vec<&[u8]> = l
        .link
        .interfaces()
        .iter()
        .map(|f| l.interface_name(f).unwrap())
        .collect();
    assert_eq!(
        iface_names,
        [&b"java/lang/Runnable"[..], &b"java/lang/Comparable"[..]]
    );
    for f in l.link.interfaces() {
        assert_eq!(f.hash(), name_hash(l.interface_name(f).unwrap()));
    }

    let names =
        |fs: &[FieldInfo]| -> Vec<&[u8]> { fs.iter().map(|f| l.field_name(f).unwrap()).collect() };
    assert_eq!(names(l.link.fields()), [&b"value"[..], &b"ratio"[..]]);
    assert_eq!(
        names(l.link.static_fields()),
        [&b"BIG"[..], &b"counter"[..]]
    );
    assert_eq!(
        l.field_descriptor(&l.link.static_fields()[0]),
        Some(&b"J"[..])
    );

    let mut saw_native = false;
    for (i, m) in l.link.methods().iter().enumerate() {
        assert_eq!(l.link.method_index(m), Some(i));
        let n = l.method_name(m).unwrap();
        let d = l.method_descriptor(m).unwrap();
        assert_eq!(m.sig_hash(), sig_hash(n, d));
        if n == b"probe" {
            saw_native = true;
            assert_eq!(m.access_flags & 0x0100, 0x0100);
            assert!(!m.has_code());
            assert_eq!(l.method_code(m), &[]);
            assert_eq!(l.method_max_locals(m), 0);
            assert_eq!(m.lnt_offset, 0);
        } else {
            assert!(m.code_offset >= 8);
            assert_ne!(m.lnt_offset, 0, "{}", core::str::from_utf8(n).unwrap());
            assert!(l.method_max_stack(m) > 0);
            assert_eq!(l.method_code(m).len(), l.method_code_len(m));
            // The LineNumberTable body starts with its entry count, ≥ 1.
            let count = classfile::be16(FIXTURE, m.lnt_offset as usize).unwrap();
            assert!(count >= 1);
        }
    }
    assert!(saw_native);

    // Every Methodref's argc is the descriptor's parameter count, and the
    // interface flag follows the tag.
    let mut mrefs = 0;
    for i in 0..l.link.cp_count() {
        if let Some(d) = l.link.methodref_desc(i) {
            mrefs += 1;
            let (_, _, desc) = l.cp_member_ref(i).unwrap();
            assert_eq!(Some(d.argc()), count_args(desc));
            assert_eq!(d.is_interface(), l.link.cp_tag(i) == Some(11));
            assert_eq!(FIXTURE[d.cp_off as usize - 1], l.link.cp_tag(i).unwrap());
        }
    }
    assert_eq!(mrefs, l.link.mrefs_len());
    assert!((0..l.link.cp_count()).any(|i| l.link.cp_tag(i) == Some(11)));

    // A Long takes two slots: the pad slot has tag 0 and word 0.
    let long_idx = (0..l.link.cp_count())
        .find(|&i| l.link.cp_tag(i) == Some(5))
        .expect("a Long constant");
    assert_eq!(l.link.cp_tag(long_idx + 1), Some(0));
    assert_eq!(l.link.cp_word(long_idx + 1), Some(0));
    assert_eq!(l.link.cp_offset(long_idx + 1), Some(0));
    // A Fieldref resolves through the shared member-ref layout.
    let fref = (0..l.link.cp_count())
        .find(|&i| l.link.cp_tag(i) == Some(9))
        .unwrap();
    let (c, n, d) = l.cp_member_ref(fref).unwrap();
    assert_eq!(c, b"Fixture");
    assert!(n == b"ratio" || n == b"value" || n == b"counter");
    assert!(d == b"D" || d == b"I");
}

#[test]
fn every_word_is_load_bearing() {
    let w = link_class(FIXTURE).unwrap();
    // A String descriptor's literal id and the header's `super_idx` are
    // the section's words, checked by `ClassSection::validate`
    // (`a_section_checks_its_literals`, `a_section_links_superclasses`).
    let link = Link::new(&w).unwrap();
    assert!(link.strs_len() > 0, "the fixture has string constants");
    let mut external: Vec<usize> = (0..link.strs_len())
        .map(|k| link.strs_off() + 2 * k + 1)
        .collect();
    external.push(layout::SUPER_IDX_WORD);
    assert_eq!(link.super_idx(), None);
    for i in 0..w.len() {
        if external.contains(&i) {
            assert_eq!(w[i], u16::MAX, "LIT_NONE / SUPER_NONE");
            continue;
        }
        let mut c = w.clone();
        c[i] ^= 0x0101;
        let detected = match Link::new(&c) {
            Err(_) => true,
            Ok(l) => l.validate(FIXTURE).is_err(),
        };
        assert!(detected, "word {i} corrupted undetected");
    }
    // Another class's bytes are not this table's.
    let l = Link::new(&w).unwrap();
    assert_eq!(l.validate(BASE), Err(LinkError::WordMismatch { word: 2 }));
}

#[test]
fn a_table_accepted_by_new_never_panics_in_an_accessor() {
    // Whatever the header words say, `new` either refuses the table or
    // every region slice is in bounds.
    let w = link_class(FIXTURE).unwrap();
    for word in 0..HEADER_WORDS {
        for delta in [1u16, 0x100, 0x7FFF] {
            let mut c = w.clone();
            c[word] = c[word].wrapping_add(delta);
            if let Ok(l) = Link::new(&c) {
                let _ = l.fields().len()
                    + l.static_fields().len()
                    + l.interfaces().len()
                    + l.methods().len()
                    + l.methodrefs().len();
                for i in 0..l.cp_count() {
                    let _ = l.cp_offset(i);
                }
            }
        }
    }
}

#[test]
fn rejects_malformed_classes() {
    assert_eq!(link_class(&[]), Err(LinkError::Truncated));
    assert_eq!(
        link_class(&[0, 1, 2, 3, 0, 0, 0, 0, 0, 1]),
        Err(LinkError::BadMagic)
    );
    let mut c = BASE.to_vec();
    c[10] = 99;
    assert_eq!(
        link_class(&c),
        Err(LinkError::UnknownTag { cp: 1, tag: 99 })
    );
    let big = vec![0u8; 70_000];
    assert_eq!(link_class(&big), Err(LinkError::ClassTooLarge));
    // A Methodref whose class slot is not a Class entry.
    let mut c = CALLER.to_vec();
    let w = link_class(CALLER).unwrap();
    let off = linked(CALLER, &w).link.methodref_desc(8).unwrap().cp_off as usize;
    c[off + 1] = 2; // #2 is a Utf8
    assert_eq!(link_class(&c), Err(LinkError::BadMethodref { cp: 8 }));
    // A name that is not UTF-8.
    let mut c = BASE.to_vec();
    c[16] = 0xFF; // first byte of "Base"
    assert_eq!(link_class(&c), Err(LinkError::BadUtf8 { cp: 2 }));
}

#[test]
fn a_section_pools_its_string_constants() {
    // The same class twice over would be a duplicate, so pool one class:
    // every String entry gets a row, equal strings share one, and a row's
    // bytes, hash and flags are the constant's.
    let sec = build_section(&[BASE, FIXTURE]).unwrap();
    let buf = aligned(&sec);
    let s = ClassSection::parse(as_bytes(&buf, sec.len())).unwrap();
    s.validate().unwrap();
    let lits = s.literals();
    assert!(!lits.is_empty());
    let fixture = s.class(1).unwrap();
    assert!(fixture.link.strs_len() >= lits.len());
    for i in 0..fixture.link.cp_count() {
        let Some(d) = fixture.link.string_desc(i) else {
            assert_ne!(fixture.link.cp_tag(i), Some(8));
            continue;
        };
        let bytes = fixture.cp_string_utf8(i).unwrap();
        assert_eq!(fixture.string_bytes(d), Some(bytes));
        assert_eq!(lits.bytes(d.lit as usize), Some(bytes));
        let row = lits.row(d.lit as usize).unwrap();
        assert_eq!(row.hash, string_hash(bytes));
        assert_eq!(row.flags, LIT_UTF8);
        assert_eq!(row.len as usize, bytes.len());
        assert_eq!(fixture.link.cp_offset(i), Some(d.cp_off as usize));
    }
    // No two rows hold the same bytes.
    for a in 0..lits.len() {
        for b in a + 1..lits.len() {
            assert_ne!(lits.bytes(a), lits.bytes(b));
        }
    }
    assert_eq!(string_hash(b"ab"), 31 * 97 + 98);
    assert_eq!(string_hash(b""), 0);
}

#[test]
fn a_section_checks_its_literals() {
    let sec = build_section(&[BASE, FIXTURE]).unwrap();
    let check = |bytes: &[u8]| {
        let buf = aligned(bytes);
        ClassSection::parse(as_bytes(&buf, bytes.len())).and_then(|s| s.validate())
    };
    check(&sec).unwrap();
    // Every byte of the pool, its count included, is load-bearing.
    let pool_at = {
        let buf = aligned(&sec);
        let s = ClassSection::parse(as_bytes(&buf, sec.len())).unwrap();
        sec.len() - 4 - 12 * s.literals().len()
    };
    for i in pool_at..sec.len() {
        let mut c = sec.clone();
        c[i] ^= 0x01;
        assert!(check(&c).is_err(), "pool byte {i} corrupted undetected");
    }
    // So is every class's literal id.
    let buf = aligned(&sec);
    let s = ClassSection::parse(as_bytes(&buf, sec.len())).unwrap();
    let fixture = s.class(1).unwrap();
    let words_at = fixture.link.as_ptr() as usize - as_bytes(&buf, sec.len()).as_ptr() as usize;
    for k in 0..fixture.link.strs_len() {
        let at = words_at + 2 * (fixture.link.strs_off() + 2 * k + 1);
        let mut c = sec.clone();
        c[at] ^= 0x01;
        assert_eq!(check(&c), Err(LinkError::BadLiteral), "literal id {k}");
    }
    // A truncated pool is refused at parse.
    assert!(check(&sec[..sec.len() - 4]).is_err());
}

/// `class <name> extends <sup>`, no members.
fn subclass(name: &[u8], sup: &[u8]) -> Vec<u8> {
    let mut c = crate::build::minimal_class(name);
    // `minimal_class` spells java/lang/Object as Utf8 #4; respell it.
    let at = c
        .windows(16)
        .position(|w| w == b"java/lang/Object")
        .unwrap();
    let mut out = c[..at - 2].to_vec();
    out.extend_from_slice(&(sup.len() as u16).to_be_bytes());
    out.extend_from_slice(sup);
    out.extend_from_slice(&c.split_off(at + 16));
    out
}

#[test]
fn a_section_links_superclasses() {
    // C extends B extends A; A extends Object, which the section lacks.
    let a = subclass(b"A", b"java/lang/Object");
    let b = subclass(b"B", b"A");
    let c = subclass(b"C", b"B");
    let sec = build_section(&[&c, &a, &b]).unwrap();
    let check = |bytes: &[u8]| {
        let buf = aligned(bytes);
        ClassSection::parse(as_bytes(&buf, bytes.len())).and_then(|s| s.validate())
    };
    check(&sec).unwrap();
    let buf = aligned(&sec);
    let s = ClassSection::parse(as_bytes(&buf, sec.len())).unwrap();
    assert_eq!(s.class(0).unwrap().link.super_idx(), Some(2)); // C -> B
    assert_eq!(s.class(1).unwrap().link.super_idx(), None); // A -> Object
    assert_eq!(s.class(2).unwrap().link.super_idx(), Some(1)); // B -> A
                                                               // A wrong index, a missing one and an invented one are all refused.
    let word = |i: usize| {
        s.class(i).unwrap().link.as_ptr() as usize - as_bytes(&buf, sec.len()).as_ptr() as usize
            + 2 * layout::SUPER_IDX_WORD
    };
    for (class, value) in [(0usize, 1u16), (0, SUPER_NONE), (1, 0), (2, 2)] {
        let mut bad = sec.clone();
        bad[word(class)..word(class) + 2].copy_from_slice(&value.to_le_bytes());
        assert_eq!(
            check(&bad),
            Err(LinkError::BadSuperIndex { idx: class as u16 })
        );
    }
}

#[test]
fn fields_carry_their_slot_and_kind() {
    let w = link_class(FIXTURE).unwrap();
    let l = linked(FIXTURE, &w);
    // Fixture: `int value; double ratio;` and statics `long BIG; int counter`.
    let f = l.link.fields();
    assert_eq!((f[0].slot(), f[0].kind()), (0, KIND_INT));
    assert_eq!((f[1].slot(), f[1].kind()), (1, KIND_DOUBLE));
    assert_eq!(l.link.own_slots(), 3);
    let st = l.link.static_fields();
    assert_eq!((st[0].slot(), st[0].kind()), (0, KIND_LONG));
    assert_eq!((st[1].slot(), st[1].kind()), (1, KIND_INT));
    for fi in f.iter().chain(st) {
        assert_eq!(fi.kind(), field_kind(l.field_descriptor(fi).unwrap()));
    }
    assert_eq!(field_kind(b"Ljava/lang/String;"), KIND_REF);
    assert_eq!(field_kind(b"[I"), KIND_REF);
    assert_eq!(field_kind(b"F"), KIND_FLOAT);
    assert_eq!(field_kind(b"Z"), KIND_INT);
    assert_eq!(kind_slots(KIND_LONG), 2);
    assert_eq!(kind_slots(KIND_REF), 1);
    let base = link_class(BASE).unwrap();
    assert_eq!(linked(BASE, &base).link.own_slots(), 0);
}

#[test]
fn from_raw_and_from_bytes_see_the_same_words() {
    let w = link_class(CALLER).unwrap();
    // SAFETY: `w` is a table `new` accepts, alive for the call.
    let raw = unsafe { Link::from_raw(w.as_ptr()) };
    assert_eq!(raw.words(), &w[..]);
    let bytes = link_bytes(&w);
    assert_eq!(bytes.len(), 2 * w.len());
    let buf = aligned(&bytes);
    let l = Link::from_bytes(as_bytes(&buf, bytes.len())).unwrap();
    assert_eq!(l.words(), &w[..]);
    assert_eq!(
        Link::from_bytes(&as_bytes(&buf, bytes.len())[1..]).err(),
        Some(LinkError::Misaligned)
    );
}

#[test]
fn a_section_round_trips_and_validates() {
    let sec = build_section(&[BASE, CALLER, FIXTURE]).unwrap();
    let buf = aligned(&sec);
    let s = ClassSection::parse(as_bytes(&buf, sec.len())).unwrap();
    s.validate().unwrap();
    assert_eq!(s.len(), 3);
    assert_eq!(s.class(0).unwrap().name(), Some(&b"Base"[..]));
    assert_eq!(s.class(2).unwrap().name(), Some(&b"Fixture"[..]));
    assert!(s.class(3).is_none());
    assert_eq!(s.find_class(b"Caller"), Some(1));
    assert_eq!(s.find_class(b"Fixture"), Some(2));
    assert_eq!(s.find_class(b"Nope"), None);
    assert_eq!(s.find(name_hash(b"Base")).len(), 1);
    assert_eq!(s.find(0xDEAD_BEEF).len(), 0);
    let index = s.index();
    assert!(index.windows(2).all(|p| p[0].hash < p[1].hash));
    for e in index {
        assert_eq!(e.hash, s.class(e.idx as usize).unwrap().link.name_hash());
    }
    for l in s.classes() {
        assert_eq!(l.link.as_ptr() as usize % 4, 0);
        assert_eq!(l.class.as_ptr() as usize % 4, 0);
        l.link.validate(l.class).unwrap();
    }
    assert_eq!(s.classes().count(), 3);
}

#[test]
fn a_section_rejects_duplicates_truncation_and_misalignment() {
    assert_eq!(
        build_section(&[BASE, BASE]),
        Err(LinkError::DuplicateClass { a: 0, b: 1 })
    );
    let sec = build_section(&[BASE]).unwrap();
    let buf = aligned(&sec);
    assert!(ClassSection::parse(as_bytes(&buf, sec.len() - 4)).is_err());
    assert_eq!(
        ClassSection::parse(&as_bytes(&buf, sec.len())[1..]).err(),
        Some(LinkError::Misaligned)
    );
    // A corrupted class word inside a section is caught by validate.
    let s = ClassSection::parse(as_bytes(&buf, sec.len())).unwrap();
    let l = s.class(0).unwrap();
    let word_off = l.link.as_ptr() as usize - s.as_bytes().as_ptr() as usize + 2 * 12;
    let mut bad = sec.clone();
    bad[word_off] ^= 1;
    let buf = aligned(&bad);
    let s = ClassSection::parse(as_bytes(&buf, bad.len())).unwrap();
    assert_eq!(s.validate(), Err(LinkError::WordMismatch { word: 12 }));
    // An empty section is fine.
    let sec = build_section(&[]).unwrap();
    let buf = aligned(&sec);
    let s = ClassSection::parse(as_bytes(&buf, sec.len())).unwrap();
    s.validate().unwrap();
    assert!(s.is_empty());
    assert_eq!(s.find_class(b"Base"), None);
}

#[test]
fn a_named_section_checks_each_class_spells_its_name() {
    use crate::build::build_section_named;
    let ok = build_section_named(&[(b"Base", BASE), (b"Caller", CALLER)]).unwrap();
    assert_eq!(ok, build_section(&[BASE, CALLER]).unwrap());
    assert_eq!(
        build_section_named(&[(b"Base", BASE), (b"Wrong", CALLER)]),
        Err(LinkError::NameMismatch { idx: 1 })
    );
}

#[test]
fn a_minimal_class_links_and_names_itself() {
    use crate::build::minimal_class;
    let bytes = minimal_class(b"pkg/Tiny");
    let w = link_class(&bytes).unwrap();
    let l = linked(&bytes, &w);
    l.link.validate(&bytes).unwrap();
    assert_eq!(l.name(), Some(&b"pkg/Tiny"[..]));
    assert_eq!(l.super_name(), Some(&b"java/lang/Object"[..]));
    assert_eq!(l.link.methods_len(), 0);
    assert_eq!(l.link.access_flags(), 0x0021);
    let a = minimal_class(b"A");
    let b = minimal_class(b"B");
    build_section(&[&a, &b, BASE]).unwrap();
}

#[test]
fn class_index_refuses_a_hash_collision() {
    // Two different names with one FNV-1a hash, found by birthday search.
    let mut seen: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    let mut pair = None;
    for i in 0u32.. {
        let mut name = b"c/".to_vec();
        let mut n = i;
        loop {
            name.push(b'a' + (n % 26) as u8);
            n /= 26;
            if n == 0 {
                break;
            }
        }
        let h = name_hash(&name);
        if let Some(other) = seen.insert(h, name.clone()) {
            pair = Some((other, name));
            break;
        }
    }
    let (a, b) = pair.unwrap();
    assert_ne!(a, b);
    assert_eq!(
        class_index(&[&a, &b]),
        Err(LinkError::HashCollision { a: 0, b: 1 })
    );
    assert_eq!(
        class_index(&[&a, &a]),
        Err(LinkError::DuplicateClass { a: 0, b: 1 })
    );
    let ok = class_index(&[b"B", b"A", b"C"]).unwrap();
    assert!(ok.windows(2).all(|p| p[0].hash < p[1].hash));
    assert_eq!(ok.iter().map(|e| e.idx).sum::<u16>(), 3);
}

#[test]
fn the_fixture_source_is_kept_for_regeneration() {
    assert!(crate::fixture::FIXTURE_SOURCE.contains("class Fixture implements Runnable"));
}

#[test]
fn error_display_names_the_index() {
    let s = alloc::format!("{}", LinkError::BadMethodref { cp: 8 });
    assert!(s.contains("Methodref") && s.contains("#8"));
    assert!(!LinkError::Truncated.as_str().is_empty());
}
