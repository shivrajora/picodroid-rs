//! Rewrites class-name references inside Utf8 constant-pool entries.
//!
//! Class names appear in three shapes per JVMS §4.3 and §4.4:
//!
//! 1. **Bare internal name** — the payload of a `CONSTANT_Class_info`'s
//!    name_index Utf8 entry, e.g. `picodroid/os/Activity`. No delimiter.
//! 2. **Object type in a descriptor** — `Lfoo/Bar;` inside a field descriptor
//!    (`Lfoo/Bar;`), method descriptor (`(Lfoo/Bar;I)V`), or array type
//!    (`[[Lfoo/Bar;`). Always delimited by `L` and `;`.
//! 3. **Generic signature** — same `Lfoo/Bar;` shape with `<>` wrappers for
//!    type parameters. Signatures are optional; we strip them elsewhere
//!    rather than rewrite.
//!
//! This module handles shapes (1) and (2). Callers decide which Utf8 entries
//! are bare internal names vs. descriptors (by their syntactic form — bare
//! names never contain `(`, `)`, or `;`).

use std::collections::HashMap;

/// Result of attempting to rewrite a Utf8 payload.
pub enum RewriteKind {
    /// Bare internal class name (no `(`, `)`, `;`, or `[`).
    BareName,
    /// Looks like a descriptor — contains `(` / `)` / `;` / `[` / `L`.
    Descriptor,
    /// Neither — leave as-is (e.g. method name, source-file name).
    Other,
}

/// Classify a Utf8 payload. Cheap heuristic tailored to class files.
pub fn classify(bytes: &[u8]) -> RewriteKind {
    // A bare internal class name contains only identifier characters,
    // `/` separators, and optionally `$` for inner classes. No parens,
    // no semicolons, no brackets.
    let has_struct = bytes
        .iter()
        .any(|&b| matches!(b, b'(' | b')' | b';' | b'['));
    if has_struct {
        return RewriteKind::Descriptor;
    }
    // Further refinement: distinguish "method name", "attribute name",
    // "source file" etc. from bare class names. The simplest proxy:
    // a bare class name has at least one `/`, or matches a known class
    // in the rename map. We let the caller check the map; here just say
    // "could be a bare name".
    if bytes.contains(&b'/') {
        RewriteKind::BareName
    } else {
        RewriteKind::Other
    }
}

/// Rewrite every `Lfoo/Bar;` substring in `src` using `class_map`, returning
/// the rewritten bytes. Allocates a fresh Vec — small and infrequent enough
/// not to matter.
pub fn rewrite_descriptor(src: &[u8], class_map: &HashMap<Vec<u8>, Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'L' {
            // Scan forward to the terminating `;`.
            if let Some(end) = src[i + 1..].iter().position(|&b| b == b';') {
                let name_start = i + 1;
                let name_end = i + 1 + end;
                let name = &src[name_start..name_end];
                if let Some(new) = class_map.get(name) {
                    out.push(b'L');
                    out.extend_from_slice(new);
                    out.push(b';');
                    i = name_end + 1;
                    continue;
                }
                // Unknown class; copy through unchanged.
                out.extend_from_slice(&src[i..=name_end]);
                i = name_end + 1;
                continue;
            }
        }
        out.push(src[i]);
        i += 1;
    }
    out
}

/// Rewrite a bare internal class name via `class_map` (returns Some new name)
/// or returns None if the name is not in the map.
pub fn rewrite_bare(src: &[u8], class_map: &HashMap<Vec<u8>, Vec<u8>>) -> Option<Vec<u8>> {
    class_map.get(src).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&[u8], &[u8])]) -> HashMap<Vec<u8>, Vec<u8>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
            .collect()
    }

    #[test]
    fn descriptor_single_ref() {
        let m = map(&[(b"foo/Bar", b"a/A")]);
        assert_eq!(rewrite_descriptor(b"Lfoo/Bar;", &m), b"La/A;".to_vec());
    }

    #[test]
    fn descriptor_method_signature() {
        let m = map(&[(b"foo/Bar", b"a/A"), (b"x/Y", b"a/B")]);
        assert_eq!(
            rewrite_descriptor(b"(Lfoo/Bar;I)Lx/Y;", &m),
            b"(La/A;I)La/B;".to_vec()
        );
    }

    #[test]
    fn descriptor_array_type() {
        let m = map(&[(b"foo/Bar", b"a/A")]);
        assert_eq!(rewrite_descriptor(b"[[Lfoo/Bar;", &m), b"[[La/A;".to_vec());
    }

    #[test]
    fn descriptor_leaves_primitives_alone() {
        let m = map(&[]);
        assert_eq!(rewrite_descriptor(b"(II)J", &m), b"(II)J".to_vec());
    }

    #[test]
    fn descriptor_unknown_class_is_preserved() {
        let m = map(&[(b"foo/Bar", b"a/A")]);
        assert_eq!(
            rewrite_descriptor(b"Lother/Thing;", &m),
            b"Lother/Thing;".to_vec()
        );
    }

    #[test]
    fn classify_detects_descriptors() {
        assert!(matches!(classify(b"()V"), RewriteKind::Descriptor));
        assert!(matches!(classify(b"Lfoo/Bar;"), RewriteKind::Descriptor));
        assert!(matches!(classify(b"[I"), RewriteKind::Descriptor));
        assert!(matches!(classify(b"foo/Bar"), RewriteKind::BareName));
        assert!(matches!(classify(b"onCreate"), RewriteKind::Other));
    }
}
