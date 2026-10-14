// SPDX-License-Identifier: GPL-3.0-only
//! The family's hardware entropy, for every caller in this crate.
//!
//! Every family defines `picodroid_port_entropy_bytes` (the RP family in
//! `hal/rp/entropy.rs`, the simulator in `hal/sim/platform.rs`). Its
//! callers are the TLS handshake seed (`net/tls.rs`) and the random serial
//! of a chip without an ID (`identity`).

extern "C" {
    /// Fills the front of `buf` with hardware entropy and returns the count:
    /// 0 while the source is still sampling, and always 0 on a chip with no
    /// TRNG. Non-blocking, and never a pseudo-random fallback.
    fn picodroid_port_entropy_bytes(buf: *mut u8, len: usize) -> usize;
}

/// Fill the front of `buf` from the hardware source and return how many
/// bytes it filled; see the declaration above for when that is 0.
pub fn fill(buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    // SAFETY: `buf` is a live, writable slice of the stated length.
    let n = unsafe { picodroid_port_entropy_bytes(buf.as_mut_ptr(), buf.len()) };
    n.min(buf.len())
}
