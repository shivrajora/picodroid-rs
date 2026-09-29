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

/// Every table re-derives from its class bytes, and re-linking the bytes on
/// the host gives the table word for word: the embedded section is what
/// the builder produces, not a stale or hand-edited blob.
#[test]
fn framework_tables_relink_word_for_word() {
    let section = framework_section();
    section
        .validate()
        .expect("framework class section validates");
    for cf in framework_classes() {
        let rebuilt = class_link::build::link_class(cf.data()).expect("relink");
        assert_eq!(
            rebuilt.as_slice(),
            cf.link().words(),
            "{}: embedded table differs from a fresh link",
            String::from_utf8_lossy(cf.class_name().unwrap())
        );
    }
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
