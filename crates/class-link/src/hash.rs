// SPDX-License-Identifier: GPL-3.0-only
//! The one hash every table and every lookup uses: 32-bit FNV-1a.
//!
//! It is the hash `pico_jvm::class_file::name_hash` has always computed
//! over class names, so a runtime that hashes its own (shrunk) constant
//! finds the same value the packer stored. `const fn`, so a name known at
//! compile time hashes to a literal.

/// FNV-1a 32-bit offset basis.
pub const FNV_OFFSET: u32 = 0x811c_9dc5;
/// FNV-1a 32-bit prime.
pub const FNV_PRIME: u32 = 0x0100_0193;

/// Continue an FNV-1a hash `h` over `bytes`.
#[inline]
pub const fn fnv1a_continue(mut h: u32, bytes: &[u8]) -> u32 {
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u32;
        h = h.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    h
}

/// Hash of a class, member or attribute name, exactly as spelled.
#[inline]
pub const fn name_hash(name: &[u8]) -> u32 {
    fnv1a_continue(FNV_OFFSET, name)
}

/// Hash of a method signature: the name followed by the descriptor, as one
/// stream (`sig_hash(n, d) == name_hash(n ++ d)`). What a method table row
/// stores and what a lookup by `(name, descriptor)` computes.
#[inline]
pub const fn sig_hash(name: &[u8], descriptor: &[u8]) -> u32 {
    fnv1a_continue(fnv1a_continue(FNV_OFFSET, name), descriptor)
}

/// `String.hashCode()` as this runtime computes it: `h = 31·h + b` over
/// the constant's bytes (its strings are byte-backed). What a literal
/// pool row stores, so `hashCode()` on a literal is a read.
#[inline]
pub const fn string_hash(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        h = h.wrapping_mul(31).wrapping_add(bytes[i] as u32);
        i += 1;
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        // Published FNV-1a 32-bit test values.
        assert_eq!(name_hash(b""), 0x811c_9dc5);
        assert_eq!(name_hash(b"a"), 0xe40c_292c);
        assert_eq!(name_hash(b"foobar"), 0xbf9c_f968);
    }

    #[test]
    fn sig_hash_is_the_hash_of_the_concatenation() {
        assert_eq!(sig_hash(b"speak", b"()I"), name_hash(b"speak()I"));
        assert_ne!(sig_hash(b"speak", b"()I"), sig_hash(b"speak", b"()V"));
    }

    #[test]
    fn is_const() {
        const H: u32 = name_hash(b"java/lang/Object");
        assert_eq!(H, name_hash(b"java/lang/Object"));
    }
}
