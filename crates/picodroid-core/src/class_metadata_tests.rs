// SPDX-License-Identifier: GPL-3.0-only
//! The parsed-metadata model against the embedded framework class set.
//!
//! M8 (docs/parity-audit.md, "2026-09-24 memory-model divergence"): a
//! class's parsed metadata is one `u16` record, byte-identical on the
//! 64-bit host and the 32-bit device, plus a header that differs by the
//! record's fat pointer and nothing else. The simulator's arena therefore
//! prices class metadata as the board does. This pins that over every
//! framework class rather than a fixture: a field that grows to `usize`, a
//! `Vec` that creeps back in, or a device model that drifts from `size_of`
//! all show up here as a host/device gap wider than one pointer per class.

use pico_jvm::class_file::{ClassFile, FAT_PTR_DELTA};
use pico_jvm::Jvm;

fn framework_classes() -> Vec<ClassFile> {
    let classes: Vec<ClassFile> = crate::framework_classes::FRAMEWORK_CLASSES
        .iter()
        .map(|b| ClassFile::parse(b).expect("parse framework class"))
        .collect();
    assert!(
        !classes.is_empty(),
        "FRAMEWORK_CLASSES is empty — run via scripts/test.sh, which sets PICODROID_APK_PATH"
    );
    classes
}

/// Host and device differ by exactly one fat pointer per parsed class, and
/// the device figure is within 2 % of the host's over the whole set.
#[test]
fn parsed_metadata_is_pointer_width_independent() {
    let classes = framework_classes();
    let (mut host, mut dev) = (0usize, 0usize);
    for cf in &classes {
        let c = cf.parsed_metadata_census().expect("eagerly parsed");
        assert_eq!(
            c.host.boxed - c.dev.boxed,
            FAT_PTR_DELTA,
            "{}: header differs by more than the fat pointer",
            String::from_utf8_lossy(cf.class_name().unwrap())
        );
        let (h, d) = cf.parsed_metadata_bytes().unwrap();
        assert_eq!(h - c.host.boxed, d - c.dev.boxed, "record bytes differ");
        host += h;
        dev += d;
    }
    assert_eq!(host - dev, classes.len() * FAT_PTR_DELTA);
    // The exact pin above is the test; this is the audit's headline figure
    // (a record averages ~400 B, so one pointer is about 2 % of it).
    assert!(
        dev * 100 >= host * 97,
        "device model {dev} B is more than 3 % under the host's {host} B"
    );
}

/// The `Jvm` aggregates are the per-class sums, and forcing every method
/// table through `load_class` prices the same as the eager parse.
#[test]
fn jvm_aggregates_match_per_class_sums() {
    let classes = framework_classes();
    let expected: (usize, usize) = classes
        .iter()
        .map(|cf| cf.parsed_metadata_bytes().unwrap())
        .fold((0, 0), |(h, d), (a, b)| (h + a, d + b));

    let mut jvm = Jvm::with_capacity(classes.len());
    for b in crate::framework_classes::FRAMEWORK_CLASSES {
        jvm.load_class(b).expect("register framework class");
    }
    assert_eq!(
        jvm.parsed_metadata_bytes(),
        (0, 0),
        "lazy: nothing parsed yet"
    );
    for cf in jvm.classes() {
        let _ = cf.methods();
    }
    assert_eq!(jvm.parsed_metadata_bytes(), expected);
    let (parsed, total) = jvm.count_parsed();
    assert_eq!((parsed, total), (classes.len(), classes.len()));
    let (table_host, table_dev) = jvm.class_table_bytes();
    assert!(
        table_host >= table_dev,
        "the class table is never cheaper here"
    );
}
