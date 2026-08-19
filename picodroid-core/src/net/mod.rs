// SPDX-License-Identifier: GPL-3.0-only
//! picodroid.net — Java networking API native implementations.

pub mod datagram_socket;
pub mod fields;
pub mod helpers;
pub mod http_connection;
/// Pure head-parsing helpers. Tested via the `#[path]` shim in `lib.rs` —
/// `net` itself is `cfg(not(test))`, so tests written inside it never run.
pub mod http_head;
pub mod http_table;
pub mod inet_address;
pub mod network_info;
mod ptr_table;
pub mod server_socket;
pub mod socket;
pub mod socket_table;
