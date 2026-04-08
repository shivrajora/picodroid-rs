//! Simulator network HAL — wraps host OS sockets (std::net).
//!
//! Provides the same public API as hal::rp::net so that Java Socket
//! native methods work identically on the simulator and real hardware.

/// Error returned by network operations.
#[derive(Debug, Clone, Copy)]
pub struct NetError(pub i32);

// The sim doesn't use FreeRTOS+TCP; it uses std::net directly.
// For now, provide stubs so the module compiles.  Full std::net
// integration will be added when the Java Socket API is wired up.

/// Check if the network stack is up (always true in sim).
pub fn is_network_up() -> bool {
    true
}

/// Get the assigned IP address (127.0.0.1 in sim).
pub fn get_ip_address() -> u32 {
    0x7F000001 // 127.0.0.1 in network byte order on little-endian
}
