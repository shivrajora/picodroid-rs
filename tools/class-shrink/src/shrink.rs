// SPDX-License-Identifier: GPL-3.0-only
//! Top-level driver: apply a [`ShrinkMap`] to a directory of `.class` files.
//!
//! This tool rewrites class names (and class-name substrings inside
//! descriptors). The parser is lossless in byte order outside the constant
//! pool, so rewriting Utf8 entries alone keeps every class file valid.
//! Member names are *allocated* here ([`cut_release_members`]) but rewritten
//! by the Gradle-side ASM pass (`ShrinkMembersTask`), which rebuilds the
//! constant pool and so never has to split a Utf8 shared between a member
//! name and a string literal.
//!
//! Utf8 entries reached only through a `CONSTANT_String` are `ldc` string
//! literals and are never rewritten, even when their text equals a mapped
//! class name — a Java string `"java/lang/Object"` is user data.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::classfile::{ClassFile, CpEntry};
use crate::descriptor::{class_refs, classify, rewrite_bare, rewrite_descriptor, RewriteKind};
use crate::mapping::ShrinkMap;
use crate::rename::{
    base26_inverse, member_inverse, member_suffix, namespace_for, short_suffix, shrunk_name,
    Namespace,
};
use std::collections::BTreeSet;

/// Recursively list every `.class` file under `root`, returning absolute
/// paths sorted lexicographically (determinism).
pub fn list_class_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "class") {
            out.push(path);
        }
    }
    Ok(())
}

/// Read a class's own internal name from its constant pool.
/// JVMS: `this_class` is a u2 at offset 2 into the class body after CP;
/// the entry it points at is `CONSTANT_Class_info` whose `name_index`
/// points to the Utf8 we want.
///
/// We already parsed the CP; the tail starts with `access_flags u2,
/// this_class u2`, so we read the second u16 of the tail to get the CP
/// index into a Class_info, then fetch that Class_info's name_index,
/// then the Utf8.
pub fn read_own_name(cf: &ClassFile) -> Option<&[u8]> {
    if cf.tail.len() < 4 {
        return None;
    }
    let this_class_idx = u16::from_be_bytes([cf.tail[2], cf.tail[3]]) as usize;
    let CpEntry::Other { tag: 7, payload } = cf.entries.get(this_class_idx)? else {
        return None;
    };
    let name_idx = u16::from_be_bytes([*payload.first()?, *payload.get(1)?]) as usize;
    match cf.entries.get(name_idx)? {
        CpEntry::Utf8(b) => Some(b),
        _ => None,
    }
}

/// Apply `map` to every class file under `in_dir`, writing the result under
/// `out_dir` mirroring the original directory structure but with the
/// shrunk class path. Returns the number of classes written.
pub fn shrink_directory(in_dir: &Path, out_dir: &Path, map: &ShrinkMap) -> io::Result<usize> {
    // Build a lookup keyed by the byte form (matches what classfile.rs sees).
    let byte_map: HashMap<Vec<u8>, Vec<u8>> = map
        .iter_classes()
        .map(|(a, b)| (a.as_bytes().to_vec(), b.as_bytes().to_vec()))
        .collect();

    fs::create_dir_all(out_dir)?;
    let files = list_class_files(in_dir)?;
    for file in &files {
        let bytes = fs::read(file)?;
        let mut cf = ClassFile::parse(&bytes)?;
        let refs = cf.utf8_refs();
        for (i, entry) in cf.entries.iter_mut().enumerate() {
            let CpEntry::Utf8(utf) = entry else {
                continue;
            };
            // A Utf8 reached only through CONSTANT_String is an `ldc` literal:
            // user data that merely looks like a class name. javac dedupes
            // Utf8s, so a slot that is also a class name or a descriptor must
            // still be rewritten — the class reference wins over the literal.
            if refs.strings.contains(&i)
                && !refs.class_names.contains(&i)
                && !refs.descriptors.contains(&i)
            {
                continue;
            }
            let payload = utf.clone();
            match classify(&payload) {
                RewriteKind::BareName => {
                    if let Some(new) = rewrite_bare(&payload, &byte_map) {
                        *utf = new;
                    }
                }
                RewriteKind::Descriptor => {
                    let new = rewrite_descriptor(&payload, &byte_map);
                    if new != payload {
                        *utf = new;
                    }
                }
                RewriteKind::Other => {}
            }
        }
        // Place the rewritten file at its new internal name (so the file tree
        // mirrors the class tree). Fall back to the original name if this
        // class wasn't renamed.
        let own_name = read_own_name(&cf)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_else(|| {
                file.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let out_file = out_dir.join(format!("{own_name}.class"));
        if let Some(parent) = out_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_file, cf.serialize())?;
    }
    Ok(files.len())
}

/// `java/**` names this class refers to without defining: the bare names
/// behind its `CONSTANT_Class` entries and the `L…;` object types in every
/// descriptor-shaped Utf8 (member references, own members, signatures).
/// These have no class file in the framework — pico-jvm serves them
/// natively — so they only enter the map through the classes that use them.
/// String literals are skipped for the same reason `shrink_directory`
/// leaves them alone.
fn referenced_java_names(cf: &ClassFile) -> Vec<String> {
    let refs = cf.utf8_refs();
    let mut out = Vec::new();
    for (i, entry) in cf.entries.iter().enumerate() {
        let CpEntry::Utf8(bytes) = entry else {
            continue;
        };
        let is_class = refs.class_names.contains(&i);
        let string_only = refs.strings.contains(&i) && !is_class && !refs.descriptors.contains(&i);
        if string_only {
            continue;
        }
        let mut push = |name: &[u8]| {
            if let Ok(s) = std::str::from_utf8(name) {
                if namespace_for(s) == Namespace::Java {
                    out.push(s.to_string());
                }
            }
        };
        match classify(bytes) {
            RewriteKind::BareName if is_class => push(bytes),
            // Array-form class entries (`[Ljava/lang/String;`) land here too.
            RewriteKind::Descriptor => {
                for name in class_refs(bytes) {
                    push(name);
                }
            }
            _ => {}
        }
    }
    out
}

/// Read additional original class names to allocate from a text file: one
/// name per line, `#` comments ignored. Tab-separated lines are scanned
/// field by field (bare internal names are taken as-is, descriptors
/// contribute their `L…;` classes), so `sdk/api-contract.tsv` — the
/// committed list of every `java/**` class pico-jvm serves — can be passed
/// directly. That is how `java/**` names the framework never references
/// itself (`java/lang/RuntimeException`, `java/util/Iterator`, …) still get
/// a `b/` entry for the apps that do.
pub fn read_extra_names(path: &Path) -> io::Result<Vec<String>> {
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("@hint") {
            continue;
        }
        for field in line.split('\t').take(3) {
            let bytes = field.as_bytes();
            match classify(bytes) {
                RewriteKind::BareName if is_internal_name(bytes) => out.push(field.to_string()),
                RewriteKind::Descriptor => out.extend(
                    class_refs(bytes)
                        .into_iter()
                        .filter_map(|n| std::str::from_utf8(n).ok())
                        .map(String::from),
                ),
                _ => {}
            }
        }
    }
    Ok(out)
}

fn is_internal_name(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'/'))
}

/// Member names of every `java/**`/`javax/**` member pico-jvm serves — the
/// `name` column of `sdk/api-contract.tsv`'s member rows. These are the
/// names the interpreter and `BuiltinHandler` match by literal on *any*
/// receiver (`toString`, `equals`, `run`, `compare`, `hasNext`, …), so they
/// are never mapped: an app override renamed away from them would silently
/// stop being found.
pub fn read_contract_member_names(path: &Path) -> io::Result<Vec<String>> {
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }
        let mut fields = line.split('\t');
        let _owner = fields.next();
        if let Some(name) = fields.next() {
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Inputs to [`cut_release_members`] beyond the corpus and keep list.
pub struct MemberCut<'a> {
    /// Class trees whose member names must never be handed out as targets
    /// (the kotlin-shim: it rides inside Kotlin PAPKs and is rewritten with
    /// the same map, so a target colliding with one of its names would be
    /// ambiguous).
    pub reserve_dirs: &'a [PathBuf],
    /// Names from [`read_contract_member_names`]: kept and reserved.
    pub contract_names: &'a [String],
    /// The release being cut; becomes `member-floor` on the first cut that
    /// maps a member.
    pub version: &'a str,
}

/// `ACC_ANNOTATION` (JVMS §4.1): annotation interfaces' members are looked
/// up by name from annotation payloads, which no remapper rewrites.
const ACC_ANNOTATION: u16 = 0x2000;

/// Candidate member names of one class tree, and every member name it
/// spells (declared or referenced) — the latter is the reserve set.
struct MemberCensus {
    candidates: BTreeSet<String>,
    spelled: BTreeSet<String>,
}

fn member_census(dir: &Path, collect_candidates: bool) -> io::Result<MemberCensus> {
    let mut census = MemberCensus {
        candidates: BTreeSet::new(),
        spelled: BTreeSet::new(),
    };
    for file in list_class_files(dir)? {
        let bytes = fs::read(&file)?;
        let cf = ClassFile::parse(&bytes)?;
        let members = cf.members()?;
        let own = read_own_name(&cf)
            .and_then(|n| std::str::from_utf8(n).ok())
            .unwrap_or("")
            .to_string();
        // Members of classes pico-jvm serves by original name (`java/**`) and
        // of the kotlin-shim stay verbatim; so do annotation members.
        let shrinkable_owner = collect_candidates
            && namespace_for(&own) == Namespace::Framework
            && !own.starts_with("kotlin/")
            && members.class_access & ACC_ANNOTATION == 0;
        for m in members.fields.iter().chain(members.methods.iter()) {
            let Ok(name) = std::str::from_utf8(&m.name) else {
                continue;
            };
            census.spelled.insert(name.to_string());
            if shrinkable_owner && is_member_candidate(name) {
                census.candidates.insert(name.to_string());
            }
        }
        for name in cf.referenced_member_names() {
            if let Ok(s) = std::str::from_utf8(name) {
                census.spelled.insert(s.to_string());
            }
        }
    }
    Ok(census)
}

/// `<init>`/`<clinit>` are JVMS-reserved; `$` marks javac synthetics
/// (`$VALUES`, `lambda$…`, `access$…`) — class-internal, but not worth a
/// const-identifier rule; names of ≤ 2 characters gain nothing.
fn is_member_candidate(name: &str) -> bool {
    !name.starts_with('<') && !name.contains('$') && name.len() > 2
}

/// Extend `map.members` with a target for every candidate member name in
/// `in_dir` (append-only: existing entries are kept verbatim and the
/// allocator resumes past the highest target already handed out).
///
/// Candidates: names declared by a framework class (`a/` namespace, not an
/// annotation), minus `<init>`-style names, `$` synthetics, names of ≤ 2
/// chars, `[[member]]` keeps and every contract name. Targets skip every
/// name spelled anywhere in the corpus, the reserve trees, the contract, the
/// keep list, or the map — so a target can never alias a name that stays.
pub fn cut_release_members(
    in_dir: &Path,
    keep: &crate::keep::KeepList,
    opts: &MemberCut<'_>,
    map: &mut ShrinkMap,
) -> io::Result<()> {
    let corpus = member_census(in_dir, true)?;
    let mut reserved: BTreeSet<String> = corpus.spelled;
    for dir in opts.reserve_dirs {
        reserved.extend(member_census(dir, false)?.spelled);
    }
    reserved.extend(opts.contract_names.iter().cloned());
    reserved.extend(keep.members.iter().cloned());
    reserved.extend(map.members.keys().cloned());
    reserved.extend(map.members.values().cloned());

    let contract: BTreeSet<&str> = opts.contract_names.iter().map(String::as_str).collect();
    let mut next = map
        .members
        .values()
        .filter_map(|t| member_inverse(t))
        .map(|raw| raw + 1)
        .max()
        .unwrap_or(0);
    let mut added = 0usize;
    for name in corpus.candidates {
        if keep.is_member_kept(&name) || contract.contains(name.as_str()) {
            continue;
        }
        if map.members.contains_key(&name) {
            continue;
        }
        let target = loop {
            let t = member_suffix(&mut next);
            if !reserved.contains(&t) {
                break t;
            }
        };
        map.members.insert(name, target);
        added += 1;
    }
    if map.has_members() && map.member_floor.is_none() {
        map.member_floor = Some(opts.version.to_string());
    }
    if let Err(e) = map.verify_injective() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, e));
    }
    eprintln!("members: {added} new targets allocated");
    Ok(())
}

/// Walk `in_dir`'s class files, collect every class that's NOT kept by
/// `keep` — the classes defined there, the `java/**` names they refer to,
/// and `extra_names` (see [`read_extra_names`]) — sort deterministically,
/// and extend `base` with freshly allocated shrunk names (append-only).
/// Each [`Namespace`] continues its own counter from where `base` left off.
/// Returns the updated map.
pub fn cut_release(
    in_dir: &Path,
    keep: &crate::keep::KeepList,
    extra_names: &[String],
    base: ShrinkMap,
) -> io::Result<ShrinkMap> {
    let files = list_class_files(in_dir)?;
    let mut discovered: Vec<String> = extra_names.to_vec();
    for file in &files {
        let bytes = fs::read(file)?;
        let cf = ClassFile::parse(&bytes)?;
        if let Some(name) = read_own_name(&cf) {
            if let Ok(s) = std::str::from_utf8(name) {
                discovered.push(s.to_string());
            }
        }
        discovered.extend(referenced_java_names(&cf));
    }
    discovered.sort();
    discovered.dedup();

    let mut map = base;
    // Next free raw allocator index per namespace: one past the highest raw
    // index already consumed by an existing map entry under that prefix.
    // Derived by inverting each entry's shrunk suffix back to its raw index
    // (rather than assuming it equals the entry count) because a
    // reserved-keyword skip consumes a raw index without producing a map
    // entry — the count-based shortcut silently undercounts once any past
    // cut has crossed one. Threaded by mutable reference through
    // short_suffix so a skip advances the shared counter instead of
    // desyncing from a per-call copy.
    let mut next = [0usize; Namespace::ALL.len()];
    for (slot, ns) in Namespace::ALL.iter().enumerate() {
        next[slot] = map
            .classes
            .values()
            .filter_map(|shrunk| base26_inverse(shrunk.strip_prefix(ns.prefix())?))
            .map(|raw| raw + 1)
            .max()
            .unwrap_or(0);
    }
    for name in discovered {
        if keep.is_kept(&name) {
            continue;
        }
        if map.classes.contains_key(&name) {
            continue;
        }
        let ns = namespace_for(&name);
        let slot = Namespace::ALL
            .iter()
            .position(|n| *n == ns)
            .expect("every namespace is listed in Namespace::ALL");
        let suffix = short_suffix(&mut next[slot]);
        map.classes.insert(name, shrunk_name(ns, &suffix));
    }
    // Never emit a map that isn't a 1:1 mapping — a duplicate shrunk name
    // would silently corrupt any build using either colliding class.
    if let Err(e) = map.verify_injective() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, e));
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
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
    fn cut_release_members_allocates_only_framework_candidates() {
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
            },
            &mut map,
        )
        .unwrap();
        let got: Vec<(&str, &str)> = map.iter_members().collect();
        // Sorted candidates: count, refresh, setText. Target `a` is spelled
        // (referenced) in the corpus, so allocation starts at `b`.
        assert_eq!(
            got,
            vec![("count", "b"), ("refresh", "c"), ("setText", "d")]
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
            },
            &mut map,
        )
        .unwrap();
        assert_eq!(map.members["zebra"], "e");
        assert_eq!(map.members["setText"], "d");
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
}
