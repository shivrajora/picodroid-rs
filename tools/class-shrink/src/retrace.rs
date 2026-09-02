// SPDX-License-Identifier: GPL-3.0-only
//! Host-side inverse of a shrink map for log text — the picodroid
//! equivalent of ProGuard's `retrace`.
//!
//! A `--shrink` firmware prints mapped names everywhere: `Class.getName()`,
//! uncaught-exception banners, `pdb` output, `Log` lines that embed a class
//! name. Both halves of a map are bijections (`a/XX` / `b/XX` for classes,
//! by-name targets for members), so reading such a log is a token
//! substitution: a class token is any `a/XX`, `a.XX`, `b/XX` or `b.XX` run
//! bounded by non-identifier characters; a member token is an identifier
//! that is a map target and sits in `.name(` / `.name` position or at the
//! end of a `Class.method` pair. Everything else passes through.

use crate::mapping::ShrinkMap;
use std::collections::HashMap;

/// A loaded map, indexed for reverse lookup.
pub struct Retracer {
    /// `a/XX` → original, plus the dotted spelling.
    classes: HashMap<String, String>,
    /// member target → original.
    members: HashMap<String, String>,
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
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
        Self { classes, members }
    }

    /// Retrace one line of text.
    pub fn line(&self, line: &str) -> String {
        let b = line.as_bytes();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        while i < b.len() {
            // Class token: `[ab][/.][A-Z]+`, not preceded by an identifier byte.
            if (b[i] == b'a' || b[i] == b'b')
                && i + 2 < b.len()
                && (b[i + 1] == b'/' || b[i + 1] == b'.')
                && b[i + 2].is_ascii_uppercase()
                && (i == 0 || !is_ident(b[i - 1]))
            {
                let mut j = i + 2;
                while j < b.len() && b[j].is_ascii_uppercase() {
                    j += 1;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> ShrinkMap {
        let mut m = ShrinkMap::new();
        m.classes
            .insert("picodroid/view/View".into(), "a/AB".into());
        m.classes
            .insert("java/lang/NullPointerException".into(), "b/AK".into());
        m.members.insert("setText".into(), "eL".into());
        m.members.insert("toString".into(), "xy".into());
        m
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
}
