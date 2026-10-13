// SPDX-License-Identifier: GPL-3.0-only
//! The hashes of the classfile-less builtin classes, sorted, for the
//! cross-set collision check an app load runs (`Jvm::load_app`): an app
//! class whose name hashes like a builtin would be dispatched as one.

use super::BUILTIN_CLASS_NAMES;
use crate::class_file::name_hash;

const N: usize = BUILTIN_CLASS_NAMES.len();

/// `name_hash` of every [`BUILTIN_CLASS_NAMES`] entry, ascending, so a
/// membership test is one binary search (`class_link::hash_in_sorted`).
pub static BUILTIN_CLASS_HASHES: [u32; N] = sorted_hashes();

/// The hashes, insertion-sorted: the only sort a `const fn` can run, and
/// `N` is small.
const fn sorted_hashes() -> [u32; N] {
    let mut out = [0u32; N];
    let mut i = 0;
    while i < N {
        let h = name_hash(BUILTIN_CLASS_NAMES[i].as_bytes());
        let mut j = i;
        while j > 0 && out[j - 1] > h {
            out[j] = out[j - 1];
            j -= 1;
        }
        out[j] = h;
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_without_duplicates_and_complete() {
        for pair in BUILTIN_CLASS_HASHES.windows(2) {
            assert!(
                pair[0] < pair[1],
                "unsorted or duplicate hash {:#x}",
                pair[0]
            );
        }
        for name in BUILTIN_CLASS_NAMES {
            assert!(
                BUILTIN_CLASS_HASHES
                    .binary_search(&name_hash(name.as_bytes()))
                    .is_ok(),
                "{name} missing"
            );
        }
        assert_eq!(BUILTIN_CLASS_HASHES.len(), BUILTIN_CLASS_NAMES.len());
    }
}
