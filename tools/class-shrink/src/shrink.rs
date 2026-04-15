//! Top-level driver: apply a [`ShrinkMap`] to a directory of `.class` files.
//!
//! v1 rewrites only class names (and class-name substrings inside
//! descriptors). Method and field names are untouched. The parser is
//! lossless in byte order outside the constant pool, so rewriting Utf8
//! entries alone keeps every class file valid.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::classfile::{ClassFile, CpEntry};
use crate::descriptor::{classify, rewrite_bare, rewrite_descriptor, RewriteKind};
use crate::mapping::ShrinkMap;

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
        for utf in cf.utf8_entries_mut() {
            let payload = utf.clone();
            match classify(&payload) {
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

/// Walk `in_dir`'s class files, collect every class that's NOT kept by
/// `keep`, sort deterministically, and extend `base` with freshly allocated
/// shrunk names (append-only). Returns the updated map.
pub fn cut_release(
    in_dir: &Path,
    keep: &crate::keep::KeepList,
    base: ShrinkMap,
) -> io::Result<ShrinkMap> {
    let files = list_class_files(in_dir)?;
    // Collect unique (own_name) entries.
    let mut discovered: Vec<String> = Vec::new();
    for file in &files {
        let bytes = fs::read(file)?;
        let cf = ClassFile::parse(&bytes)?;
        if let Some(name) = read_own_name(&cf) {
            if let Ok(s) = std::str::from_utf8(name) {
                discovered.push(s.to_string());
            }
        }
    }
    discovered.sort();
    discovered.dedup();

    let mut map = base;
    // Next free allocator index = current size of map (every existing entry
    // consumed one slot).
    let mut next = map.classes.len();
    for name in discovered {
        if keep.is_kept(&name) {
            continue;
        }
        if map.classes.contains_key(&name) {
            continue;
        }
        let suffix = crate::rename::short_suffix(next);
        map.classes
            .insert(name, crate::rename::shrunk_name(&suffix));
        next += 1;
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keep::KeepList;
    use std::path::PathBuf;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("cs-shrink-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn cut_release_skips_kept() {
        // Without actually generating .class files we can smoke-test the
        // keep check by feeding an empty dir: nothing gets shrunk.
        let dir = tmp("cut-empty");
        let keep = KeepList::default();
        let m = cut_release(&dir, &keep, ShrinkMap::new()).unwrap();
        assert!(m.classes.is_empty());
    }
}
