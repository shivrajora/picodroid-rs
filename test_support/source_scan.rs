// SPDX-License-Identifier: GPL-3.0-only
//! Source-scan helpers shared by every text-based guard in the workspace.
//!
//! `#[path]`-included — like `gc_root_scan.rs` beside it — by
//! `platforms/rp/src/task_affinity.rs` (the core-placement rules),
//! `picodroid-core/src/rtos/mod.rs` (the seam guard) and
//! `picodroid-core/src/porting.rs` (the checklist guard), so the walker and
//! the comment stripper exist once. Each includer uses a subset, hence the
//! allow below.
//!
//! Why guards read *text*: what they enforce lives on both sides of a `cfg`
//! (`cfg(not(test))` device code, `cfg(test)` scans), so under `cargo test`
//! there is nothing to call. The source is the one thing both configurations
//! share.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Every file with one of `exts` under `dir`, recursively, except one named
/// `skip_file` — the guard's own file, which quotes the tokens it looks for
/// in string literals no comment stripper removes.
pub fn sources(dir: &Path, exts: &[&str], skip_file: Option<&str>, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, exts, skip_file, out);
        } else if skip_file.is_none_or(|s| path.file_name().is_some_and(|n| n != s))
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.contains(&e))
        {
            out.push(path);
        }
    }
}

/// Source with comments removed — `//` to end of line and `/* … */`
/// blocks — so a comment that mentions a banned token, or quotes a call, can
/// neither trip a rule nor satisfy one. Newlines are kept: line structure
/// matters to the `#define` checks.
pub fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let (at, block) = match (rest.find("//"), rest.find("/*")) {
            (None, None) => {
                out.push_str(rest);
                return out;
            }
            (Some(l), None) => (l, false),
            (None, Some(b)) => (b, true),
            (Some(l), Some(b)) => {
                if l < b {
                    (l, false)
                } else {
                    (b, true)
                }
            }
        };
        out.push_str(&rest[..at]);
        rest = if block {
            match rest[at + 2..].find("*/") {
                Some(n) => &rest[at + 2 + n + 2..],
                None => "",
            }
        } else {
            match rest[at..].find('\n') {
                Some(n) => &rest[at + n..],
                None => "",
            }
        };
    }
}

pub fn read_stripped(path: &Path) -> String {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    strip_comments(&text)
}

/// `path` relative to `root`, for messages.
pub fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
