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
//! Test-only, wired via `#[cfg(test)] #[path]` in `lib.rs` like
//! `api_contract.rs`.

#[cfg(test)]
#[path = "../../../test_support/source_scan.rs"]
mod source_scan;

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
            if HOST_ONLY.contains(&name) {
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
}
