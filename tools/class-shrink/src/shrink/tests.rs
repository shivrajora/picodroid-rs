// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::keep::KeepList;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("cs-shrink-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn utf8(s: &str) -> CpEntry {
    CpEntry::Utf8(s.as_bytes().to_vec())
}

fn class(name_idx: u16) -> CpEntry {
    CpEntry::Other {
        tag: 7,
        payload: name_idx.to_be_bytes().to_vec(),
    }
}

fn string(utf8_idx: u16) -> CpEntry {
    CpEntry::Other {
        tag: 8,
        payload: utf8_idx.to_be_bytes().to_vec(),
    }
}

/// Build a minimal, member-less class file from 1-based constant-pool
/// entries; `this_class` indexes the `CONSTANT_Class` naming the class.
fn build_class(entries: Vec<CpEntry>, this_class: u16) -> Vec<u8> {
    build_class_with(entries, this_class, 0x0021, &[], &[])
}

fn nat(name_idx: u16, desc_idx: u16) -> CpEntry {
    let mut payload = name_idx.to_be_bytes().to_vec();
    payload.extend_from_slice(&desc_idx.to_be_bytes());
    CpEntry::Other { tag: 12, payload }
}

/// [`build_class`] with `(name_idx, desc_idx)` fields and methods; each
/// member gets one dummy 3-byte attribute so the length-skip is exercised.
fn build_class_with(
    entries: Vec<CpEntry>,
    this_class: u16,
    access: u16,
    fields: &[(u16, u16)],
    methods: &[(u16, u16)],
) -> Vec<u8> {
    let mut all = vec![CpEntry::Other {
        tag: 0,
        payload: Vec::new(),
    }];
    all.extend(entries);
    let mut tail = Vec::new();
    tail.extend_from_slice(&access.to_be_bytes());
    tail.extend_from_slice(&this_class.to_be_bytes());
    tail.extend_from_slice(&[0u8; 4]); // super_class, interfaces_count
    for table in [fields, methods] {
        tail.extend_from_slice(&(table.len() as u16).to_be_bytes());
        for &(name, desc) in table {
            tail.extend_from_slice(&0x0001u16.to_be_bytes());
            tail.extend_from_slice(&name.to_be_bytes());
            tail.extend_from_slice(&desc.to_be_bytes());
            tail.extend_from_slice(&1u16.to_be_bytes()); // attributes_count
            tail.extend_from_slice(&name.to_be_bytes()); // attribute_name_index (any Utf8)
            tail.extend_from_slice(&3u32.to_be_bytes());
            tail.extend_from_slice(&[7, 7, 7]);
        }
    }
    tail.extend_from_slice(&[0u8; 2]); // attributes_count
    ClassFile {
        header: b"\xCA\xFE\xBA\xBE\x00\x00\x00\x34".to_vec(),
        entries: all,
        tail,
    }
    .serialize()
}

fn utf8_at(cf: &ClassFile, idx: usize) -> &[u8] {
    match &cf.entries[idx] {
        CpEntry::Utf8(b) => b,
        other => panic!("entry {idx} is not Utf8: {other:?}"),
    }
}

#[test]
fn cut_release_skips_kept() {
    // Without actually generating .class files we can smoke-test the
    // keep check by feeding an empty dir: nothing gets shrunk.
    let dir = tmp("cut-empty");
    let keep = KeepList::default();
    let m = cut_release(&dir, &keep, &[], ShrinkMap::new()).unwrap();
    assert!(m.classes.is_empty());
}

#[test]
fn cut_release_harvests_referenced_java_names_into_b() {
    let dir = tmp("cut-harvest");
    let bytes = build_class(
        vec![
            class(2),                      // #1 this_class
            utf8("foo/Bar"),               // #2
            class(4),                      // #3 super
            utf8("java/lang/Object"),      // #4
            utf8("(Ljava/lang/String;)V"), // #5 own-member descriptor (tail-only ref)
            string(7),                     // #6 ldc literal
            utf8("java/util/List"),        // #7 literal text — must NOT be harvested
            class(9),                      // #8 array class entry
            utf8("[Ljava/lang/Runnable;"), // #9
            utf8("java/net/Socket"),       // #10 unreferenced bare name — not a class
        ],
        1,
    );
    fs::write(dir.join("Bar.class"), bytes).unwrap();
    let m = cut_release(&dir, &KeepList::default(), &[], ShrinkMap::new()).unwrap();
    let got: Vec<(&str, &str)> = m.iter_classes().collect();
    assert_eq!(
        got,
        vec![
            ("foo/Bar", "a/A"),
            ("java/lang/Object", "b/A"),
            ("java/lang/Runnable", "b/B"),
            ("java/lang/String", "b/C"),
        ]
    );
}

#[test]
fn cut_release_continues_each_namespace_counter() {
    let dir = tmp("cut-counters");
    let bytes = build_class(
        vec![
            class(2),                 // #1
            utf8("foo/Bar"),          // #2
            class(4),                 // #3
            utf8("java/lang/String"), // #4
        ],
        1,
    );
    fs::write(dir.join("Bar.class"), bytes).unwrap();
    let mut base = ShrinkMap::new();
    base.classes.insert("x/Y".into(), "a/C".into());
    base.classes.insert("java/lang/Object".into(), "b/B".into());
    let m = cut_release(&dir, &KeepList::default(), &[], base).unwrap();
    assert_eq!(m.classes["x/Y"], "a/C");
    assert_eq!(m.classes["java/lang/Object"], "b/B");
    assert_eq!(m.classes["foo/Bar"], "a/D", "a/ continues after a/C");
    assert_eq!(
        m.classes["java/lang/String"], "b/C",
        "b/ continues after b/B"
    );
}

#[test]
fn extra_names_come_from_plain_lists_and_the_contract_tsv() {
    let dir = tmp("extra-names");
    let file = dir.join("names.tsv");
    fs::write(
        &file,
        "# comment\n\
         java/lang/RuntimeException\n\
         java/util/Iterator\tnext\t()Ljava/lang/Object;\n\
         @extends\tjava/lang/Error\tjava/lang/Throwable\n\
         @nameonly\tjava/lang/CloneNotSupportedException\n\
         @hint\tjava/lang/System\tout\tno stdout; see java/lang/Nope\n\
         @hint\tjava/lang/Thread*\t\tglobs are not names\n",
    )
    .unwrap();
    let mut names = read_extra_names(&file).unwrap();
    names.sort();
    names.dedup();
    assert_eq!(
        names,
        vec![
            "java/lang/CloneNotSupportedException",
            "java/lang/Error",
            "java/lang/Object",
            "java/lang/RuntimeException",
            "java/lang/Throwable",
            "java/util/Iterator",
        ]
    );
    let m = cut_release(&dir, &KeepList::default(), &names, ShrinkMap::new()).unwrap();
    assert_eq!(m.classes["java/lang/RuntimeException"], "b/D");
    assert_eq!(m.classes.len(), 6);
}

#[test]
fn contract_member_names_come_from_column_two() {
    let dir = tmp("contract-members");
    let file = dir.join("api-contract.tsv");
    fs::write(
        &file,
        "# comment\n\
         java/lang/Object\n\
         java/lang/Object\ttoString\t\n\
         java/util/Iterator\tnext\t()Ljava/lang/Object;\n\
         java/util/Iterator\thasNext\t()Z\n\
         @extends\tjava/lang/Error\tjava/lang/Throwable\n\
         @hint\tjava/lang/System\tout\tno stdout\n",
    )
    .unwrap();
    assert_eq!(
        read_contract_member_names(&file).unwrap(),
        vec!["hasNext", "next", "toString"]
    );
}

/// `foo/Widget` declares `setText`, `refresh`, `toString` (contract),
/// `main` (keep), `id` (too short), `<init>`, `lambda$x$0` and a field
/// `count`; `java/lang/Math` declares `abs` (kept owner). Only the
/// framework names get targets, and no target equals a spelled name.
#[test]
fn cut_release_members_maps_every_served_name() {
    let dir = tmp("cut-members");
    let widget = build_class_with(
        vec![
            class(2),                      // #1
            utf8("foo/Widget"),            // #2
            utf8("setText"),               // #3
            utf8("(Ljava/lang/String;)V"), // #4
            utf8("refresh"),               // #5
            utf8("()V"),                   // #6
            utf8("toString"),              // #7  contract-kept
            utf8("()Ljava/lang/String;"),  // #8
            utf8("main"),                  // #9  keep.toml
            utf8("id"),                    // #10 too short
            utf8("<init>"),                // #11
            utf8("lambda$x$0"),            // #12 synthetic
            utf8("count"),                 // #13 field
            utf8("I"),                     // #14
            nat(16, 6),                    // #15 referenced member `a` — reserved
            utf8("a"),                     // #16
        ],
        1,
        0x0021,
        &[(13, 14)],
        &[(3, 4), (5, 6), (7, 8), (9, 6), (10, 6), (11, 6), (12, 6)],
    );
    fs::write(dir.join("Widget.class"), widget).unwrap();
    let math = build_class_with(
        vec![
            class(2),               // #1
            utf8("java/lang/Math"), // #2
            utf8("abs"),            // #3
            utf8("(I)I"),           // #4
        ],
        1,
        0x0021,
        &[],
        &[(3, 4)],
    );
    fs::create_dir_all(dir.join("java/lang")).unwrap();
    fs::write(dir.join("java/lang/Math.class"), math).unwrap();

    let mut keep = KeepList::default();
    keep.members.push("main".into());
    let contract = vec!["toString".to_string()];
    let mut map = ShrinkMap::new();
    cut_release_members(
        &dir,
        &keep,
        &MemberCut {
            reserve_dirs: &[],
            contract_names: &contract,
            version: "0.16.0",
            floor: false,
        },
        &mut map,
    )
    .unwrap();
    let got: Vec<(&str, &str)> = map.iter_members().collect();
    // Sorted candidates: every declared name — the `java/**` owner's
    // `abs`, the field, the synthetic — plus the contract's `toString`;
    // `main` is kept, `id` too short, `<init>` reserved. Target `a` is
    // spelled (referenced) in the corpus, so allocation starts at `b`.
    assert_eq!(
        got,
        vec![
            ("abs", "b"),
            ("count", "c"),
            ("lambda$x$0", "d"),
            ("refresh", "e"),
            ("setText", "f"),
            ("toString", "g"),
        ]
    );
    assert_eq!(map.member_floor.as_deref(), Some("0.16.0"));

    // Append-only resume: a second cut with a new member continues.
    let more = build_class_with(
        vec![class(2), utf8("foo/Other"), utf8("zebra"), utf8("()V")],
        1,
        0x0021,
        &[],
        &[(3, 4)],
    );
    fs::write(dir.join("Other.class"), more).unwrap();
    cut_release_members(
        &dir,
        &keep,
        &MemberCut {
            reserve_dirs: &[],
            contract_names: &contract,
            version: "0.17.0",
            floor: false,
        },
        &mut map,
    )
    .unwrap();
    assert_eq!(map.members["zebra"], "h");
    assert_eq!(map.members["setText"], "f");
    assert_eq!(
        map.member_floor.as_deref(),
        Some("0.16.0"),
        "floor is sticky"
    );
}

#[test]
fn reserve_dirs_block_targets() {
    let dir = tmp("cut-reserve-corpus");
    let shim = tmp("cut-reserve-shim");
    let widget = build_class_with(
        vec![class(2), utf8("foo/Widget"), utf8("refresh"), utf8("()V")],
        1,
        0x0021,
        &[],
        &[(3, 4)],
    );
    fs::write(dir.join("Widget.class"), widget).unwrap();
    let shim_cls = build_class_with(
        vec![class(2), utf8("kotlin/Unit"), utf8("a"), utf8("()V")],
        1,
        0x0021,
        &[],
        &[(3, 4)],
    );
    fs::write(shim.join("Unit.class"), shim_cls).unwrap();
    let mut map = ShrinkMap::new();
    cut_release_members(
        &dir,
        &KeepList::default(),
        &MemberCut {
            reserve_dirs: &[shim],
            contract_names: &[],
            version: "0.16.0",
            floor: false,
        },
        &mut map,
    )
    .unwrap();
    assert_eq!(map.members["refresh"], "b", "`a` is spelled by the shim");
}

#[test]
fn shrink_directory_leaves_string_literals_alone() {
    let in_dir = tmp("shrink-lit-in");
    let out_dir = tmp("shrink-lit-out");
    let bytes = build_class(
        vec![
            class(2),                 // #1 this_class
            utf8("foo/Bar"),          // #2
            string(4),                // #3 literal only
            utf8("java/util/List"),   // #4 — mapped, but only an ldc literal
            class(6),                 // #5 super
            utf8("java/lang/Object"), // #6 — shared by a Class and a String (javac dedup)
            string(6),                // #7
        ],
        1,
    );
    fs::write(in_dir.join("Bar.class"), bytes).unwrap();
    let mut map = ShrinkMap::new();
    map.classes.insert("foo/Bar".into(), "a/A".into());
    map.classes.insert("java/util/List".into(), "b/A".into());
    map.classes.insert("java/lang/Object".into(), "b/B".into());
    assert_eq!(shrink_directory(&in_dir, &out_dir, &map).unwrap(), 1);

    let out = fs::read(out_dir.join("a/A.class")).expect("written under shrunk name");
    let cf = ClassFile::parse(&out).unwrap();
    assert_eq!(utf8_at(&cf, 2), b"a/A");
    assert_eq!(
        utf8_at(&cf, 4),
        b"java/util/List",
        "literal-only slot untouched"
    );
    assert_eq!(
        utf8_at(&cf, 6),
        b"b/B",
        "shared slot follows the class reference"
    );
}

#[test]
fn shrink_directory_rewrites_a_default_package_this_class() {
    let in_dir = tmp("shrink-dflt-in");
    let out_dir = tmp("shrink-dflt-out");
    let bytes = build_class(
        vec![
            class(2),       // #1 this_class
            utf8("Main"),   // #2 no `/` — classify alone says Other
            utf8("LMain;"), // #3 descriptor naming it
        ],
        1,
    );
    fs::write(in_dir.join("Main.class"), bytes).unwrap();
    let mut map = ShrinkMap::new();
    map.classes.insert("Main".into(), "c/A".into());
    shrink_directory(&in_dir, &out_dir, &map).unwrap();
    let out = fs::read(out_dir.join("c/A.class")).expect("written under c/A");
    let cf = ClassFile::parse(&out).unwrap();
    assert_eq!(utf8_at(&cf, 2), b"c/A");
    assert_eq!(utf8_at(&cf, 3), b"Lc/A;");
}

/// One simple class-file with the given name and `(name, desc)` methods.
fn simple_class(name: &str, methods: &[(&str, &str)]) -> Vec<u8> {
    let mut entries = vec![class(2), utf8(name)];
    let mut idx: Vec<(u16, u16)> = Vec::new();
    for (m, d) in methods {
        entries.push(utf8(m));
        entries.push(utf8(d));
        let n = entries.len() as u16;
        idx.push((n - 1, n));
    }
    build_class_with(entries, 1, 0x0021, &[], &idx)
}

fn write(dir: &Path, name: &str, bytes: Vec<u8>) {
    let path = dir.join(format!("{name}.class"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn app_keep() -> KeepList {
    let mut keep = KeepList::default();
    keep.members.push("main".into());
    keep.members.push("injectMembers".into());
    keep.globs.push("kotlin/**".into());
    keep
}

fn no_reserve() -> AppCut<'static> {
    AppCut {
        reserve_dirs: &[],
        reserve_names: &[],
    }
}

#[test]
fn cut_app_allocates_c_names_and_follows_components_for_injectors() {
    let dir = tmp("cut-app");
    write(
        &dir,
        "app/Main",
        simple_class(
            "app/Main",
            &[("formatLux", "()V"), ("invoke", "()V"), ("main", "()V")],
        ),
    );
    write(
        &dir,
        "app/Main_MembersInjector",
        simple_class(
            "app/Main_MembersInjector",
            &[("injectMembers", "(Lapp/Main;)V")],
        ),
    );
    write(&dir, "app/Outer$In", simple_class("app/Outer$In", &[]));
    write(
        &dir,
        "app/Outer_In_MembersInjector",
        simple_class("app/Outer_In_MembersInjector", &[]),
    );
    // The shim: kept as a class, and `invoke` — which it spells — must
    // not become an app candidate even though app/Main declares it.
    write(
        &dir,
        "kotlin/Unit",
        simple_class("kotlin/Unit", &[("invoke", "()V")]),
    );
    let mut base = ShrinkMap::new();
    base.classes
        .insert("picodroid/app/Activity".into(), "a/A".into());
    base.members.insert("setText".into(), "f".into());
    let map = cut_app(&dir, &app_keep(), base, &no_reserve()).unwrap();

    let classes: Vec<(&str, &str)> = map.iter_classes().collect();
    assert_eq!(
        classes,
        vec![
            ("app/Main", "c/A"),
            ("app/Main_MembersInjector", "c/A_MembersInjector"),
            ("app/Outer$In", "c/C"),
            ("app/Outer_In_MembersInjector", "c/C_MembersInjector"),
            ("picodroid/app/Activity", "a/A"),
        ]
    );
    let members: Vec<(&str, &str)> = map.iter_members().collect();
    // Counter resumes after the base's `f`; `main`/`injectMembers` kept,
    // `invoke` is shim-spelled.
    assert_eq!(members, vec![("formatLux", "g"), ("setText", "f")]);
}

#[test]
fn cut_app_rejects_default_package_and_synthetic_prefixes() {
    let dir = tmp("cut-app-dflt");
    write(&dir, "Main", simple_class("Main", &[]));
    let err = cut_app(&dir, &app_keep(), ShrinkMap::new(), &no_reserve()).unwrap_err();
    assert!(err.to_string().contains("default package"), "{err}");

    let dir = tmp("cut-app-synth");
    write(&dir, "c/Foo", simple_class("c/Foo", &[]));
    let err = cut_app(&dir, &app_keep(), ShrinkMap::new(), &no_reserve()).unwrap_err();
    assert!(err.to_string().contains("synthetic"), "{err}");
}

#[test]
fn cut_app_member_targets_skip_reserved_names_and_short_or_mapped_candidates() {
    let dir = tmp("cut-app-members");
    write(
        &dir,
        "app/Main",
        simple_class(
            "app/Main",
            &[
                ("setText", "()V"), // base-mapped override: not a candidate
                ("refresh", "()V"), // candidate
                // Declared by the SDK since the base map was cut: the
                // framework calls it by this spelling, so it must keep it.
                ("onSaveInstanceState", "()V"),
                ("id", "()V"), // too short
                ("<init>", "()V"),
            ],
        ),
    );
    let mut base = ShrinkMap::new();
    base.members.insert("setText".into(), "f".into());
    let reserve = vec![
        "g".to_string(),
        "PI".to_string(),
        "onSaveInstanceState".to_string(),
    ];
    let opts = AppCut {
        reserve_dirs: &[],
        reserve_names: &reserve,
    };
    let map = cut_app(&dir, &app_keep(), base, &opts).unwrap();
    let members: Vec<(&str, &str)> = map.iter_members().collect();
    assert_eq!(
        members,
        vec![("refresh", "h"), ("setText", "f")],
        "`g` is reserved by list, so refresh takes `h`"
    );
}

#[test]
fn cut_app_refuses_an_app_that_spells_a_release_target() {
    let dir = tmp("cut-app-clash");
    write(&dir, "app/Main", simple_class("app/Main", &[("f", "()V")]));
    let mut base = ShrinkMap::new();
    base.members.insert("setText".into(), "f".into());
    let err = cut_app(&dir, &app_keep(), base, &no_reserve()).unwrap_err();
    assert!(
        err.to_string().contains("targets of the release map"),
        "{err}"
    );
}
