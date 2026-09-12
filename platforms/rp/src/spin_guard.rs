// SPDX-License-Identifier: GPL-3.0-only
//! The spin ledger: every wait that burns cycles is named, capped or excused.
//!
//! A task that spins — on a status bit, on an atomic another core sets, on
//! a cycle counter — holds its core against everything at or below its
//! priority, and with time slicing off an equal-priority spinner never
//! yields at all (`task_priority.rs`, "One tier for all Java"). The
//! scheduling audit (`docs/scheduling-audit-2026-09.md`) found the spins
//! that mattered in exactly the places a reviewer does not look: port glue,
//! a delay type, an error-recovery path. This scan is the check that those
//! do not come back.
//!
//! What it reads: every `.rs`, `.c` and `.h` under this crate's `src/` and
//! under the shared crate's `drivers/`, `hal/`, `os/` and `install/`
//! (comments blanked, line structure kept). What it rejects, unless excused:
//!
//! 1. **The primitives** — `cortex_m::asm::delay(`, `asm::nop(`,
//!    `core::hint::spin_loop(`, C's `__asm volatile("nop")`. A wait built
//!    from these is a busy-wait by construction.
//! 2. **An empty-body `while`** — `while <register read> {}` in Rust, or a
//!    C body that is only an asm barrier. That is a spin on hardware state
//!    with no bound.
//! 3. **A `loop {}` or `for` whose body is only a nop** — the same thing
//!    spelled differently (a counted `for` of nops is a cycle delay).
//!
//! What excuses one:
//!
//! - Writing it with [`picodroid_core::spin_until!`], which names and caps
//!    the wait and returns a `SpinTimeout` instead of hanging. This is the
//!    normal answer and needs no marker.
//! - A `spin-ok: <why>` comment on the construct's line, on the line above
//!    it, or on the line above a `macro_rules!` whose body it is in. For a
//!    wait that is genuinely microseconds of hardware timing (an ADC
//!    conversion, a 100 ns interrupt-sample delay) or runs before the
//!    scheduler exists.
//! - A `spin-todo: <finding>` comment in the same places, for a spin the
//!    audit ranked and a work package will replace. Debt, written down.
//! - A `spin-ok-file: <why>` comment in a file's first forty lines, for a
//!    file that is spins by nature (the SMP port's spinlocks, one-time PSRAM
//!    bring-up with the scheduler not yet running).
//! - A `while` on `reset_done()`: a block leaving reset is a few bus cycles
//!    and every peripheral init has one; that idiom is recognised outright.
//!
//! Both marker kinds are counted and the counts are pinned below, the way
//! `task_affinity.rs` pins its spawn sites: adding a `spin-ok` or a
//! `spin-todo` means changing a number in this file, which is a review
//! event, and paying a `spin-todo` down means lowering one.

#[cfg(test)]
#[path = "../../../crates/test_support/source_scan.rs"]
mod source_scan;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::source_scan::sources;

    const SELF: &str = "spin_guard.rs";

    /// `spin-ok:` markers across the scanned tree. Each one is a wait that
    /// is documented as genuinely needing to spin; the ledger the test
    /// prints on failure lists them all.
    const EXPECTED_SPIN_OK: usize = 13;
    /// `spin-todo:` markers: audit findings not yet replaced. Only ever
    /// lowered by the work package that replaces one.
    const EXPECTED_SPIN_TODO: usize = 7;

    /// A wait built from one of these is a busy-wait by construction.
    const BANNED_TOKENS: &[&str] = &[
        "cortex_m::asm::delay(",
        "asm::delay(",
        "cortex_m::asm::nop(",
        "asm::nop(",
        "core::hint::spin_loop(",
        "hint::spin_loop(",
        "__asm volatile(\"nop\"",
    ];

    /// Statements that make a loop body a spin rather than work.
    const NOOP_STATEMENTS: &[&str] = &[
        "__asm volatile(\"\" ::: \"memory\");",
        "__asm volatile(\"nop\");",
        "cortex_m::asm::nop();",
        "asm::nop();",
        "core::hint::spin_loop();",
        "hint::spin_loop();",
        "spin_loop();",
    ];

    fn crate_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    }

    fn repo_root() -> PathBuf {
        let root = crate_root();
        root.parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or(root)
    }

    fn rel(path: &Path) -> String {
        super::source_scan::rel(&repo_root(), path)
    }

    /// Comments replaced by spaces, newlines kept, so a line number in the
    /// result is a line number in the file. (The shared `strip_comments`
    /// drops the newlines inside a block comment, which C files are full
    /// of.) String literals are left alone: the tokens above never occur
    /// inside one except in this file, which is skipped.
    fn blank_comments(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
                while i < b.len() && b[i] != b'\n' {
                    out.push(' ');
                    i += 1;
                }
            } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                i += 2;
                out.push_str("  ");
                while i < b.len() && !(b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/') {
                    out.push(if b[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
                if i < b.len() {
                    out.push_str("  ");
                    i += 2;
                }
            } else {
                out.push(b[i] as char);
                i += 1;
            }
        }
        out
    }

    fn is_ident(c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_'
    }

    /// Byte offset of the `{` that opens the construct whose keyword ends
    /// at `from`, or `None` for a `do { } while (…);` tail.
    fn body_open(b: &[u8], from: usize) -> Option<usize> {
        let mut depth = 0i32;
        let mut i = from;
        while i < b.len() {
            match b[i] {
                b'(' | b'[' => depth += 1,
                b')' | b']' => {
                    depth -= 1;
                    if depth < 0 {
                        // A `)` with no `(` of its own: the keyword was
                        // inside some other construct's argument list —
                        // a string literal in an assertion message, say.
                        return None;
                    }
                }
                b'{' if depth == 0 => return Some(i),
                b';' if depth == 0 => return None,
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Whether `offset` sits inside a string literal on its line (an odd
    /// number of unescaped quotes before it).
    fn in_string(blanked: &str, offset: usize) -> bool {
        let line_start = blanked[..offset].rfind('\n').map_or(0, |p| p + 1);
        let b = blanked.as_bytes();
        let mut quotes = 0;
        let mut i = line_start;
        while i < offset {
            if b[i] == b'"' && (i == 0 || b[i - 1] != b'\\') {
                quotes += 1;
            }
            i += 1;
        }
        quotes % 2 == 1
    }

    /// Byte offset one past the `}` matching the `{` at `open`.
    fn body_close(b: &[u8], open: usize) -> usize {
        let mut depth = 0i32;
        let mut i = open;
        while i < b.len() {
            match b[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        b.len()
    }

    /// True when a loop body is nothing but whitespace and no-op statements.
    fn body_is_spin(body: &str) -> bool {
        let mut rest = body.to_string();
        for s in NOOP_STATEMENTS {
            rest = rest.replace(s, "");
        }
        rest.chars().all(|c| c.is_whitespace() || c == ';')
    }

    fn line_of(text: &str, offset: usize) -> usize {
        text[..offset].matches('\n').count()
    }

    /// One flagged construct: the 0-based lines it spans and a label. A
    /// primitive is one line; a loop runs to its closing brace, so an
    /// excuse on the loop covers the `nop` inside it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Hit {
        line: usize,
        end: usize,
        what: &'static str,
    }

    /// Every spin-shaped construct in comment-blanked source text.
    fn find_spins(blanked: &str) -> Vec<Hit> {
        let b = blanked.as_bytes();
        let mut hits = Vec::new();
        for (line, text) in blanked.lines().enumerate() {
            if BANNED_TOKENS.iter().any(|t| text.contains(t)) {
                hits.push(Hit {
                    line,
                    end: line,
                    what: "busy-wait primitive",
                });
            }
        }
        for (kw, what) in [
            ("while", "empty-body while"),
            ("loop", "nop-only loop"),
            ("for", "nop-only for"),
        ] {
            let mut at = 0;
            while let Some(pos) = blanked[at..].find(kw) {
                let start = at + pos;
                let end = start + kw.len();
                at = end;
                let bounded = (start == 0 || !is_ident(b[start - 1]))
                    && (end >= b.len() || !is_ident(b[end]));
                if !bounded || in_string(blanked, start) {
                    continue;
                }
                let Some(open) = body_open(b, end) else {
                    continue;
                };
                let close = body_close(b, open);
                let cond = &blanked[end..open];
                if kw == "while" && cond.contains("reset_done()") {
                    continue;
                }
                // `for` is a loop only as `for x in y {` or C's `for (…) {`;
                // `impl Sync for T {}` is the same keyword and an empty body.
                if kw == "for" && !(cond.contains(" in ") || cond.trim_start().starts_with('(')) {
                    continue;
                }
                if !body_is_spin(&blanked[open + 1..close - 1]) {
                    continue;
                }
                let line = line_of(blanked, start);
                // `bkpt(); loop {}` is a fault handler's full stop, not a wait.
                if kw == "loop" && previous_code_line(blanked, line).contains("bkpt()") {
                    continue;
                }
                hits.push(Hit {
                    line,
                    end: line_of(blanked, close - 1),
                    what,
                });
            }
        }
        // A primitive inside a flagged loop is that loop's problem, not a
        // second finding; an excuse on the loop then covers it.
        let loops: Vec<(usize, usize)> = hits
            .iter()
            .filter(|h| h.end > h.line)
            .map(|h| (h.line, h.end))
            .collect();
        hits.retain(|h| h.end > h.line || !loops.iter().any(|&(s, e)| h.line > s && h.line <= e));
        hits.sort_by_key(|h| h.line);
        hits.dedup();
        hits
    }

    /// The nearest line above `line` in comment-blanked text that is
    /// neither blank nor an attribute (`#[allow(clippy::empty_loop)]` sits
    /// between a fault handler's `bkpt()` and its `loop {}`).
    fn previous_code_line(blanked: &str, line: usize) -> &str {
        let lines: Vec<&str> = blanked.lines().collect();
        lines[..line.min(lines.len())]
            .iter()
            .rev()
            .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("#["))
            .copied()
            .unwrap_or("")
    }

    fn has_marker(line: &str) -> bool {
        line.contains("spin-ok:") || line.contains("spin-todo:")
    }

    fn is_comment_line(line: &str) -> bool {
        let t = line.trim_start();
        t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
    }

    /// Whether the comment block directly above `line` (contiguous comment
    /// lines, no blank line between them and `line`) carries a marker.
    fn marker_above(raw_lines: &[&str], line: usize) -> bool {
        let mut i = line;
        while i > 0 {
            i -= 1;
            let l = raw_lines[i];
            if l.trim().is_empty() {
                return false;
            }
            if has_marker(l) {
                return true;
            }
            if !is_comment_line(l) {
                return false;
            }
        }
        false
    }

    /// Whether the construct starting on `line` (0-based) is excused by a
    /// marker on its line, in the comment block above it, or in the one
    /// above the header of the `macro_rules!` it sits in.
    fn excused(raw_lines: &[&str], line: usize) -> bool {
        if has_marker(raw_lines[line]) || marker_above(raw_lines, line) {
            return true;
        }
        // Inside a macro: walk up to the nearest `macro_rules!` header that
        // has not been closed, and accept a marker on it or above it.
        let mut depth = 0i32;
        let mut i = line + 1;
        while i > 0 {
            i -= 1;
            let l = raw_lines[i];
            depth += l.matches('}').count() as i32 - l.matches('{').count() as i32;
            if l.trim_start().starts_with("macro_rules!") && depth < 0 {
                return has_marker(l) || marker_above(raw_lines, i);
            }
        }
        false
    }

    fn file_is_excused(raw: &str) -> bool {
        raw.lines().take(40).any(|l| l.contains("spin-ok-file:"))
    }

    fn scanned_files() -> Vec<PathBuf> {
        let mut files = Vec::new();
        sources(
            &crate_root().join("src"),
            &["rs", "c", "h"],
            Some(SELF),
            &mut files,
        );
        let core = repo_root().join("crates/picodroid-core/src");
        for dir in ["drivers", "hal", "os", "install"] {
            sources(&core.join(dir), &["rs", "c", "h"], None, &mut files);
        }
        files.retain(|p| {
            let s = p.to_string_lossy();
            // The simulator is host code; hal/spin.rs defines the macro.
            !s.contains("/hal/sim/") && !s.ends_with("/hal/spin.rs")
        });
        files.sort();
        files
    }

    #[test]
    fn every_spin_is_named_capped_or_excused() {
        let files = scanned_files();
        for must in [
            "hal/rp/dma.rs",
            "hal/rp/pio_spi.rs",
            "port/net/cyw43_port.c",
            "drivers/st7789.rs",
        ] {
            assert!(
                files.iter().any(|p| p.to_string_lossy().ends_with(must)),
                "scanner did not find {must} — the layout changed, not the code"
            );
        }
        let mut violations = Vec::new();
        let mut ok = Vec::new();
        let mut todo = Vec::new();
        for file in &files {
            let raw = std::fs::read_to_string(file).unwrap();
            let raw_lines: Vec<&str> = raw.lines().collect();
            for (i, l) in raw_lines.iter().enumerate() {
                if l.contains("spin-ok:") {
                    ok.push(format!("{}:{}", rel(file), i + 1));
                }
                if l.contains("spin-todo:") {
                    todo.push(format!("{}:{}", rel(file), i + 1));
                }
            }
            if file_is_excused(&raw) {
                ok.push(format!("{} (whole file)", rel(file)));
                continue;
            }
            let blanked = blank_comments(&raw);
            for hit in find_spins(&blanked) {
                if !excused(&raw_lines, hit.line) {
                    violations.push(format!(
                        "{}:{}: {} — {}",
                        rel(file),
                        hit.line + 1,
                        hit.what,
                        raw_lines[hit.line].trim()
                    ));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "unexcused busy-waits. Wait on the kernel (a semaphore an interrupt \
             gives, a task notification, delay_ms); for a hardware wait that must \
             stay a spin use `picodroid_core::spin_until!` so it is named and \
             capped; a genuine microsecond timing loop gets a `// spin-ok: <why>` \
             on it or the line above (and a bump of EXPECTED_SPIN_OK here). \
             docs/scheduling-audit-2026-09.md has the rules. Found:\n  {}",
            violations.join("\n  ")
        );
        assert_eq!(
            ok.len(),
            EXPECTED_SPIN_OK,
            "the number of `spin-ok` excuses changed. Each is a wait that is \
             documented as genuinely needing to spin; update EXPECTED_SPIN_OK \
             only after reading the new one. Ledger:\n  {}",
            ok.join("\n  ")
        );
        assert_eq!(
            todo.len(),
            EXPECTED_SPIN_TODO,
            "the number of `spin-todo` entries changed. New debt needs a finding \
             in docs/scheduling-audit-2026-09.md; paid-down debt lowers \
             EXPECTED_SPIN_TODO. Ledger:\n  {}",
            todo.join("\n  ")
        );
    }

    #[test]
    fn the_scan_knows_a_spin_from_a_loop() {
        let spins = [
            "while p.DMA.ch(0).busy().bit_is_set() {}",
            "while p\n    .DMA\n    .busy()\n    .bit_is_set()\n{}",
            "loop {\n    cortex_m::asm::nop();\n}",
            "while (!(sio_hw->fifo_st & SIO_FIFO_ST_RDY)) {\n    __asm volatile(\"\" ::: \"memory\");\n}",
            "while (*lock == 0u) { __asm volatile(\"\" ::: \"memory\"); }",
            "for _ in 0..16 {\n    core::hint::spin_loop();\n}",
            "cortex_m::asm::delay(ns / 7);",
        ];
        for s in spins {
            assert!(!find_spins(&blank_comments(s)).is_empty(), "missed: {s}");
        }
        let fine = [
            "while x {\n    work();\n}",
            "while (x) { y++; }",
            "loop {\n    if done() { break; }\n    core::hint::spin_loop_hint_is_not_this();\n}",
            "while p.RESETS.reset_done().read().dma().bit_is_clear() {}",
            "do {\n    lo = read();\n} while (hi != read_hi());",
            "assert!(x, \"a Thread.start/join loop then leaks every child\", rel(p));",
            "asm::bkpt();\nloop {}",
            "asm::bkpt();\n#[allow(clippy::empty_loop)]\nloop {}",
            "// while busy {}\n/* loop { nop(); } */",
            "spin_until!(!p.busy(), 1_000_000, \"dma abort\")",
            "unsafe impl Sync for SemCell {}",
        ];
        for s in fine {
            assert!(
                find_spins(&blank_comments(s)).is_empty(),
                "false positive: {s}"
            );
        }
    }

    #[test]
    fn markers_excuse_their_construct_and_macro_bodies() {
        let raw = "// spin-ok: two microseconds\nwhile p.ready().bit_is_clear() {}\n\nwhile p.other().bit_is_clear() {}\n";
        let lines: Vec<&str> = raw.lines().collect();
        assert!(excused(&lines, 1));
        assert!(!excused(&lines, 3));
        // A marker anywhere in the comment block directly above counts; a
        // blank line ends the block.
        let two = "// spin-todo: F3 — the reason\n// continues here\nwhile !flag.load() {\n    nop();\n}\n";
        let lines: Vec<&str> = two.lines().collect();
        assert!(excused(&lines, 2));
        let gap = "// spin-ok: stale\n\nwhile x {}\n";
        let lines: Vec<&str> = gap.lines().collect();
        assert!(!excused(&lines, 2));
        // A counted loop of nops is a cycle delay; the marker above `for`
        // covers the primitive inside it.
        let f = "// spin-ok: ~100 ns settle\nfor _ in 0..16 {\n    core::hint::spin_loop();\n}\n";
        let hits = find_spins(&blank_comments(f));
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(excused(&f.lines().collect::<Vec<_>>(), hits[0].line));
        let m = "// spin-ok: FIFO-depth polls\nmacro_rules! poll {\n    ($s:expr) => {{\n        while $s.tnf().bit_is_clear() {}\n    }};\n}\n";
        let lines: Vec<&str> = m.lines().collect();
        assert!(excused(&lines, 3));
        // The nop inside a flagged loop is folded into the loop's finding.
        let l = "loop {\n    cortex_m::asm::nop();\n}\n";
        let hits = find_spins(&blank_comments(l));
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!((hits[0].line, hits[0].end), (0, 2));
        assert!(file_is_excused(
            "//! spin-ok-file: pre-scheduler bring-up\n"
        ));
        assert!(!file_is_excused("//! nothing here\n"));
    }
}
