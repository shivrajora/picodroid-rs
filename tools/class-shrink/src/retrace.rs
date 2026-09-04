// SPDX-License-Identifier: GPL-3.0-only
//! Host-side inverse of a shrink map for log text — the picodroid
//! equivalent of ProGuard's `retrace`.
//!
//! A `--shrink` firmware prints mapped names everywhere: `Class.getName()`,
//! uncaught-exception banners, `pdb` output, `Log` lines that embed a class
//! name. Both halves of a map are bijections (`a/XX` / `b/XX` for classes,
//! by-name targets for members), so reading such a log is a token
//! substitution: a class token is any `a/XX`, `a.XX`, `b/XX`, `b.XX`,
//! `c/XX` or `c.XX` run (optionally with the `_MembersInjector` tail an
//! app-shrunk map gives injector classes) bounded by non-identifier
//! characters; a member token is an identifier
//! that is a map target and sits in `.name(` / `.name` position or at the
//! end of a `Class.method` pair. Everything else passes through.
//!
//! A second pass resolves stack-trace frames. Release firmware carries no
//! `LineNumberTable` and prints `at pkg.Class.method(pc=9)`; given the
//! unstripped host trees the same classes were compiled into
//! ([`Retracer::load_classes`]), that frame becomes
//! `at pkg.Class.method(Class.java:39)` — the spelling the sim and
//! debug-profile firmware print themselves. Names are un-shrunk first, so
//! the lookup always sees original names. A frame is left as it was when
//! nothing resolves; several same-named overloads that disagree print all
//! candidates (`Class.java:12|40`), as ProGuard does, because the frame
//! carries no descriptor.

use crate::classfile::{ClassFile, LineInfo};
use crate::mapping::ShrinkMap;
use std::collections::HashMap;
use std::io;
use std::path::Path;

/// A loaded map, indexed for reverse lookup.
pub struct Retracer {
    /// `a/XX` → original, plus the dotted spelling.
    classes: HashMap<String, String>,
    /// member target → original.
    members: HashMap<String, String>,
    /// Internal class name → its source positions, from host class trees.
    lines: HashMap<String, LineInfo>,
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Bytes a `pkg.Class.method` frame token is made of (`<init>` included).
fn is_frame_byte(b: u8) -> bool {
    is_ident(b) || matches!(b, b'.' | b'/' | b'<' | b'>')
}

impl Retracer {
    pub fn new(map: &ShrinkMap) -> Self {
        let mut classes = HashMap::new();
        for (from, to) in map.iter_classes() {
            classes.insert(to.to_string(), from.to_string());
            classes.insert(to.replace('/', "."), from.replace('/', "."));
        }
        let members = map
            .iter_members()
            .map(|(from, to)| (to.to_string(), from.to_string()))
            .collect();
        Self {
            classes,
            members,
            lines: HashMap::new(),
        }
    }

    /// Index every `.class` under `dir` (an unstripped tree in original
    /// names: `sdk/build/classes/java/main`, an app's `build/classes`) for
    /// `pc=` resolution. Returns how many classes were read. The first tree
    /// to name a class wins, so passing the SDK and an app tree in either
    /// order is fine.
    pub fn load_classes(&mut self, dir: &Path) -> io::Result<usize> {
        let mut n = 0;
        for path in crate::shrink::list_class_files(dir)? {
            let bytes = std::fs::read(&path)?;
            let cf = ClassFile::parse(&bytes)?;
            let Some(name) = crate::shrink::read_own_name(&cf) else {
                continue;
            };
            let name = String::from_utf8_lossy(name).into_owned();
            let info = cf.line_info()?;
            self.lines.entry(name).or_insert(info);
            n += 1;
        }
        Ok(n)
    }

    /// Register one class's positions directly (tests, or a caller that
    /// already parsed the tree).
    pub fn add_class(&mut self, internal_name: &str, info: LineInfo) {
        self.lines.insert(internal_name.to_string(), info);
    }

    /// Retrace one line of text: un-shrink names, then resolve `(pc=N)`.
    pub fn line(&self, line: &str) -> String {
        let unshrunk = self.unshrink(line);
        if self.lines.is_empty() {
            return unshrunk;
        }
        self.resolve_pcs(&unshrunk)
    }

    fn unshrink(&self, line: &str) -> String {
        let b = line.as_bytes();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        while i < b.len() {
            // Class token: `[abc][/.][A-Z]+(_MembersInjector)?`, not preceded
            // by an identifier byte.
            if matches!(b[i], b'a' | b'b' | b'c')
                && i + 2 < b.len()
                && (b[i + 1] == b'/' || b[i + 1] == b'.')
                && b[i + 2].is_ascii_uppercase()
                && (i == 0 || !is_ident(b[i - 1]))
            {
                let mut j = i + 2;
                while j < b.len() && b[j].is_ascii_uppercase() {
                    j += 1;
                }
                const INJECTOR_TAIL: &[u8] = b"_MembersInjector";
                if b[j..].starts_with(INJECTOR_TAIL) {
                    j += INJECTOR_TAIL.len();
                }
                if j >= b.len() || !is_ident(b[j]) {
                    if let Some(orig) = self.classes.get(&line[i..j]) {
                        out.push_str(orig);
                        i = j;
                        continue;
                    }
                }
            }
            // Member token: identifier after a `.` (or `#`), followed by `(`,
            // `:`, `)`, whitespace or end of line.
            if is_ident(b[i]) && i > 0 && (b[i - 1] == b'.' || b[i - 1] == b'#') {
                let mut j = i;
                while j < b.len() && is_ident(b[j]) {
                    j += 1;
                }
                let boundary = j >= b.len() || matches!(b[j], b'(' | b':' | b')' | b' ' | b',');
                if boundary {
                    if let Some(orig) = self.members.get(&line[i..j]) {
                        out.push_str(orig);
                        i = j;
                        continue;
                    }
                }
                out.push_str(&line[i..j]);
                i = j;
                continue;
            }
            // Advance one UTF-8 scalar.
            let ch = line[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }

    /// Replace every `Class.method(pc=N)` whose class and method are known
    /// with `Class.method(File.java:L)`; anything else stays verbatim.
    fn resolve_pcs(&self, line: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut rest = line;
        while let Some(k) = rest.find("(pc=") {
            let head = &rest[..k];
            let after = &rest[k + 4..];
            let digits = after
                .bytes()
                .position(|b| !b.is_ascii_digit())
                .unwrap_or(after.len());
            let pc: Option<u16> = if digits > 0 && after[digits..].starts_with(')') {
                after[..digits].parse().ok()
            } else {
                None
            };
            let token_start = head
                .char_indices()
                .rev()
                .find(|&(_, c)| !(c.is_ascii() && is_frame_byte(c as u8)))
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0);
            let resolved = pc.and_then(|pc| self.frame_position(&head[token_start..], pc));
            out.push_str(head);
            match resolved {
                Some(pos) => {
                    out.push('(');
                    out.push_str(&pos);
                    out.push(')');
                    rest = &after[digits + 1..];
                }
                None => {
                    out.push_str("(pc=");
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// `File.java:L` (or `Unknown Source:L`, or `File.java:L1|L2` when
    /// same-named overloads disagree) for a `pkg.Class.method` token.
    fn frame_position(&self, token: &str, pc: u16) -> Option<String> {
        let dot = token.rfind('.')?;
        let (class, method) = (&token[..dot], &token[dot + 1..]);
        let info = self.lines.get(&class.replace('.', "/"))?;
        let mut lines: Vec<u16> = info
            .methods
            .iter()
            .filter(|m| m.name == method.as_bytes())
            .filter_map(|m| m.line_at(pc))
            .collect();
        lines.sort_unstable();
        lines.dedup();
        if lines.is_empty() {
            return None;
        }
        let file = info
            .source_file
            .as_deref()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_else(|| "Unknown Source".to_string());
        let joined = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("|");
        Some(format!("{file}:{joined}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classfile::MethodLines;

    fn map() -> ShrinkMap {
        let mut m = ShrinkMap::new();
        m.classes
            .insert("picodroid/view/View".into(), "a/AB".into());
        m.classes
            .insert("java/lang/NullPointerException".into(), "b/AK".into());
        m.members.insert("setText".into(), "eL".into());
        m.members.insert("toString".into(), "xy".into());
        m.classes.insert("app/Main".into(), "c/A".into());
        m.classes.insert(
            "app/Main_MembersInjector".into(),
            "c/A_MembersInjector".into(),
        );
        m.members.insert("formatLux".into(), "qZ".into());
        m
    }

    fn method(name: &str, entries: &[(u16, u16)]) -> MethodLines {
        MethodLines {
            name: name.as_bytes().to_vec(),
            descriptor: b"()V".to_vec(),
            entries: entries.to_vec(),
        }
    }

    /// The map plus host line tables for View (View.java) and Main (no
    /// SourceFile), with an overloaded `formatLux`.
    fn retracer_with_lines() -> Retracer {
        let mut r = Retracer::new(&map());
        r.add_class(
            "picodroid/view/View",
            LineInfo {
                source_file: Some(b"View.java".to_vec()),
                methods: vec![method("setText", &[(0, 120), (3, 121), (9, 125)])],
            },
        );
        r.add_class(
            "app/Main",
            LineInfo {
                source_file: None,
                methods: vec![
                    method("formatLux", &[(0, 12)]),
                    method("formatLux", &[(0, 40), (3, 41)]),
                    method("<init>", &[(0, 7)]),
                ],
            },
        );
        r
    }

    #[test]
    fn app_classes_and_injectors_retrace() {
        let r = Retracer::new(&map());
        assert_eq!(r.line("at c/A.qZ(pc=3)"), "at app/Main.formatLux(pc=3)");
        assert_eq!(r.line("push c.A"), "push app.Main");
        assert_eq!(
            r.line("injectMembers failed: c/A_MembersInjector"),
            "injectMembers failed: app/Main_MembersInjector"
        );
        assert_eq!(r.line("c/AB_Foo stays"), "c/AB_Foo stays");
    }

    #[test]
    fn classes_in_both_spellings() {
        let r = Retracer::new(&map());
        assert_eq!(
            r.line("at a/AB.eL(pc=3)"),
            "at picodroid/view/View.setText(pc=3)"
        );
        assert_eq!(
            r.line("Uncaught b.AK: boom"),
            "Uncaught java.lang.NullPointerException: boom"
        );
    }

    #[test]
    fn unrelated_text_passes_through() {
        let r = Retracer::new(&map());
        assert_eq!(
            r.line("a/ZZ is not mapped; nab/AB neither"),
            "a/ZZ is not mapped; nab/AB neither"
        );
        assert_eq!(
            r.line("xy alone is not a member"),
            "xy alone is not a member"
        );
        assert_eq!(r.line("obj.xy()"), "obj.toString()");
    }

    #[test]
    fn resolves_pc_after_unshrinking() {
        let r = retracer_with_lines();
        // Shrunk frame, dotted class as the JVM prints it.
        assert_eq!(
            r.line("    at a.AB.eL(pc=4)"),
            "    at picodroid.view.View.setText(View.java:121)"
        );
        // Already-original frame with a slash class name.
        assert_eq!(
            r.line("at picodroid/view/View.setText(pc=9)"),
            "at picodroid/view/View.setText(View.java:125)"
        );
        // No SourceFile → Android's Unknown Source; <init> is a valid token.
        assert_eq!(
            r.line("at c.A.<init>(pc=0)"),
            "at app.Main.<init>(Unknown Source:7)"
        );
    }

    #[test]
    fn overloads_that_disagree_print_every_candidate() {
        let r = retracer_with_lines();
        assert_eq!(
            r.line("at c.A.qZ(pc=3)"),
            "at app.Main.formatLux(Unknown Source:12|41)"
        );
    }

    #[test]
    fn unresolvable_frames_stay_verbatim() {
        let r = retracer_with_lines();
        assert_eq!(r.line("at other.Cls.run(pc=3)"), "at other.Cls.run(pc=3)");
        assert_eq!(
            r.line("at app.Main.missing(pc=3)"),
            "at app.Main.missing(pc=3)"
        );
        assert_eq!(r.line("weird (pc=x) text"), "weird (pc=x) text");
        assert_eq!(r.line("(pc=3) alone"), "(pc=3) alone");
    }
}
