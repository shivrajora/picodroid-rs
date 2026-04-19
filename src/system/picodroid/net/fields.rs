//! Field slot indices for picodroid.net Java classes.
//!
//! Each constant maps to the declaration order of the corresponding
//! private field in the Java class.

pub mod socket {
    pub const HANDLE: usize = 0;
}

pub mod server_socket {
    pub const HANDLE: usize = 0;
}

pub mod datagram_socket {
    pub const HANDLE: usize = 0;
}

pub mod datagram_packet {
    pub const DATA: usize = 0;
    pub const LENGTH: usize = 1;
    pub const ADDRESS: usize = 2;
    pub const PORT: usize = 3;
}

pub mod http_input_stream {
    pub const HANDLE: usize = 0;
}

pub mod http_output_stream {
    pub const HANDLE: usize = 0;
}
