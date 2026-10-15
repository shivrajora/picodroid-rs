// SPDX-License-Identifier: GPL-3.0-only
//! Builder ↔ reader ↔ validator round trips over three fixtures: two
//! hand-assembled classes (the JVM's own `Base` and `Caller`) and a javac
//! class that exercises every region.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use crate::build::{build_section, class_index, link_class, unpooled_class};
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

/// A copy of some bytes at an 8-byte aligned address (a `Vec<u8>` promises
/// nothing), as the section reader requires: seven spare bytes, so an
/// 8-aligned window fits wherever the allocator put the buffer.
struct Aligned {
    bytes: Vec<u8>,
    start: usize,
}

fn aligned(bytes: &[u8]) -> Aligned {
    // Never resized after this, so the window stays where it is.
    let mut copy = vec![0u8; bytes.len() + 7];
    let start = (8 - copy.as_ptr() as usize % 8) % 8;
    copy[start..start + bytes.len()].copy_from_slice(bytes);
    Aligned { bytes: copy, start }
}

/// The first `len` bytes of the aligned copy.
fn as_bytes(v: &Aligned, len: usize) -> &[u8] {
    &v.bytes[v.start..v.start + len]
}

/// A linked section, aligned, to read classes out of.
struct Sec(Aligned, usize);

impl Sec {
    fn of(classes: &[&[u8]]) -> Sec {
        let sec = build_section(classes).unwrap();
        Sec(aligned(&sec), sec.len())
    }
    fn section(&self) -> ClassSection<'_> {
        ClassSection::parse(as_bytes(&self.0, self.1)).unwrap()
    }
    fn class(&self, i: usize) -> Linked<'_> {
        self.section().class(i).unwrap()
    }
}

/// `class` linked on its own: a one-class section.
fn one(class: &[u8]) -> Sec {
    let sec = link_class(class).unwrap();
    Sec(aligned(&sec), sec.len())
}

#[test]
fn base_links_and_validates() {
    let s = one(BASE);
    let l = s.class(0);
    l.link.validate(l.class).unwrap();
    assert_eq!(l.link.total_words() % 2, 0);
    // Five Utf8 entries, each down to its tag byte.
    let utf8_bytes = 4 + 16 + 5 + 3 + 4;
    assert_eq!(l.link.class_len(), BASE.len() - 5 * 2 - utf8_bytes);
    assert_eq!(unpooled_class(&l).unwrap(), BASE);
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
    // Every constant-pool word is the entry's data offset, one past its
    // tag — a Utf8's is its ref into the pool.
    for i in 1..l.link.cp_count() {
        let off = l.link.cp_offset(i).unwrap();
        if l.link.cp_tag(i) == Some(1) {
            assert!(l.link.utf8_ref(off).is_some());
        } else {
            assert_eq!(l.class[off - 1], l.link.cp_tag(i).unwrap());
        }
    }
    assert_eq!(l.utf8(2), Some(&b"Base"[..]));
    assert_eq!(l.cp_class_name(3), Some(&b"java/lang/Object"[..]));
    assert_eq!(l.utf8(1), None);
    assert_eq!(l.link.cp_tag(8), None);
}

#[test]
fn a_methodref_gets_a_descriptor() {
    let s = one(CALLER);
    let l = s.class(0);
    l.link.validate(l.class).unwrap();
    assert_eq!(l.link.mrefs_len(), 1);
    assert_eq!(l.link.cp_tag(8), Some(10));
    let d = l.link.methodref_desc(8).expect("descriptor");
    assert_eq!(d.argc(), 0);
    assert!(!d.is_interface());
    let off = d.cp_off as usize;
    assert_eq!(l.class[off - 1], 10);
    assert_eq!(&l.class[off..off + 4], &[0, 9, 0, 10]);
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
    let s = one(FIXTURE);
    let l = s.class(0);
    l.link.validate(l.class).unwrap();
    assert_eq!(unpooled_class(&l).unwrap(), FIXTURE);
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
            let count = classfile::be16(l.class, m.lnt_offset as usize).unwrap();
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
            assert_eq!(l.class[d.cp_off as usize - 1], l.link.cp_tag(i).unwrap());
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
    let s = one(FIXTURE);
    let fixture = s.class(0);
    let (class, pool) = (fixture.class, fixture.link.pool());
    let w = fixture.link.words().to_vec();
    // A String descriptor's literal id, the header's `super_idx` and its
    // pool words are the section's words, checked by
    // `ClassSection::validate` and `ClassSection::class`
    // (`a_section_checks_its_literals`, `a_section_links_superclasses`,
    // `a_section_checks_its_pool`). A Utf8 entry's ref is the class's
    // content, as its bytes were: it must name an entry, and what it names
    // is what derives the hashes.
    let link = Link::new(&w, pool).unwrap();
    assert!(link.strs_len() > 0, "the fixture has string constants");
    let mut external: Vec<usize> = (0..link.strs_len())
        .map(|k| link.strs_off() + 2 * k + 1)
        .collect();
    external.extend([
        layout::SUPER_IDX_WORD,
        layout::POOL_REL_WORD,
        layout::POOL_REL_WORD + 1,
        layout::POOL_WORDS_WORD,
    ]);
    external.extend(
        (0..link.cp_count())
            .filter(|&i| link.cp_tag(i) == Some(1))
            .map(|i| HEADER_WORDS + i),
    );
    assert_eq!(link.super_idx(), None);
    for i in 0..w.len() {
        if external.contains(&i) {
            continue;
        }
        let mut c = w.clone();
        c[i] ^= 0x0101;
        let detected = match Link::new(&c, pool) {
            Err(_) => true,
            Ok(l) => l.validate(class).is_err(),
        };
        assert!(detected, "word {i} corrupted undetected");
    }
    // Another class's bytes are not this table's.
    let base = one(BASE);
    assert_eq!(
        link.validate(base.class(0).class),
        Err(LinkError::WordMismatch { word: 2 })
    );
}

#[test]
fn a_table_accepted_by_new_never_panics_in_an_accessor() {
    // Whatever the header words say, `new` either refuses the table or
    // every region slice is in bounds.
    let s = one(FIXTURE);
    let fixture = s.class(0);
    let w = fixture.link.words().to_vec();
    for word in 0..HEADER_WORDS {
        for delta in [1u16, 0x100, 0x7FFF] {
            let mut c = w.clone();
            c[word] = c[word].wrapping_add(delta);
            if let Ok(l) = Link::new(&c, fixture.link.pool()) {
                let _ = l.fields().len()
                    + l.static_fields().len()
                    + l.interfaces().len()
                    + l.methods().len()
                    + l.methodrefs().len();
                for i in 0..l.cp_count() {
                    let _ = l.cp_offset(i);
                }
                let linked = Linked {
                    class: fixture.class,
                    link: l,
                };
                let _ = (linked.name(), linked.super_name());
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
    let at = CALLER
        .windows(5)
        .position(|w| w == [0x0A, 0x00, 0x09, 0x00, 0x0A])
        .unwrap();
    c[at + 2] = 2; // #2 is a Utf8
    assert_eq!(link_class(&c), Err(LinkError::BadMethodref { cp: 8 }));
    // A name that is not UTF-8.
    let mut c = BASE.to_vec();
    c[16] = 0xFF; // first byte of "Base"
    assert_eq!(link_class(&c), Err(LinkError::BadUtf8 { cp: 2 }));
    // An empty class name: ref 0 is the pool's empty string, which is how
    // "no superclass" is spelled.
    assert_eq!(
        link_class(&crate::build::minimal_class(b"")),
        Err(LinkError::BadThisClass)
    );
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
fn a_section_pools_its_utf8_constants_once() {
    // Base and Caller both spell java/lang/Object, Code, ()I and speak;
    // the pool holds each once, and both classes name the same bytes.
    let s = Sec::of(&[BASE, CALLER, FIXTURE]);
    let sec = s.section();
    sec.validate().unwrap();
    let pool = sec.pool();
    let mut entries: Vec<&[u8]> = Vec::new();
    let mut at = 0;
    while at < pool.len() {
        let e = classfile::utf8_at(pool, at).unwrap();
        entries.push(e);
        at = (at + 2 + e.len()).next_multiple_of(4);
    }
    let count = |needle: &[u8]| entries.iter().filter(|e| **e == needle).count();
    assert!(entries.iter().all(|e| count(e) == 1), "each string once");
    assert_eq!(count(b"java/lang/Object"), 1);
    assert_eq!(count(b"speak"), 1);
    let (base, caller) = (s.class(0), s.class(1));
    assert_eq!(base.super_name(), caller.super_name());
    assert_eq!(
        base.super_name().unwrap().as_ptr(),
        caller.super_name().unwrap().as_ptr()
    );
    // Entry 0 is the empty string.
    assert_eq!(&pool[..4], &[0, 0, 0, 0]);
    // Every class comes back as the class file it was.
    assert_eq!(unpooled_class(&base).unwrap(), BASE);
    assert_eq!(unpooled_class(&caller).unwrap(), CALLER);
    assert_eq!(unpooled_class(&s.class(2)).unwrap(), FIXTURE);
    // Classes that share names cost less pooled than as class files.
    let many: Vec<Vec<u8>> = (0..16u8)
        .map(|i| crate::build::minimal_class(&[b'c', b'/', b'A' + i]))
        .collect();
    let refs: Vec<&[u8]> = many.iter().map(|c| &c[..]).collect();
    let s = Sec::of(&refs);
    let plain: usize = many.iter().map(|c| c.len()).sum();
    let pooled: usize =
        (0..16).map(|i| s.class(i).class.len()).sum::<usize>() + s.section().pool().len();
    // Fifteen of the sixteen `java/lang/Object`s are gone, less padding.
    assert!(pooled + 15 * 12 < plain, "{pooled} vs {plain}");
}

#[test]
fn a_section_checks_its_pool() {
    let sec = build_section(&[BASE, CALLER]).unwrap();
    let check = |bytes: &[u8]| {
        let buf = aligned(bytes);
        ClassSection::parse(as_bytes(&buf, bytes.len())).and_then(|s| s.validate())
    };
    check(&sec).unwrap();
    let le32 = |at: usize| u32::from_le_bytes(sec[at..at + 4].try_into().unwrap()) as usize;
    let (pool_off, pool_len) = (le32(8), le32(12));
    assert_eq!(pool_off + pool_len, sec.len());
    // The empty first entry, every length word and every pad byte.
    let mut c = sec.clone();
    c[pool_off + 1] = 1;
    assert_eq!(check(&c), Err(LinkError::BadPool));
    let mut c = sec.clone();
    c[pool_off + 5] ^= 0x01; // the second entry's length
    assert!(check(&c).is_err());
    // A pool past the section, or not where the tables say.
    for (at, v) in [(8, pool_off + 4), (12, pool_len - 4), (12, pool_len + 4)] {
        let mut c = sec.clone();
        c[at..at + 4].copy_from_slice(&(v as u32).to_le_bytes());
        assert!(check(&c).is_err(), "header word at {at} = {v}");
    }
    let s = Sec::of(&[BASE, CALLER]);
    let base_off = s.class(0).link.as_ptr() as usize - s.section().as_bytes().as_ptr() as usize;
    for word in [layout::POOL_REL_WORD, layout::POOL_WORDS_WORD] {
        let mut c = sec.clone();
        c[base_off + 2 * word] ^= 0x04;
        let buf = aligned(&c);
        let parsed = ClassSection::parse(as_bytes(&buf, c.len())).unwrap();
        assert!(parsed.class(0).is_none(), "pool word {word}");
        assert_eq!(parsed.validate(), Err(LinkError::BadPool));
    }
    // A ref past the pool is refused.
    let base = s.class(0);
    let utf8 = (0..base.link.cp_count())
        .find(|&i| base.link.cp_tag(i) == Some(1))
        .unwrap();
    let mut c = sec.clone();
    let at = base_off + 2 * (HEADER_WORDS + utf8);
    c[at..at + 2].copy_from_slice(&0xFFFFu16.to_le_bytes());
    assert!(check(&c).is_err());
}

#[test]
fn a_section_checks_its_literals() {
    let sec = build_section(&[BASE, FIXTURE]).unwrap();
    let check = |bytes: &[u8]| {
        let buf = aligned(bytes);
        ClassSection::parse(as_bytes(&buf, bytes.len())).and_then(|s| s.validate())
    };
    check(&sec).unwrap();
    // Every byte of the literal rows, their count included, is
    // load-bearing.
    let (lits_at, lits_end) = {
        let buf = aligned(&sec);
        let s = ClassSection::parse(as_bytes(&buf, sec.len())).unwrap();
        let at = u32::from_le_bytes(sec[4..8].try_into().unwrap()) as usize + 8 * s.len();
        (at, at + 4 + 12 * s.literals().len())
    };
    for i in lits_at..lits_end {
        let mut c = sec.clone();
        c[i] ^= 0x01;
        assert!(check(&c).is_err(), "literal byte {i} corrupted undetected");
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
    // A truncated section is refused at parse.
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
    let s = one(FIXTURE);
    let l = s.class(0);
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
    assert_eq!(one(BASE).class(0).link.own_slots(), 0);
}

#[test]
fn from_raw_and_from_bytes_see_the_same_words() {
    let s = Sec::of(&[BASE, CALLER]);
    let caller = s.class(1);
    // SAFETY: a table the section handed out, alive for the call.
    let raw = unsafe { Link::from_raw(caller.link.as_ptr()) };
    assert_eq!(raw.words(), caller.link.words());
    assert_eq!(raw.pool(), caller.link.pool());
    assert_eq!(raw.pool().as_ptr(), s.section().pool().as_ptr());
    assert_eq!(
        Linked {
            class: caller.class,
            link: raw
        }
        .name(),
        Some(&b"Caller"[..])
    );
    let bytes = crate::build::link_bytes(caller.link.words());
    let buf = aligned(&bytes);
    let l = Link::from_bytes(as_bytes(&buf, bytes.len()), caller.link.pool()).unwrap();
    assert_eq!(l.words(), caller.link.words());
    assert_eq!(
        Link::from_bytes(&as_bytes(&buf, bytes.len())[1..], caller.link.pool()).err(),
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
    assert_eq!(unpooled_class(&l).unwrap(), BASE);
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
    let s = one(&bytes);
    let l = s.class(0);
    l.link.validate(l.class).unwrap();
    assert_eq!(unpooled_class(&l).unwrap(), bytes);
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

// ── Bytecode constraints are checked where the table is derived ──────────

#[test]
fn a_class_with_malformed_bytecode_does_not_link() {
    use crate::build::simple_class;
    // A well-formed method links and round-trips through a section.
    let ok = simple_class(b"Fine", b"java/lang/Object", Some(&[0x04, 0xAC]));
    link_class(&ok).expect("iconst_1; ireturn links");
    build_section(&[&ok]).expect("a section of it builds");
    // A truncated `wide`, a `tableswitch` with `low = i32::MIN`, a negative
    // `npairs` and a branch out of the code are each refused at the
    // instruction, so the interpreter never indexes past `code`.
    let mut huge_table = vec![0x03, 0xAA, 0, 0];
    for w in [13i32, i32::MIN, 1, 13, 13] {
        huge_table.extend_from_slice(&w.to_be_bytes());
    }
    huge_table.push(0xB1);
    let mut negative_pairs = vec![0x03, 0xAB, 0, 0];
    for w in [9i32, -1] {
        negative_pairs.extend_from_slice(&w.to_be_bytes());
    }
    negative_pairs.push(0xB1);
    for (code, pc) in [
        (&[0xC4u8, 0x15, 0x00][..], 0u16),
        (&huge_table[..], 1),
        (&negative_pairs[..], 1),
        (&[0xA7, 0x00, 0x40, 0xB1][..], 0),
    ] {
        let class = simple_class(b"Broken", b"java/lang/Object", Some(code));
        assert_eq!(
            link_class(&class),
            Err(LinkError::BadBytecode { pc }),
            "{code:x?}"
        );
        assert_eq!(
            build_section(&[&class]),
            Err(LinkError::BadBytecode { pc }),
            "{code:x?}"
        );
    }
}

// ── Superclass chains must leave the set ─────────────────────────────────

#[test]
fn a_superclass_cycle_does_not_build() {
    use crate::build::simple_class;
    let a_self = simple_class(b"A", b"A", None);
    assert_eq!(
        build_section(&[&a_self]),
        Err(LinkError::SuperCycle { idx: 0 }),
        "A extends A"
    );
    let a = simple_class(b"A", b"B", None);
    let b = simple_class(b"B", b"A", None);
    assert_eq!(
        build_section(&[&a, &b]),
        Err(LinkError::SuperCycle { idx: 0 }),
        "A extends B extends A"
    );
    // A chain that leaves the set is fine, however deep.
    let c = simple_class(b"C", b"B", None);
    let b = simple_class(b"B", b"A", None);
    let a = simple_class(b"A", b"java/lang/Object", None);
    let section = build_section(&[&c, &b, &a]).expect("C extends B extends A extends Object");
    let aligned = aligned(&section);
    ClassSection::parse(as_bytes(&aligned, section.len()))
        .expect("parses")
        .validate()
        .expect("validates");
}
