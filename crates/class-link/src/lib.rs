// SPDX-License-Identifier: GPL-3.0-only
//! Pack-time linking of JVM class files.
//!
//! A class file is standard JVM bytes; everything the interpreter needs to
//! *find* things in it — where each constant-pool entry lies, which methods
//! and fields it declares, what a call site's descriptor means — used to be
//! parsed into RAM the first time a class was touched (`Parsed`, ~360 B a
//! class, the largest heap consumer on the device). This crate computes that
//! record once, at pack time for an app and at firmware-build time for the
//! framework, as a **link table** stored next to the class bytes and read in
//! place from flash. It adds what the parser never had: a hash per method
//! signature and per superclass / interface name, so resolution compares
//! integers, and a 4-byte descriptor per `Methodref` (`argc` and flags), so
//! an invoke learns its argument count from one read instead of decoding the
//! constant pool.
//!
//! Three layers, all little-endian, all sized for `u16` offsets (a class
//! file is at most 65,535 bytes):
//!
//! - [`Link`]: one class's table, `u16` words (see [`layout`] for the map).
//! - [`ClassSection`]: a set of classes, each with its table, plus a
//!   [`IndexEntry`] array sorted by name hash and the set's string
//!   constants deduplicated into a [`Literal`] pool — the payload of a PAPK
//!   `CLSS` section and, byte for byte, the framework corpus embedded in
//!   firmware. One reader serves both.
//! - [`build`] (feature `build`, needs `alloc`): the builder. It and
//!   [`Link::validate`] run the *same* derivation of the table from the class
//!   bytes ([`classfile::derive`]) — one writes each word, the other compares
//!   each word — so a table that validates is exactly what the builder would
//!   have produced, and the two can never drift apart.
//!
//! Names are hashed as the class file spells them. Under `--shrink` the
//! files are already renamed when they get here, so the hashes match what
//! the runtime computes over its own (shrunk) constants.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "build")]
extern crate alloc;

// The tables are little-endian and read in place as `u16` words; a
// big-endian target would need byte swaps this crate does not do.
#[cfg(target_endian = "big")]
compile_error!(
    "class-link reads little-endian tables in place; big-endian targets are unsupported"
);

pub mod classfile;
pub mod descriptor;
mod error;
pub mod hash;
pub mod layout;
pub mod section;
mod validate;

#[cfg(feature = "build")]
pub mod build;

#[cfg(all(test, feature = "build"))]
mod fixture;
#[cfg(all(test, feature = "build"))]
mod tests;

pub use descriptor::count_args;
pub use error::LinkError;
pub use hash::{name_hash, sig_hash, string_hash};
pub use layout::{
    field_kind, kind_slots, FieldInfo, IfaceInfo, Link, Linked, MethodInfo, MethodrefDesc,
    StringDesc, HEADER_WORDS, KIND_DOUBLE, KIND_FLOAT, KIND_INT, KIND_LONG, KIND_REF, LINK_MAGIC,
    LIT_NONE, MREF_INTERFACE, SUPER_NONE,
};
pub use section::{ClassSection, IndexEntry, Literal, Literals, LIT_UTF8, MAX_LITERALS};
