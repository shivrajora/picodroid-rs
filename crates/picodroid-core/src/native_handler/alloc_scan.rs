// SPDX-License-Identifier: GPL-3.0-only
//! Ratchet on infallible allocations in the native arms.
//!
//! The rule the 2026-09-13 QA round established: the simulator's device heap
//! model is the RP2350's; the RP2040 (160 KB) and the touch kit (its LVGL
//! draw buffers) are tighter, so every infallible allocation on a path Java
//! can reach is a board reset waiting for a big enough app — `vec![0u8;
//! len]` for a 20 KB read took the RP2040 down where an `OutOfMemoryError`
//! was due (J24). J23–J26 made the formatter, the file streams, frames and
//! interning fallible; this scan keeps the remaining list from growing.
//!
//! It counts the shapes that allocate a whole new buffer — `to_vec`,
//! `Vec::with_capacity`, `format!`, `vec!`, `String::from`, `to_string`,
//! `to_owned`, `Box::new`, `collect` — per file under the two native-arm
//! trees, against the baseline below, and fails on any change in either
//! direction: growth asks for a `try_reserve` (and the `-2`/OOM answer the
//! HAL and streams use) or a baseline bump with a reason; shrinkage asks for
//! the baseline to follow, so it ratchets down. Growth on an existing
//! buffer (`push`, `extend`) is out of scope: it is everywhere, and the
//! sites that matter reserve first.
//!
//! Except for one kind of buffer, which gets a second, stricter rule: the
//! `ObjectHeap`'s own tables (`crates/jvm/src/object_heap/`). They live as
//! long as the heap and gain an entry per object, so their growth step is a
//! single request the size of the whole table — the lambda registry's was
//! the touch kit's 7,680 B reset, and the exception side tables had the same
//! shape. Every `self.<table>.push(` (or `insert`, `extend…`, `resize`)
//! there must follow a `reserve_fallible(&mut self.<table>` or
//! `self.<table>.try_reserve…` in the same function.
//!
//! Test-only, wired via `#[cfg(test)] #[path]` in `lib.rs` like
//! `api_contract.rs`.

#[cfg(test)]
use test_support::source_scan;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::source_scan::{read_stripped, rel, sources};

    /// The shapes counted, as substrings of comment-stripped source.
    const SHAPES: &[&str] = &[
        ".to_vec()",
        "Vec::with_capacity(",
        "format!(",
        "vec![",
        "String::from(",
        ".to_string()",
        ".to_owned()",
        "Box::new(",
        ".collect()",
        ".collect::<",
    ];

    /// Files under the scanned trees that never reach firmware: host-side
    /// generators and guards (`cfg(test)` from the first item), and this
    /// file, which quotes the shapes it looks for.
    const HOST_ONLY: &[&str] = &[
        "alloc_scan.rs",
        "api_contract.rs",
        "member_names.rs",
        "method_tables.rs",
        "tests.rs",
    ];

    /// `(file, sites)` — the accepted count per file, workspace-relative.
    /// Bump a row in the commit that adds the site and say in the message
    /// why it cannot fail on a full heap; lower it when a site goes.
    const BASELINE: &[(&str, usize)] = &[
        ("crates/jvm/src/native/arrays.rs", 5),
        ("crates/jvm/src/native/class_obj.rs", 1),
        ("crates/jvm/src/native/string.rs", 25),
        ("crates/jvm/src/native/string_format.rs", 2),
        ("crates/picodroid-core/src/native_handler/io/mod.rs", 5),
        ("crates/picodroid-core/src/native_handler/json.rs", 3),
        ("crates/picodroid-core/src/native_handler/threads.rs", 1),
    ];

    /// Source up to its trailing `#[cfg(test)] mod …` block: unit tests are
    /// at the bottom by convention, and their allocations never ship.
    fn firmware_part(text: &str) -> &str {
        let mut cut = text.len();
        let mut at = 0usize;
        for line in text.split_inclusive('\n') {
            if line.trim() == "#[cfg(test)]" {
                let rest = &text[at + line.len()..];
                if rest.trim_start().starts_with("mod ") {
                    cut = at;
                    break;
                }
            }
            at += line.len();
        }
        &text[..cut]
    }

    fn count_sites(text: &str) -> Vec<(usize, String)> {
        let mut hits = Vec::new();
        for (n, line) in text.lines().enumerate() {
            if SHAPES.iter().any(|s| line.contains(s)) {
                hits.push((n + 1, line.trim().to_string()));
            }
        }
        hits
    }

    #[test]
    fn infallible_allocation_sites_match_the_baseline() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = crate_root.parent().unwrap().parent().unwrap();
        let trees = [
            workspace.join("crates/jvm/src/native"),
            crate_root.join("src/native_handler"),
        ];
        let mut files = Vec::new();
        for tree in &trees {
            sources(tree, &["rs"], None, &mut files);
        }
        files.sort();

        let mut report = String::new();
        let mut current: Vec<(String, usize)> = Vec::new();
        for path in &files {
            let name = path.file_name().unwrap().to_str().unwrap();
            // A `tests/` directory is a test module split by topic; its files
            // are named for what they test (`tests/string.rs`), not `tests.rs`.
            let in_tests_dir = path.components().any(|c| c.as_os_str() == "tests");
            if HOST_ONLY.contains(&name) || in_tests_dir {
                continue;
            }
            let text = read_stripped(path);
            let hits = count_sites(firmware_part(&text));
            if hits.is_empty() {
                continue;
            }
            let file = rel(workspace, path);
            let accepted = BASELINE
                .iter()
                .find(|(f, _)| *f == file)
                .map_or(0, |(_, n)| *n);
            if hits.len() != accepted {
                report.push_str(&format!(
                    "\n{file}: {} infallible allocation site(s), baseline {accepted}\n",
                    hits.len()
                ));
                for (line, src) in &hits {
                    report.push_str(&format!("    {line}: {src}\n"));
                }
            }
            current.push((file, hits.len()));
        }
        for (file, _) in BASELINE {
            if !current.iter().any(|(f, _)| f == file) {
                report.push_str(&format!(
                    "\n{file}: in the baseline but has no sites (or is gone)\n"
                ));
            }
        }

        assert!(
            report.is_empty(),
            "infallible allocation sites in the native arms changed:{report}\n\
             A new site on a path Java can reach is a board reset on the RP2040 or the \
             touch kit once the heap is full (docs/qa-2026-09-13-followups.md §3): \
             reserve with try_reserve and answer -2 / OutOfMemoryError like the HAL's \
             read_at does, or — if the site cannot run on a full heap — bump its row \
             in BASELINE (native_handler/alloc_scan.rs) in the same commit and say why. \
             A site that went away lowers its row.\n\nCurrent counts:\n{}",
            current
                .iter()
                .map(|(f, n)| format!("        (\"{f}\", {n}),\n"))
                .collect::<String>()
        );
    }

    /// Calls that grow an existing `Vec`, as the text that follows the
    /// receiver.
    const GROWTH: &[&str] = &[
        ".push(",
        ".insert(",
        ".extend(",
        ".extend_from_slice(",
        ".resize(",
    ];

    /// `ObjectHeap` tables allowed to grow without a reservation, and why.
    const UNRESERVED_TABLES: &[(&str, &str)] = &[(
        "alloc_histo",
        "mem-diag builds only: the opt-in allocation histogram, 4 B per loaded class",
    )];

    /// `(line number, line)` for `text` minus its `#[cfg(test)] mod … { … }`
    /// blocks, which rustfmt closes with a `}` in column 0. Unlike
    /// [`firmware_part`] this keeps code after a test module:
    /// `object_heap/mod.rs` has an `impl ObjectHeap` between its two.
    fn without_test_mods(text: &str) -> Vec<(usize, &str)> {
        let mut out = Vec::new();
        let mut lines = text.lines().enumerate().peekable();
        while let Some((n, line)) = lines.next() {
            let opens_test_mod = line.trim() == "#[cfg(test)]"
                && lines
                    .peek()
                    .is_some_and(|(_, l)| l.starts_with("mod ") && l.trim_end().ends_with('{'));
            if opens_test_mod {
                for (_, l) in lines.by_ref() {
                    if l == "}" {
                        break;
                    }
                }
                continue;
            }
            out.push((n + 1, line));
        }
        out
    }

    /// One `self.<table>.<growth>(` call: its line, the table, the source,
    /// and whether the same function reserved that table before it.
    struct TableGrowth {
        line: usize,
        table: String,
        src: String,
        reserved: bool,
    }

    fn table_growth(lines: &[(usize, &str)]) -> Vec<TableGrowth> {
        let mut sites = Vec::new();
        let mut fn_start = 0;
        for (i, &(n, line)) in lines.iter().enumerate() {
            let t = line.trim_start();
            if t.starts_with("fn ") || t.contains(" fn ") {
                fn_start = i;
            }
            let mut rest = line;
            while let Some(at) = rest.find("self.") {
                rest = &rest[at + "self.".len()..];
                let end = rest
                    .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                    .unwrap_or(rest.len());
                let (table, after) = rest.split_at(end);
                if table.is_empty() || !GROWTH.iter().any(|g| after.starts_with(g)) {
                    continue;
                }
                let fallible = format!("reserve_fallible(&mut self.{table}");
                let try_reserve = format!("self.{table}.try_reserve");
                let reserved = lines[fn_start..i]
                    .iter()
                    .any(|(_, l)| l.contains(&fallible) || l.contains(&try_reserve));
                sites.push(TableGrowth {
                    line: n,
                    table: table.to_string(),
                    src: line.trim().to_string(),
                    reserved,
                });
            }
        }
        sites
    }

    #[test]
    fn object_heap_tables_reserve_before_they_grow() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = crate_root.parent().unwrap().parent().unwrap();
        let mut files = Vec::new();
        sources(
            &workspace.join("crates/jvm/src/object_heap"),
            &["rs"],
            None,
            &mut files,
        );
        files.sort();

        let mut seen = 0;
        let mut report = String::new();
        for path in &files {
            let text = read_stripped(path);
            let sites = table_growth(&without_test_mods(&text));
            seen += sites.len();
            for s in sites {
                if !s.reserved && !UNRESERVED_TABLES.iter().any(|(t, _)| *t == s.table) {
                    let file = rel(workspace, path);
                    report.push_str(&format!("\n    {file}:{}: {}", s.line, s.src));
                }
            }
        }
        // The heap's tables all grow somewhere; none found means the scan
        // lost its target (a move, or a rename of the receiver).
        assert!(
            seen > 0,
            "no ObjectHeap table growth found: repoint the scan"
        );
        assert!(
            report.is_empty(),
            "ObjectHeap tables grown with no reservation in the same function:{report}\n\n\
             These tables gain an entry per object, so a growth step is one request the \
             size of the whole table, and an infallible one resets the board on a full heap \
             (the touch kit's 7,680 B lambda registry, docs/qa-2026-09-13-followups.md §3). \
             Reserve through reserve_fallible first and hand the caller Exhausted, or, for a \
             table that cannot grow on a full heap, add it to UNRESERVED_TABLES \
             (native_handler/alloc_scan.rs) with the reason."
        );
    }

    #[test]
    fn the_table_rule_flags_growth_its_function_did_not_reserve() {
        let src = "\
impl ObjectHeap {
    pub fn register(&mut self, k: u16) -> Result<(), Exhausted> {
        reserve_fallible(&mut self.table, 1)?;
        self.table.push(k);
        Ok(())
    }
    pub fn register_blind(&mut self, k: u16) {
        self.table.push(k);
    }
}
#[cfg(test)]
mod tests {
    fn t(h: &mut ObjectHeap) {
        h.table.push(0);
        self.table.push(1);
    }
}
impl ObjectHeap {
    fn after_the_tests(&mut self) {
        self.other.insert(0, 1);
    }
}
";
        let sites = table_growth(&without_test_mods(src));
        let got: Vec<(usize, &str, bool)> = sites
            .iter()
            .map(|s| (s.line, s.table.as_str(), s.reserved))
            .collect();
        // The reservation in `register` does not cover `register_blind`,
        // the test module is skipped, and the code after it is not.
        assert_eq!(
            got,
            [
                (4, "table", true),
                (8, "table", false),
                (20, "other", false)
            ]
        );
    }
}
