// SPDX-License-Identifier: GPL-3.0-only
//! Generator for the compile-time API contract, `sdk/api-contract.tsv` —
//! the `java/**` and `javax/**` surface pico-jvm actually serves, written
//! from the runtime's own tables so the Gradle-side verifier
//! (`buildSrc/.../classfile/ApiContract.kt`, task `verifyApiContract`) can
//! reject an app reference at build time instead of letting it die on
//! device as `NoSuchMethod`.
//!
//! Inputs, unioned:
//! * the SDK's `java/**`/`javax/**` class files (`FRAMEWORK_CLASSES`) — every
//!   non-private member, descriptor-exact; their `ACC_NATIVE` methods are
//!   proven handled by `method_tables.rs` (only classes with bodies exist:
//!   `no_bodiless_java_framework_classes`);
//! * `pico_jvm::native::BUILTIN_METHODS` and this crate's
//!   `PLATFORM_BUILTIN_METHODS` — the classfile-less builtins, name-level
//!   unless the arm is descriptor-guarded — plus `BUILTIN_INTERFACE_METHODS`,
//!   the lambda-targetable SAM members of the classfile-less interfaces;
//! * `BUILTIN_CLASS_NAMES`, `BUILTIN_SUPER`, `BUILTIN_INTERFACES` — which
//!   names resolve at all, and the hierarchy the verifier walks;
//! * `TOLERATED` / `NAME_ONLY_CLASSES` — static shapes javac emits but never
//!   executes, and `CONTRACT_HINTS` — what to use instead.
//!
//! `api_contract_is_current` regenerates the text and compares it with the
//! committed file; `scripts/gen-api-contract.sh` runs it with
//! `PICODROID_UPDATE_API_CONTRACT=1` to rewrite the file. Both shrink lanes
//! of `scripts/test.sh` must produce identical output (names and descriptors
//! are un-shrunk first).
//!
//! Test-only, wired via `#[cfg(test)] #[path]` in `lib.rs` like
//! `method_tables.rs`: nothing here may reach firmware (`class_registry.rs`
//! is compiled in, so a hint table there would be RP2040 `.rodata`).

/// Advice the verifier prints beside a rejected reference:
/// `(owner, or "prefix*"; member name, or "" for the class; text)`. Every
/// hint must name something the contract does NOT serve
/// (`contract_hints_name_nothing_served`), so a hint can never contradict a
/// working call.
pub const CONTRACT_HINTS: &[(&str, &str, &str)] = &[
    (
        "java/lang/System",
        "out",
        "there is no stdout on device — log with picodroid.util.Log.i(tag, msg)",
    ),
    (
        "java/lang/System",
        "err",
        "there is no stderr on device — log with picodroid.util.Log.e(tag, msg)",
    ),
    (
        "java/lang/System",
        "exit",
        "there is no process to exit — finish() the Activity or return from main",
    ),
    (
        "java/lang/System",
        "nanoTime",
        "use System.currentTimeMillis() (millisecond resolution is what the boards have)",
    ),
    (
        "java/io/PrintStream",
        "",
        "there is no stdout on device — log with picodroid.util.Log",
    ),
    (
        "java/util/LinkedList",
        "",
        "not a pico-jvm builtin — use java.util.ArrayList",
    ),
    (
        "java/util/ArrayDeque",
        "",
        "not a pico-jvm builtin — use java.util.ArrayList",
    ),
    (
        "java/util/Stack",
        "",
        "not a pico-jvm builtin — use java.util.ArrayList",
    ),
    (
        "java/util/Vector",
        "",
        "not a pico-jvm builtin — use java.util.ArrayList",
    ),
    (
        "java/util/TreeMap",
        "",
        "not a pico-jvm builtin — use java.util.HashMap (unordered) and sort the keys when needed",
    ),
    (
        "java/util/TreeSet",
        "",
        "not a pico-jvm builtin — use java.util.HashSet",
    ),
    (
        "java/lang/String",
        "matches",
        "no regex engine — use startsWith / endsWith / contains / indexOf (split() takes a literal separator)",
    ),
    (
        "java/lang/String",
        "replaceAll",
        "no regex engine — String.replace(CharSequence, CharSequence) is literal",
    ),
    (
        "java/lang/String",
        "replaceFirst",
        "no regex engine — String.replace(CharSequence, CharSequence) is literal",
    ),
    (
        "java/lang/Thread*",
        "",
        "use picodroid.concurrent.Thread (same API shape) — java.lang.Thread is not a builtin",
    ),
    (
        "java/util/concurrent/*",
        "",
        "use picodroid.concurrent.* (ExecutorService, Future, atomics, CountDownLatch, TimeUnit)",
    ),
    (
        "java/util/function/*",
        "",
        "no java.util.function — use Runnable, Comparator, or your own single-method interface",
    ),
    ("java/lang/Math", "random", "use java.util.Random"),
    (
        "java/util/Arrays",
        "asList",
        "build an ArrayList and add() the elements",
    ),
    (
        "java/util/Arrays",
        "equals",
        "compare the elements in a loop",
    ),
    ("java/lang/Float", "isNaN", "a NaN is the only value that is != itself"),
    ("java/lang/Double", "isNaN", "a NaN is the only value that is != itself"),
    (
        "java/io/BufferedReader",
        "",
        "no java.io streams — read files with picodroid.io.FileInputStream into a byte[]",
    ),
    (
        "java/io/InputStreamReader",
        "",
        "no java.io streams — read files with picodroid.io.FileInputStream into a byte[]",
    ),
    (
        "java/io/FileReader",
        "",
        "no java.io streams — read files with picodroid.io.FileInputStream into a byte[]",
    ),
    (
        "java/io/FileWriter",
        "",
        "no java.io streams — write files with picodroid.io.FileOutputStream",
    ),
    (
        "java/io/PrintWriter",
        "",
        "no java.io streams — write files with picodroid.io.FileOutputStream",
    ),
];

/// Rows the verifier must accept although the runtime does not serve them:
/// shapes javac / kotlinc emit statically in every class of their kind and
/// that are never executed by a working program. `(owner, name, desc, why)`.
pub const TOLERATED: &[(&str, &str, &str, &str)] = &[
    (
        "java/lang/Enum",
        "valueOf",
        "(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;",
        "the synthetic callee of every enum's valueOf(String); unsupported (compatibility matrix), never reached by the demos",
    ),
    (
        "java/util/Locale",
        "ROOT",
        "Ljava/util/Locale;",
        "kotlinc's uppercase()/lowercase() pass it to String.toUpperCase(Locale); getstatic on a classfile-less class reads null and the arm ignores it",
    ),
    (
        "java/util/Locale",
        "US",
        "Ljava/util/Locale;",
        "as ROOT",
    ),
];

/// Classes the JVM tolerates by name only — usable as a superclass, catch
/// type or cast target, never instantiated (a `new` of one would yield an
/// `"unknown"`-class object). Emitted as `@nameonly` rows, which the
/// verifier accepts for every class kind except `new` / `anewarray`.
/// `(class, why)`.
pub const NAME_ONLY_CLASSES: &[(&str, &str)] = &[
    (
        "java/util/AbstractList",
        "compile-time superclass of the kotlin-shim's EnumEntriesList; its <init> falls through to Object",
    ),
    (
        "java/lang/NoSuchFieldError",
        "catch_type in every $WhenMappings / $SwitchMap; never thrown by this JVM",
    ),
    (
        "java/lang/CloneNotSupportedException",
        "catch_type in every clone() override that calls super.clone(); Object.clone never throws it here",
    ),
];

#[cfg(test)]
mod tests {
    use super::{CONTRACT_HINTS, NAME_ONLY_CLASSES, TOLERATED};
    use crate::native_method_tables_tests::{ALL_HANDLED, PLATFORM_BUILTIN_METHODS};
    use crate::shrink_names::{unshrink_class, unshrink_descriptor};
    use pico_jvm::class_file::ClassFile;
    use pico_jvm::interpreter::{BUILTIN_INTERFACES, BUILTIN_SUPER};
    use pico_jvm::native::{
        BuiltinMethodRow, BUILTIN_CLASS_NAMES, BUILTIN_INTERFACE_METHODS, BUILTIN_METHODS,
        BUILTIN_SDK_HANDLED,
    };
    use std::collections::BTreeSet;
    use std::fmt::Write as _;

    /// Repo-relative path of the generated file.
    const CONTRACT_PATH: &str = "sdk/api-contract.tsv";
    /// The committed copy, baked in so a stale file fails `cargo test`.
    const COMMITTED: &str = include_str!("../../../sdk/api-contract.tsv");
    /// Env var that makes `api_contract_is_current` rewrite the file.
    const UPDATE_VAR: &str = "PICODROID_UPDATE_API_CONTRACT";
    /// JVMS §4.6 `ACC_PRIVATE`.
    const ACC_PRIVATE: u16 = 0x0002;

    /// `(owner, name, desc)`; a class row has empty name and desc, a
    /// name-level row an empty desc.
    type Row = (String, String, String);

    fn row(owner: &str, name: &str, desc: &str) -> Row {
        (owner.to_string(), name.to_string(), desc.to_string())
    }

    fn utf8(cf: &ClassFile, index: u16, what: &str) -> String {
        core::str::from_utf8(
            cf.cp_utf8(index)
                .unwrap_or_else(|| panic!("{what}: constant-pool index {index} is not Utf8")),
        )
        .unwrap_or_else(|_| panic!("{what}: not UTF-8"))
        .to_string()
    }

    /// Class and member rows of every SDK `java/**` / `javax/**` class file
    /// (original names — the shrink lane loads shrunk ones), plus how many
    /// such classes were seen (the non-vacuity guard).
    fn sdk_rows() -> (BTreeSet<Row>, usize) {
        let mut rows = BTreeSet::new();
        let mut classes = 0;
        for bytes in crate::framework_classes::FRAMEWORK_CLASSES {
            let cf = ClassFile::parse(bytes).expect("parse framework class");
            let loaded = core::str::from_utf8(cf.class_name().expect("class name"))
                .expect("class name is UTF-8");
            let class = unshrink_class(loaded);
            if !(class.starts_with("java/") || class.starts_with("javax/")) {
                continue;
            }
            classes += 1;
            rows.insert(row(class, "", ""));
            for m in cf.methods() {
                if m.access_flags & ACC_PRIVATE != 0 {
                    continue;
                }
                let name = utf8(&cf, m.name_index, "method name");
                if name == "<clinit>" {
                    continue;
                }
                let desc = utf8(&cf, m.descriptor_index, "method descriptor");
                rows.insert((class.to_string(), name, unshrink_descriptor(&desc)));
            }
            // FieldInfo carries no access flags; a private field row is
            // harmless (javac already refuses the access).
            for f in cf.fields().iter().chain(cf.static_fields()) {
                let name = utf8(&cf, f.name_index, "field name");
                let desc = utf8(&cf, f.descriptor_index, "field descriptor");
                rows.insert((class.to_string(), name, unshrink_descriptor(&desc)));
            }
        }
        (rows, classes)
    }

    fn builtin_rows(table: &[(&str, &[BuiltinMethodRow])], rows: &mut BTreeSet<Row>) {
        for &(class, methods) in table {
            for &(name, descs) in methods {
                if descs.is_empty() {
                    rows.insert(row(class, name, ""));
                } else {
                    for d in descs {
                        rows.insert(row(class, name, d));
                    }
                }
            }
        }
    }

    struct Contract {
        rows: BTreeSet<Row>,
        sdk_classes: usize,
        text: String,
    }

    /// The contract as the runtime tables define it right now.
    fn generate() -> Contract {
        let (mut rows, sdk_classes) = sdk_rows();

        // Every name the JVM resolves at all: builtins, the hierarchy tables'
        // keys and values, and the name-only classes.
        for name in BUILTIN_CLASS_NAMES {
            rows.insert(row(name, "", ""));
        }
        for (child, parent) in BUILTIN_SUPER {
            rows.insert(row(child, "", ""));
            rows.insert(row(parent, "", ""));
        }
        for (class, ifaces) in BUILTIN_INTERFACES {
            rows.insert(row(class, "", ""));
            for i in ifaces.iter() {
                rows.insert(row(i, "", ""));
            }
        }
        builtin_rows(BUILTIN_METHODS, &mut rows);
        builtin_rows(BUILTIN_INTERFACE_METHODS, &mut rows);
        builtin_rows(PLATFORM_BUILTIN_METHODS, &mut rows);
        // Platform natives on java/** classes (System.currentTimeMillis);
        // already exact rows from the class file, unioned for completeness.
        for table in ALL_HANDLED {
            for (class, method, desc) in table.iter() {
                if class.starts_with("java/") {
                    rows.insert(row(class, method, desc));
                }
            }
        }
        for (owner, name, desc, _) in TOLERATED {
            rows.insert(row(owner, name, desc));
        }

        // A member row whose owner has no class row would be unreachable by
        // the verifier's resolution except through TOLERATED (Locale).
        let tolerated_owners: BTreeSet<&str> = TOLERATED.iter().map(|t| t.0).collect();
        for (owner, name, _) in &rows {
            assert!(
                name.is_empty()
                    || rows.contains(&row(owner, "", ""))
                    || tolerated_owners.contains(owner.as_str()),
                "member row for {owner} but no class row — a table names a class the JVM \
                 does not resolve"
            );
        }
        for (owner, name, text) in CONTRACT_HINTS {
            assert!(
                !owner.contains(['\t', '\n'])
                    && !name.contains(['\t', '\n'])
                    && !text.contains(['\t', '\n']),
                "hint text must not contain tabs or newlines: {owner}.{name}"
            );
        }

        let text = render(&rows);
        Contract {
            rows,
            sdk_classes,
            text,
        }
    }

    fn render(rows: &BTreeSet<Row>) -> String {
        let mut out = String::new();
        out.push_str(
            "# sdk/api-contract.tsv — the java/** and javax/** surface pico-jvm serves.\n\
             #\n\
             # GENERATED by picodroid-core's `api_contract_is_current` test\n\
             # (picodroid-core/src/native_handler/api_contract.rs) from the runtime's\n\
             # own tables; do not edit by hand. Regenerate with\n\
             #     scripts/gen-api-contract.sh\n\
             # after changing an SDK java/** class, BUILTIN_METHODS / BUILTIN_CLASS_NAMES /\n\
             # BUILTIN_INTERFACE_METHODS (jvm/src/native/mod.rs), BUILTIN_SUPER / BUILTIN_INTERFACES\n\
             # (jvm/src/interpreter/helpers.rs), or picodroid-core's\n\
             # PLATFORM_BUILTIN_METHODS / CONTRACT_HINTS / TOLERATED.\n\
             #\n\
             # Consumed by the Gradle task `verifyApiContract`\n\
             # (buildSrc/src/main/kotlin/picodroid/classfile/ApiContract.kt), which runs\n\
             # on every app's compiled classes before they are packed into a PAPK.\n\
             #\n\
             # Grammar (tab-separated):\n\
             #   owner                            the class resolves: new / checkcast /\n\
             #                                    instanceof / catch / super / interface /\n\
             #                                    ldc / array element / lambda SAM type\n\
             #   owner<TAB>name<TAB>desc          member served with exactly this descriptor\n\
             #   owner<TAB>name<TAB>              member served for any descriptor (native\n\
             #                                    dispatch is keyed on (class, name))\n\
             #   @extends<TAB>child<TAB>parent    builtin superclass edge (BUILTIN_SUPER)\n\
             #   @implements<TAB>class<TAB>iface  builtin interface edge (BUILTIN_INTERFACES)\n\
             #   @nameonly<TAB>class              resolves as a catch / super / cast type only,\n\
             #                                    never instantiated\n\
             #   @hint<TAB>owner|prefix*<TAB>name|<empty><TAB>text\n\
             #                                    advice printed beside a rejected reference\n\
             #\n\
             # Resolution (mirrors pico-jvm's dispatch_native): a member is served on an\n\
             # owner when a row matches on the owner, on any @extends ancestor, or on\n\
             # java/lang/Object; a member of an interface is served when a class that\n\
             # @implements it (transitively) serves it, or an app class implementing it\n\
             # declares it. A class-file-backed class also serves its declared members\n\
             # by inheritance in the usual way.\n\
             #\n",
        );
        out.push_str(
            "# ── Tolerated: accepted although not served (static shapes never executed) ──\n",
        );
        for (owner, name, desc, why) in TOLERATED {
            let _ = writeln!(out, "#   {owner}.{name}{desc} — {why}");
        }
        for (class, why) in NAME_ONLY_CLASSES {
            let _ = writeln!(out, "#   {class} (name only) — {why}");
        }
        out.push_str("#\n# ── Classes and members ──\n");
        for (owner, name, desc) in rows {
            if name.is_empty() {
                let _ = writeln!(out, "{owner}");
            } else {
                let _ = writeln!(out, "{owner}\t{name}\t{desc}");
            }
        }
        out.push_str("#\n# ── Hierarchy ──\n");
        let mut edges: BTreeSet<String> = BTreeSet::new();
        for (child, parent) in BUILTIN_SUPER {
            edges.insert(format!("@extends\t{child}\t{parent}"));
        }
        for (class, ifaces) in BUILTIN_INTERFACES {
            for i in ifaces.iter() {
                edges.insert(format!("@implements\t{class}\t{i}"));
            }
        }
        for e in &edges {
            let _ = writeln!(out, "{e}");
        }
        out.push_str("#\n# ── Name-only classes ──\n");
        let mut name_only: Vec<&str> = NAME_ONLY_CLASSES.iter().map(|(c, _)| *c).collect();
        name_only.sort_unstable();
        for c in name_only {
            let _ = writeln!(out, "@nameonly\t{c}");
        }
        out.push_str("#\n# ── Hints ──\n");
        for (owner, name, text) in CONTRACT_HINTS {
            let _ = writeln!(out, "@hint\t{owner}\t{name}\t{text}");
        }
        out
    }

    /// The first few lines around the first difference, both sides.
    fn first_difference(generated: &str, committed: &str) -> String {
        let g: Vec<&str> = generated.lines().collect();
        let c: Vec<&str> = committed.lines().collect();
        let at = g
            .iter()
            .zip(c.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(g.len().min(c.len()));
        let lo = at.saturating_sub(2);
        let mut out = format!(
            "first difference at line {} (generated {} lines, committed {} lines)\n",
            at + 1,
            g.len(),
            c.len()
        );
        let _ = writeln!(out, "--- generated:");
        for l in g.iter().skip(lo).take(6) {
            let _ = writeln!(out, "    {l}");
        }
        let _ = writeln!(out, "--- committed:");
        for l in c.iter().skip(lo).take(6) {
            let _ = writeln!(out, "    {l}");
        }
        out
    }

    /// The committed `sdk/api-contract.tsv` equals what the runtime tables
    /// generate. With `PICODROID_UPDATE_API_CONTRACT=1` the file is rewritten
    /// instead (`scripts/gen-api-contract.sh`).
    #[test]
    fn api_contract_is_current() {
        let contract = generate();

        // Non-vacuity first: an empty FRAMEWORK_CLASSES (a bare `cargo test`
        // without PICODROID_APK_PATH) or a parser regression must not
        // silently generate a contract with no SDK rows.
        // Six today: Class, Math, System, Arrays, Collections, javax/inject/Provider
        // (the body-less java/** stubs were retired by T2.2).
        assert!(
            contract.sdk_classes >= 5,
            "only {} SDK java/javax class files seen — FRAMEWORK_CLASSES is empty (run \
             via scripts/test.sh or scripts/gen-api-contract.sh, which set \
             PICODROID_APK_PATH) or the class-file parser broke",
            contract.sdk_classes
        );
        let class_rows = contract.rows.iter().filter(|r| r.1.is_empty()).count();
        let member_rows = contract.rows.len() - class_rows;
        assert!(
            class_rows >= 70,
            "only {class_rows} class rows — a table collapsed"
        );
        assert!(
            member_rows >= 250,
            "only {member_rows} member rows — a table collapsed"
        );
        for (owner, name, desc) in [
            ("java/util/HashMap", "put", ""),
            ("java/util/Arrays", "sort", "([I)V"),
            ("java/lang/System", "currentTimeMillis", "()J"),
            ("java/lang/Object", "wait", "(J)V"),
            ("java/lang/String", "<init>", "([B)V"),
            ("javax/inject/Provider", "get", "()Ljava/lang/Object;"),
            ("java/lang/Runnable", "run", "()V"),
            ("java/util/List", "", ""),
        ] {
            assert!(
                contract.rows.contains(&row(owner, name, desc)),
                "expected row {owner} {name} {desc} is missing from the generated contract"
            );
        }
        // And a known miss stays a miss.
        assert!(
            !contract
                .rows
                .iter()
                .any(|(o, n, _)| o == "java/lang/String" && n == "matches"),
            "String.matches is not served by pico-jvm; the contract must not list it"
        );

        if contract.text == COMMITTED {
            return;
        }
        if std::env::var(UPDATE_VAR).as_deref() == Ok("1") {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join(CONTRACT_PATH);
            std::fs::write(&path, &contract.text)
                .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
            eprintln!(
                "wrote {} ({} class rows, {} member rows)",
                path.display(),
                class_rows,
                member_rows
            );
            return;
        }
        panic!(
            "{CONTRACT_PATH} is stale — the runtime tables changed. \
             {}Regenerate it with scripts/gen-api-contract.sh and commit the result.",
            first_difference(&contract.text, COMMITTED)
        );
    }

    /// A hint that names a served class or member would contradict a
    /// working call, and a prefix hint over a dispatched builtin would fire
    /// on that builtin's ordinary misses.
    #[test]
    fn contract_hints_name_nothing_served() {
        let contract = generate();
        for (owner, name, _) in CONTRACT_HINTS {
            if let Some(prefix) = owner.strip_suffix('*') {
                assert!(
                    name.is_empty(),
                    "prefix hint {owner} must be class-level (empty name)"
                );
                assert!(
                    !BUILTIN_METHODS.iter().any(|(c, _)| c.starts_with(prefix)),
                    "prefix hint {owner} covers a dispatched builtin"
                );
            } else if name.is_empty() {
                assert!(
                    !contract.rows.contains(&row(owner, "", ""))
                        && !NAME_ONLY_CLASSES.iter().any(|(c, _)| c == owner),
                    "hint for {owner} but the class is served"
                );
            } else {
                assert!(
                    !contract
                        .rows
                        .iter()
                        .any(|(o, n, _)| o == owner && n == name),
                    "hint for {owner}.{name} but the member is served"
                );
            }
        }
    }

    /// `BUILTIN_SDK_HANDLED` (the jvm-side natives of class-file-backed
    /// java/** classes) is a subset of the exact rows — the class file
    /// declares each of them, so the generated contract must too.
    #[test]
    fn builtin_sdk_handled_rows_are_exact_rows() {
        let contract = generate();
        for (class, method, desc) in BUILTIN_SDK_HANDLED {
            assert!(
                contract.rows.contains(&row(class, method, desc)),
                "BUILTIN_SDK_HANDLED row {class}.{method}{desc} is not an exact contract row"
            );
        }
    }
}
