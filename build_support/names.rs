// SPDX-License-Identifier: GPL-3.0-only
//! Generator for the `c::` / `m::` / `d::` name-constant modules — one
//! spelling per build, chosen here, for every Java class, member and
//! descriptor the runtime names from Rust.
//!
//! Under `--shrink` the loaded framework spells `picodroid/view/View` as
//! `a/AB`, `toString` as `xy` and `(Ljava/lang/String;)V` as `(Lb/AQ;)V`.
//! Rather than translate at run time (the `unshrink_class` match and the
//! `b/` class-file boundary this replaced — both spellings in flash, a
//! 300-arm `match` on every native call), every Rust site names a Java
//! thing through a generated `const` whose *value* is whatever the loaded
//! framework spells:
//!
//! ```text
//! c::picodroid_view_View   "picodroid/view/View"     or "a/AB"
//! c::java_lang_String      "java/lang/String"        or "b/AQ"
//! m::toString              "toString"                or "xy"
//! d::String__V             "(Ljava/lang/String;)V"   or "(Lb/AQ;)V"
//! ```
//!
//! A no-shrink build compiles to exactly the literals it matched before; a
//! shrink build carries no original spelling anywhere. The inputs are
//! committed, map-independent lists — `sdk/class-names.tsv`,
//! `sdk/member-names.tsv`, `sdk/api-contract.tsv`, `sdk/descriptors.tsv` —
//! so a bare `cargo build` with no Gradle run and no active map compiles
//! too. Values come from the active shrink map (`PICODROID_SHRINK=1`)
//! through the two lookup closures the caller supplies; the same function
//! serves `jvm/build.rs` and `picodroid-core/build.rs`, each into its own
//! `OUT_DIR`, and a picodroid-core test proves the two modules agree.
//!
//! Idents: a class is its internal name with `/` and `$` as `_`
//! (`picodroid_app_AlertDialog_Builder`); a member is its name, raw-escaped
//! when it is a Rust keyword (`r#await`); a descriptor's ident is the first
//! column of `sdk/descriptors.tsv`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Spelling lookups against the active map: `None` = unmapped, keep the
/// original. Both are identity when no map is active.
pub struct Targets<'a> {
    pub class: &'a dyn Fn(&str) -> Option<String>,
    pub member: &'a dyn Fn(&str) -> Option<String>,
}

/// The committed inputs, read from `root`.
pub struct Sources {
    /// Every class the runtime may name (`sdk/class-names.tsv` ∪ contract
    /// owners and descriptor references).
    pub classes: BTreeSet<String>,
    /// Every member name the runtime may name (`sdk/member-names.tsv` ∪ the
    /// contract's member column).
    pub members: BTreeSet<String>,
    /// `(ident, descriptor)` rows of `sdk/descriptors.tsv`.
    pub descriptors: Vec<(String, String)>,
}

fn read_lines(path: &Path) -> Vec<String> {
    println!("cargo:rerun-if-changed={}", path.display());
    match fs::read_to_string(path) {
        Ok(text) => text
            .lines()
            .map(str::trim_end)
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(String::from)
            .collect(),
        Err(_) => {
            println!(
                "cargo:warning={} is missing — the generated name consts will be incomplete; \
                 run scripts/gen-api-contract.sh",
                path.display()
            );
            Vec::new()
        }
    }
}

/// `L…;` class references inside a descriptor.
fn class_refs(desc: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = desc.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'L' {
            if let Some(end) = desc[i..].find(';') {
                out.push(desc[i + 1..i + end].to_string());
                i += end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

pub fn read_sources(root: &Path) -> Sources {
    let sdk = root.join("sdk");
    let mut classes: BTreeSet<String> = BTreeSet::new();
    let mut members: BTreeSet<String> = BTreeSet::new();

    for line in read_lines(&sdk.join("class-names.tsv")) {
        classes.insert(line.trim().to_string());
    }
    for line in read_lines(&sdk.join("member-names.tsv")) {
        if let Some((_kind, name)) = line.split_once('\t') {
            members.insert(name.trim().to_string());
        }
    }
    // api-contract.tsv: `owner \t member \t descriptor` rows (member and
    // descriptor may be empty for class-only rows); `@hint` lines are advice.
    for line in read_lines(&sdk.join("api-contract.tsv")) {
        if line.starts_with('@') {
            continue;
        }
        let mut f = line.split('\t');
        if let Some(owner) = f.next() {
            if !owner.is_empty() {
                classes.insert(owner.to_string());
            }
        }
        if let Some(name) = f.next() {
            if !name.is_empty() {
                members.insert(name.to_string());
            }
        }
        if let Some(desc) = f.next() {
            classes.extend(class_refs(desc));
        }
    }
    let mut descriptors = Vec::new();
    for line in read_lines(&sdk.join("descriptors.tsv")) {
        let Some((ident, desc)) = line.split_once('\t') else {
            panic!("sdk/descriptors.tsv: malformed row {line:?} (want `ident<TAB>descriptor`)");
        };
        descriptors.push((ident.trim().to_string(), desc.trim().to_string()));
    }
    Sources {
        classes,
        members,
        descriptors,
    }
}

/// The Rust identifier for a class internal name.
pub fn class_ident(name: &str) -> String {
    name.chars()
        .map(|ch| if ch == '/' || ch == '$' { '_' } else { ch })
        .collect()
}

/// The Rust identifier a Java member name is exposed as in `m::`. Names that
/// are Rust keywords are raw (`r#type`); the four that cannot be raw get a
/// trailing underscore; javac synthetics (`$`) and `<init>`-style names,
/// which no Rust code matches, get none.
pub fn rust_ident_for(name: &str) -> Option<String> {
    if name.is_empty() || name.contains('$') || name.starts_with('<') {
        return None;
    }
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || name.as_bytes()[0].is_ascii_digit()
    {
        return None;
    }
    Some(match name {
        "self" | "super" | "crate" | "Self" => format!("{name}_"),
        "as" | "break" | "const" | "continue" | "else" | "enum" | "extern" | "false" | "fn"
        | "for" | "if" | "impl" | "in" | "let" | "loop" | "match" | "mod" | "move" | "mut"
        | "pub" | "ref" | "return" | "static" | "struct" | "trait" | "true" | "type" | "unsafe"
        | "use" | "where" | "while" | "async" | "await" | "dyn" | "abstract" | "become" | "box"
        | "do" | "final" | "macro" | "override" | "priv" | "typeof" | "unsized" | "virtual"
        | "yield" | "try" | "gen" => format!("r#{name}"),
        _ => name.to_string(),
    })
}

/// Rewrite every `L…;` class reference of `desc` through `class`.
pub fn rewrite_descriptor(desc: &str, class: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(desc.len());
    let mut rest = desc;
    while let Some(pos) = rest.find('L') {
        let (head, tail) = rest.split_at(pos);
        out.push_str(head);
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let name = &tail[1..end];
        out.push('L');
        match class(name) {
            Some(t) => out.push_str(&t),
            None => out.push_str(name),
        }
        out.push(';');
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Write `<out>/names.rs`. `shrink_active` is recorded as a `const` for
/// tests that need to know whether the loaded spelling differs.
pub fn emit_names(out: &Path, root: &Path, targets: &Targets<'_>, shrink_active: bool) {
    let src = read_sources(root);
    let mut body = String::from(
        "// Generated by build_support/names.rs — do not edit.\n\n\
         /// Whether an active shrink map renamed anything in this build.\n\
         pub const SHRINK_ACTIVE: bool = ",
    );
    body.push_str(if shrink_active { "true" } else { "false" });
    body.push_str(";\n\n");

    // ---- classes -------------------------------------------------------
    body.push_str(
        "/// Java class names as the loaded framework spells them — the active\n\
         /// shrink map's target under `--shrink`, the original otherwise. Never\n\
         /// spell one as a string literal (`no_original_name_literals`).\n\
         #[allow(non_upper_case_globals, dead_code)]\n\
         pub mod c {\n",
    );
    let mut class_pairs: Vec<(String, String)> = Vec::new();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for name in &src.classes {
        let ident = class_ident(name);
        if let Some(prev) = seen.insert(ident.clone(), name.clone()) {
            panic!("class names {prev:?} and {name:?} both map to the Rust ident {ident}");
        }
        let value = (targets.class)(name).unwrap_or_else(|| name.clone());
        body.push_str(&format!("    pub const {ident}: &str = {value:?};\n"));
        class_pairs.push((name.clone(), value));
    }
    body.push_str("}\n\n");

    // ---- members -------------------------------------------------------
    body.push_str(
        "/// Java method and field names as the loaded framework spells them.\n\
         #[allow(non_upper_case_globals, dead_code)]\n\
         pub mod m {\n",
    );
    let mut member_pairs: Vec<(String, String)> = Vec::new();
    for name in &src.members {
        let value = (targets.member)(name).unwrap_or_else(|| name.clone());
        if value != *name {
            member_pairs.push((name.clone(), value.clone()));
        }
        let Some(ident) = rust_ident_for(name) else {
            continue;
        };
        body.push_str(&format!("    pub const {ident}: &str = {value:?};\n"));
    }
    body.push_str("}\n\n");

    // ---- descriptors ---------------------------------------------------
    body.push_str(
        "/// Method and type descriptors as the loaded framework spells them,\n\
         /// from `sdk/descriptors.tsv`.\n\
         #[allow(non_upper_case_globals, dead_code)]\n\
         pub mod d {\n",
    );
    let mut dseen: BTreeSet<String> = BTreeSet::new();
    for (ident, desc) in &src.descriptors {
        assert!(
            dseen.insert(ident.clone()),
            "sdk/descriptors.tsv: duplicate ident {ident}"
        );
        let value = rewrite_descriptor(desc, targets.class);
        body.push_str(&format!("    pub const {ident}: &str = {value:?};\n"));
    }
    body.push_str("}\n\n");

    // ---- test-only reverse translation ---------------------------------
    // The shrink lane reads names out of the loaded corpus and must compare
    // them against original-name tables (the contract generator, the
    // method-table cross-check). Firmware never needs any of this.
    body.push_str(
        "/// `(original, loaded)` for every class in `sdk/class-names.tsv` — the\n\
         /// oracle the boundary tests drive.\n\
         #[cfg(test)]\n\
         pub static CLASS_PAIRS: &[(&str, &str)] = &[\n",
    );
    for (from, to) in &class_pairs {
        body.push_str(&format!("    ({from:?}, {to:?}),\n"));
    }
    body.push_str("];\n\n");
    body.push_str(
        "/// `(original, loaded)` for every member the active map renames.\n\
         #[cfg(test)]\n\
         pub static MEMBER_PAIRS: &[(&str, &str)] = &[\n",
    );
    for (from, to) in &member_pairs {
        body.push_str(&format!("    ({from:?}, {to:?}),\n"));
    }
    body.push_str("];\n\n");
    for (fn_name, doc, pairs, reverse) in [
        (
            "unshrink_class",
            "Original spelling of a loaded class name (test-only).",
            &class_pairs,
            true,
        ),
        (
            "shrink_class",
            "Loaded spelling of an original class name (test-only).",
            &class_pairs,
            false,
        ),
        (
            "unshrink_member",
            "Original spelling of a loaded member name (test-only).",
            &member_pairs,
            true,
        ),
        (
            "shrink_member",
            "Loaded spelling of an original member name (test-only).",
            &member_pairs,
            false,
        ),
    ] {
        body.push_str(&format!(
            "/// {doc}\n#[cfg(test)]\npub fn {fn_name}(name: &str) -> &str {{\n    match name {{\n"
        ));
        for (from, to) in pairs.iter() {
            if from == to {
                continue;
            }
            if reverse {
                body.push_str(&format!("        {to:?} => {from:?},\n"));
            } else {
                body.push_str(&format!("        {from:?} => {to:?},\n"));
            }
        }
        body.push_str("        other => other,\n    }\n}\n\n");
    }
    body.push_str(
        "/// Un-shrink every `L<class>;` chunk of a descriptor (test-only).\n\
         #[cfg(test)]\n\
         pub fn unshrink_descriptor(desc: &str) -> alloc::string::String {\n\
         \x20   let mut out = alloc::string::String::with_capacity(desc.len());\n\
         \x20   let mut rest = desc;\n\
         \x20   while let Some(pos) = rest.find('L') {\n\
         \x20       let (head, tail) = rest.split_at(pos);\n\
         \x20       out.push_str(head);\n\
         \x20       let Some(end) = tail.find(';') else {\n\
         \x20           out.push_str(tail);\n\
         \x20           return out;\n\
         \x20       };\n\
         \x20       out.push('L');\n\
         \x20       out.push_str(unshrink_class(&tail[1..end]));\n\
         \x20       out.push(';');\n\
         \x20       rest = &tail[end + 1..];\n\
         \x20   }\n\
         \x20   out.push_str(rest);\n\
         \x20   out\n\
         }\n",
    );
    fs::write(out.join("names.rs"), body)
        .unwrap_or_else(|e| panic!("cannot write {}/names.rs: {e}", out.display()));
}
