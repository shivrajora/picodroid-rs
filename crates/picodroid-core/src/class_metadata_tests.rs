// SPDX-License-Identifier: GPL-3.0-only
//! The embedded framework class set as the JVM loads it.
//!
//! Every class arrives with the link table `build.rs` built for it, in one
//! class section shared with the PAPK format (docs/designs/class-link-2026-09.md).
//! These pin, over the whole corpus rather than a fixture: that the section
//! and every table validate against the bytes rustc embedded; that
//! registering the set costs the class table alone — 12 B a class on the
//! device, no parsed record, nothing on the heap per class; and that every
//! class is found by name through the section's index, with the answer the
//! JVM's own lookup gives.

use pico_jvm::class_file::{find_class, ClassFile, Classes, CLASS_FILE_DELTA};
use pico_jvm::Jvm;

use crate::framework_classes::{class_files, framework_section, FRAMEWORK_CLASS_COUNT};

fn framework_classes() -> Vec<ClassFile> {
    let classes = class_files();
    assert!(
        !classes.is_empty(),
        "FRAMEWORK_CLSS is empty — run via scripts/test.sh, which sets PICODROID_APK_PATH"
    );
    classes
}

/// Every table re-derives from its class bytes, and re-linking the class
/// files on the host gives the section byte for byte: the embedded section
/// is what the builder produces, not a stale or hand-edited blob. The class
/// files come back from the section itself (`unpooled_class` puts each
/// class's Utf8 constants back from the shared pool), so this also pins
/// that round trip over the whole corpus.
#[test]
fn framework_section_relinks_byte_for_byte() {
    let section = framework_section();
    section
        .validate()
        .expect("framework class section validates");
    let class_files: Vec<Vec<u8>> = section
        .classes()
        .map(|l| class_link::build::unpooled_class(&l).expect("unpool"))
        .collect();
    assert_eq!(class_files.len(), FRAMEWORK_CLASS_COUNT);
    let refs: Vec<&[u8]> = class_files.iter().map(|c| c.as_slice()).collect();
    let rebuilt = class_link::build::build_section(&refs).expect("relink");
    assert!(
        rebuilt.as_slice() == section.as_bytes(),
        "embedded section differs from a fresh link ({} vs {} bytes)",
        section.as_bytes().len(),
        rebuilt.len()
    );
    for (l, file) in section.classes().zip(&class_files) {
        assert_eq!(&file[..4], &[0xCA, 0xFE, 0xBA, 0xBE]);
        assert!(l.class.len() < file.len());
    }
}

/// The framework's Utf8 constants are pooled once each: the section is
/// smaller than its class files by most of what they repeat.
#[test]
fn framework_utf8_constants_are_pooled() {
    let section = framework_section();
    let pool = section.pool();
    let (mut entries, mut plain) = (0usize, 0usize);
    for l in section.classes() {
        for i in 0..l.link.cp_count() {
            if let Some(b) = l.utf8(i) {
                entries += 1;
                plain += b.len();
            }
        }
    }
    assert!(
        pool.len() < plain,
        "pool {} B, Utf8 {} B",
        pool.len(),
        plain
    );
    assert!(pool.len() <= class_link::section::MAX_POOL_BYTES / 2);
    eprintln!(
        "framework Utf8: {entries} entries, {plain} B as class files, {} B pooled",
        pool.len()
    );
}

/// The framework's string constants are pooled once each: fewer rows than
/// `String` entries, and few enough to leave the app's pool and the
/// runtime's own strings the rest of a `u16` of references.
#[test]
fn framework_string_constants_are_pooled() {
    let literals = framework_section().literals();
    let entries: usize = framework_classes()
        .iter()
        .map(|cf| cf.link().strs_len())
        .sum();
    assert!(!literals.is_empty());
    assert!(literals.len() < entries, "{} rows", literals.len());
    assert!(literals.len() < class_link::MAX_LITERALS / 4);
    eprintln!(
        "framework literals: {} rows for {} String entries",
        literals.len(),
        entries
    );
}

/// Registration costs the class table and nothing else, on both targets:
/// two pointers and the name hash a class.
#[test]
fn a_registered_class_costs_two_pointers_and_a_hash() {
    let mut jvm = Jvm::with_capacity(FRAMEWORK_CLASS_COUNT);
    jvm.load_framework(framework_section())
        .expect("load framework");
    assert_eq!(jvm.class_count(), FRAMEWORK_CLASS_COUNT);
    let (host, dev) = jvm.class_table_bytes();
    assert_eq!(dev, 12 + FRAMEWORK_CLASS_COUNT * 16);
    assert_eq!(
        host - dev,
        FRAMEWORK_CLASS_COUNT * CLASS_FILE_DELTA + (core::mem::size_of::<Vec<ClassFile>>() - 12)
    );
}

/// The section's index and the JVM's lookup agree on every class, and a
/// name the set does not hold is found by neither.
#[test]
fn every_framework_class_is_found_by_name() {
    let section = framework_section();
    let classes = framework_classes();
    for (i, cf) in classes.iter().enumerate() {
        let name = cf.class_name().unwrap();
        assert_eq!(section.find_class(name), Some(i));
        assert_eq!(find_class(Classes::linear(&classes), name), Some(i));
        assert_eq!(cf.name_hash(), class_link::name_hash(name));
    }
    assert_eq!(section.find_class(b"no/such/Class"), None);
    assert_eq!(
        find_class(Classes::linear(&classes), b"no/such/Class"),
        None
    );
}

/// Every method's signature hash and every interface hash in the tables are
/// the hashes of the names the class bytes spell.
#[test]
fn table_hashes_match_the_names() {
    for cf in framework_classes() {
        for m in cf.methods() {
            let n = cf.method_name(m).unwrap();
            let d = cf.method_descriptor(m).unwrap();
            assert_eq!(m.sig_hash(), class_link::sig_hash(n, d));
        }
        for f in cf.interfaces() {
            assert_eq!(f.hash(), class_link::name_hash(cf.iface_name(f).unwrap()));
        }
        if let Some(sup) = cf.view().super_name() {
            assert_eq!(cf.super_hash(), class_link::name_hash(sup));
        }
    }
}

/// No two methods of one framework class share a signature hash, so the
/// hash compare in the resolution walk names the method on its own and the
/// confirming read is one per hit — for every class of the corpus, the
/// deep `View`/`TextView` hierarchies included.
#[test]
fn signature_hashes_are_unique_within_every_framework_class() {
    for cf in framework_classes() {
        let mut seen = std::collections::HashSet::new();
        for m in cf.methods() {
            assert!(
                seen.insert(m.sig_hash()),
                "{}: two methods hash alike ({} {})",
                String::from_utf8_lossy(cf.class_name().unwrap()),
                String::from_utf8_lossy(cf.method_name(m).unwrap()),
                String::from_utf8_lossy(cf.method_descriptor(m).unwrap())
            );
        }
    }
}

/// How much of the embedded section is class-file framing the link table
/// has made redundant: the `method_info` header and `Code` preamble of every
/// method (8 B + 18 B; the table locates the code and reads `max_stack`,
/// `max_locals` and the name and descriptor refs out of that framing), and
/// the fixed constant-pool entries (tag bytes — the table packs the tags
/// already — and the payload of `Class`, `String`, the three `*ref` kinds
/// and `NameAndType`). The sizing behind the class store's flash half in
/// docs/quality-roadmap.md; printed with `--nocapture`, never a gate.
#[test]
fn framework_class_file_framing() {
    use class_link::classfile::{
        fixed_entry_len, walk_cp, Utf8Form, TAG_CLASS, TAG_FIELDREF, TAG_INTERFACE_METHODREF,
        TAG_METHODREF, TAG_NAME_AND_TYPE, TAG_STRING, TAG_UTF8,
    };
    let section = framework_section();
    let (mut classes, mut class_bytes, mut methods, mut with_code) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut tag_bytes, mut class_string, mut refs, mut name_and_type, mut other) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for l in section.classes() {
        classes += 1;
        class_bytes += l.class.len();
        for m in l.link.methods() {
            methods += 1;
            if m.code_offset != 0 {
                with_code += 1;
            }
        }
        walk_cp(l.class, Utf8Form::Pooled, |_, tag, _| {
            if tag == 0 {
                return Ok(());
            }
            tag_bytes += 1;
            if tag == TAG_UTF8 {
                return Ok(());
            }
            let payload = fixed_entry_len(tag).expect("known tag");
            match tag {
                TAG_CLASS | TAG_STRING => class_string += payload,
                TAG_FIELDREF | TAG_METHODREF | TAG_INTERFACE_METHODREF => refs += payload,
                TAG_NAME_AND_TYPE => name_and_type += payload,
                _ => other += payload,
            }
            Ok(())
        })
        .expect("pooled class walks");
    }
    let method_framing = methods * 8 + with_code * 18;
    let cp_framing = tag_bytes + class_string + refs + name_and_type;
    println!(
        "framing: {} classes, section {} B, class bytes {} B; methods {} ({} with code) \
         method framing {} B; cp framing {} B (tags {}, Class/String {}, refs {}, NameAndType {}; \
         other payload {} B); total removable {} B",
        classes,
        section.as_bytes().len(),
        class_bytes,
        methods,
        with_code,
        method_framing,
        cp_framing,
        tag_bytes,
        class_string,
        refs,
        name_and_type,
        other,
        method_framing + cp_framing
    );
    assert!(method_framing > 0 && cp_framing > 0);
}
