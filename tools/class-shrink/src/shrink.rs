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
/// already rename in lockstep), not in `reserve_names` (an SDK name the base
/// map lacks is spelled verbatim by the framework, and so must an override of
/// it be), not kept, and not spelled by a kept class.
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
    // An SDK name the base map does not carry is one the framework spells
    // verbatim — declared since that release was cut, or too short to map.
    // An app member of that name may be an override (`onSaveInstanceState`
    // before its first release cut), so it stays verbatim too; renaming it
    // would silently detach the override from the framework's call.
    let sdk_names: BTreeSet<&str> = opts.reserve_names.iter().map(String::as_str).collect();
    for name in candidates {
        if keep.is_member_kept(&name)
            || map.members.contains_key(&name)
            || kept_spelled.contains(&name)
            || sdk_names.contains(name.as_str())
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
mod tests;
