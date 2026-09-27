// SPDX-License-Identifier: GPL-3.0-only
//! Field slot indices for the picodroid.protobuf Java classes.
//!
//! Each constant maps to the declaration order of the corresponding
//! instance field in the Java class; `native_field_tables_tests` checks
//! them against the class files.

pub mod coded_input_stream {
    pub const BUF: usize = 0;
    pub const POS: usize = 1;
    pub const LIMIT: usize = 2;
    // `mLastTag` (slot 3) and `mStart` (slot 4) follow; the natives never
    // read them.
}

pub mod coded_output_stream {
    pub const BUF: usize = 0;
    pub const POS: usize = 1;
    pub const LIMIT: usize = 2;
}
