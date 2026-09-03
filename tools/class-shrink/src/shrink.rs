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
            // `classify` needs a `/` to call a Utf8 a bare class name; a
            // default-package class (`Main`) has none, but its
            // `CONSTANT_Class` reference settles it — without this its own
            // `this_class` would stay while `LMain;` in descriptors moves.
            let kind = match classify(&payload) {
                RewriteKind::Other if refs.class_names.contains(&i) => RewriteKind::BareName,
                kind => kind,
            };
            match kind {
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
/// `name` column of `sdk/api-contract.tsv`'s member rows. The runtime
/// matches these on *any* receiver (`toString`, `equals`, `run`, `compare`,
/// `hasNext`, …) through the generated `m::` consts, so they are mapped like
/// every other member: an app override is renamed in lockstep by the same
/// by-name map, and the JVM's arms compile to the same target.
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

/// Every identifier-shaped tab-separated field of a text file (`#` and `@`
/// lines skipped) — the member column of `sdk/api-contract.tsv` and both
/// columns of `sdk/member-names.tsv` read this way, so [`cut_app`] can
/// reserve every name the SDK declares or serves without caring which file
/// it came from. Owners (`java/lang/Object`) and descriptors are not
/// identifiers and drop out.
pub fn read_member_name_list(path: &Path) -> io::Result<Vec<String>> {
    let text = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }
        for field in line.split('\t') {
            let ident = !field.is_empty()
                && field
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$'));
            if ident {
                out.push(field.to_string());
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
    /// Names from [`read_contract_member_names`]: mapped (every one the
    /// runtime serves gets a target even when no framework class declares
    /// it) and reserved.
    pub contract_names: &'a [String],
    /// The release being cut; becomes `member-floor` on the first cut that
    /// maps a member, or whenever `floor` asks for it.
    pub version: &'a str,
    /// Re-base `member-floor` on `version`: this cut renames names an older
    /// map spelled verbatim, so PAPKs shrunk before it must be rejected
    /// (`compat::MEMBER_SHRINK_FLOOR`).
    pub floor: bool,
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
        // Members of the kotlin-shim stay verbatim (it is reserved, not
        // mapped); so do annotation members. Everything else the SDK
        // declares — `picodroid/**`, `javax/**` and the `java/**` classes
        // it ships bodies for — is a candidate.
        let shrinkable_owner = collect_candidates
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

/// `<init>`/`<clinit>` are JVMS-reserved; names of ≤ 2 characters gain
/// nothing. javac synthetics (`$VALUES`, `lambda$…`, `access$…`) are
/// mapped like any other name — declaration and every reference are
/// rewritten together by the ASM remapper, and no Rust code matches them.
fn is_member_candidate(name: &str) -> bool {
    !name.starts_with('<') && name.len() > 2
}

/// Extend `map.members` with a target for every candidate member name in
/// `in_dir` (append-only: existing entries are kept verbatim and the
/// allocator resumes past the highest target already handed out).
///
/// Candidates: names declared by an SDK class (not the kotlin-shim, not an
/// annotation) plus every contract name, minus `<init>`-style names, names
/// of ≤ 2 chars and `[[member]]` keeps. Targets skip every name spelled
/// anywhere in the corpus, the reserve trees, the contract, the keep list,
/// or the map — so a target can never alias a name that stays.
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

    let mut candidates = corpus.candidates;
    candidates.extend(
        opts.contract_names
            .iter()
            .filter(|n| is_member_candidate(n))
            .cloned(),
    );
    let mut next = map
        .members
        .values()
        .filter_map(|t| member_inverse(t))
        .map(|raw| raw + 1)
        .max()
        .unwrap_or(0);
    let mut added = 0usize;
    for name in candidates {
        if keep.is_member_kept(&name) {
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
    if map.has_members() && (map.member_floor.is_none() || opts.floor) {
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

/// Inputs to [`cut_app`] beyond the app tree, the keep list and the base map.
pub struct AppCut<'a> {
    /// Class trees whose member names are reserved (never a target) without
    /// being part of the app — the kotlin-shim when it is staged elsewhere.
    pub reserve_dirs: &'a [PathBuf],
    /// Member names reserved by list: every name the SDK declares
    /// (`sdk/member-names.tsv`) and every contract member
    /// (`sdk/api-contract.tsv`). The release cut reserved the SDK corpus's
    /// spelled names but the map does not persist that set, and the SDK's
    /// ≤ 2-char names (`of`, `id`, `E`, `PI`) stay unmapped — an app
    /// subclass must never have a private member renamed onto one of them.
    pub reserve_names: &'a [String],
}

/// Generated by the `@Inject` annotation processor next to each component
/// (`inject/compiler/.../Names.java`) and *derived* at run time by
/// `picodroid-core/src/lifecycle.rs` from the component's runtime class
/// name (`$` → `_`, then this suffix). The two must agree, so a shrunk
/// injector is spelled from its component's shrunk name.
pub const MEMBERS_INJECTOR_SUFFIX: &str = "_MembersInjector";

fn invalid(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

/// Extend `base` (the active release map) with the app's own classes under
/// `c/` and its private member names, ProGuard-style, returning the merged
/// map. Every downstream consumer (`shrink-dir`, the Gradle
/// `ShrinkMembersTask`, `papk-pack --shrink-map`, `retrace`) reads the
/// result exactly as it reads a release map.
///
/// Classes: every class defined under `app_dir` that the keep list does not
/// keep (`kotlin/**` rides inside Kotlin PAPKs and stays verbatim). A class
/// named `<flat(X)>_MembersInjector` is renamed to `<shrunk(X)>_MembersInjector`
/// so the runtime derivation keeps resolving; an injector whose component
/// is kept is kept too. Default-package classes and names under a synthetic
/// prefix are rejected.
///
/// Members: names declared by an app class, longer than two characters,
/// not `<init>`-style, not in the base map (SDK overrides such as `onCreate`
/// already rename in lockstep), not kept, and not spelled by a kept class.
/// Targets resume the base map's counter and skip everything the app tree,
/// the reserve trees, `reserve_names`, the keep list and the base map spell.
pub fn cut_app(
    app_dir: &Path,
    keep: &crate::keep::KeepList,
    base: ShrinkMap,
    opts: &AppCut<'_>,
) -> io::Result<ShrinkMap> {
    let mut defined: Vec<String> = Vec::new();
    let mut all_names: Vec<String> = Vec::new();
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    // Every member name the tree spells (declared or referenced) — never a
    // target — and the subset spelled by kept classes, never a candidate.
    let mut spelled: BTreeSet<String> = BTreeSet::new();
    let mut kept_spelled: BTreeSet<String> = BTreeSet::new();
    for file in list_class_files(app_dir)? {
        let bytes = fs::read(&file)?;
        let cf = ClassFile::parse(&bytes)?;
        let members = cf.members()?;
        let own = read_own_name(&cf)
            .and_then(|n| std::str::from_utf8(n).ok())
            .map(str::to_string)
            .ok_or_else(|| invalid(format!("{}: cannot read this_class", file.display())))?;
        let kept = keep.is_kept(&own);
        if !kept {
            if !own.contains('/') {
                return Err(invalid(format!(
                    "app class `{own}` is in the default package; give it a package \
                     (the shrinker needs a `/` to tell a class name from a member name)"
                )));
            }
            if crate::rename::is_synthetic_name(&own) {
                return Err(invalid(format!(
                    "app class `{own}` sits under a synthetic shrink prefix (a/, b/, c/); \
                     rename the package"
                )));
            }
            if base.classes.contains_key(&own) {
                return Err(invalid(format!(
                    "app class `{own}` has the same name as a framework class in the base map"
                )));
            }
            defined.push(own.clone());
        }
        all_names.push(own);
        let shrinkable_owner = !kept && members.class_access & ACC_ANNOTATION == 0;
        for m in members.fields.iter().chain(members.methods.iter()) {
            let Ok(name) = std::str::from_utf8(&m.name) else {
                continue;
            };
            spelled.insert(name.to_string());
            if kept {
                kept_spelled.insert(name.to_string());
            } else if shrinkable_owner && is_member_candidate(name) {
                candidates.insert(name.to_string());
            }
        }
        for name in cf.referenced_member_names() {
            if let Ok(s) = std::str::from_utf8(name) {
                spelled.insert(s.to_string());
                if kept {
                    kept_spelled.insert(s.to_string());
                }
            }
        }
    }
    defined.sort();
    defined.dedup();

    let mut map = base;

    // ── Classes ─────────────────────────────────────────────────────────
    let mut next = map
        .classes
        .values()
        .filter_map(|shrunk| base26_inverse(shrunk.strip_prefix(Namespace::App.prefix())?))
        .map(|raw| raw + 1)
        .max()
        .unwrap_or(0);
    for name in &defined {
        let suffix = short_suffix(&mut next);
        map.classes
            .insert(name.clone(), shrunk_name(Namespace::App, &suffix));
    }
    // Injector rule. `flat` covers kept classes too: an injector whose
    // component stays verbatim must stay verbatim itself.
    let flat: HashMap<String, &String> =
        all_names.iter().map(|x| (x.replace('$', "_"), x)).collect();
    for name in &defined {
        let Some(stem) = name.strip_suffix(MEMBERS_INJECTOR_SUFFIX) else {
            continue;
        };
        let Some(component) = flat.get(stem) else {
            continue;
        };
        match map.classes.get(*component).cloned() {
            Some(target) => {
                map.classes
                    .insert(name.clone(), format!("{target}{MEMBERS_INJECTOR_SUFFIX}"));
            }
            None => {
                map.classes.remove(name);
            }
        }
    }

    // ── Members ─────────────────────────────────────────────────────────
    let clashes: Vec<&String> = spelled
        .iter()
        .filter(|n| map.members.values().any(|t| t == *n))
        .collect();
    if !clashes.is_empty() {
        return Err(invalid(format!(
            "the app spells member name(s) that are targets of the release map: {}; \
             rename them in the app source",
            clashes
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let mut reserved: BTreeSet<String> = spelled;
    for dir in opts.reserve_dirs {
        reserved.extend(member_census(dir, false)?.spelled);
    }
    reserved.extend(opts.reserve_names.iter().cloned());
    reserved.extend(keep.members.iter().cloned());
    reserved.extend(map.members.keys().cloned());
    reserved.extend(map.members.values().cloned());

    let mut next_member = map
        .members
        .values()
        .filter_map(|t| member_inverse(t))
        .map(|raw| raw + 1)
        .max()
        .unwrap_or(0);
    let mut added = 0usize;
    for name in candidates {
        if keep.is_member_kept(&name)
            || map.members.contains_key(&name)
            || kept_spelled.contains(&name)
        {
            continue;
        }
        let target = loop {
            let t = member_suffix(&mut next_member);
            if !reserved.contains(&t) {
                break t;
            }
        };
        map.members.insert(name, target);
        added += 1;
    }
    if let Err(e) = map.verify_injective() {
        return Err(invalid(e));
    }
    eprintln!(
        "cut-app: {} app classes under c/, {added} app member targets",
        defined.len()
    );
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
                    ("id", "()V"),      // too short
                    ("<init>", "()V"),
                ],
            ),
        );
        let mut base = ShrinkMap::new();
        base.members.insert("setText".into(), "f".into());
        let reserve = vec!["g".to_string(), "PI".to_string()];
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
}
