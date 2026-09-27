// SPDX-License-Identifier: GPL-3.0-only
//! The handshake RNG: an HMAC-SHA256 counter generator keyed with 32 bytes
//! of hardware entropy — the `hmac` and `sha2` the TLS code already links,
//! and nothing with SIMD-aligned state. (`rand_chacha` keeps its block in a
//! 16-byte-aligned type on x86, which the simulator's allocator refuses
//! because the device heap aligns to 8; the soft SHA-256 state is `u32`s.)
//!
//! Output block `i` is `HMAC(seed, i)`; the seed never leaves the key and
//! the counter never repeats within a session, so the stream is a PRF of
//! secret input — what the ECDHE ephemeral key and the ClientHello random
//! need. One session, one seed: `TlsSession::open` takes fresh entropy
//! every time.

use hmac::{Hmac, Mac};
use rand_core::{CryptoRng, RngCore};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct HmacDrbg {
    seed: [u8; 32],
    counter: u64,
    block: [u8; 32],
    used: usize,
}

impl HmacDrbg {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            seed,
            counter: 0,
            block: [0; 32],
            used: 32,
        }
    }

    fn refill(&mut self) {
        let mut mac = HmacSha256::new_from_slice(&self.seed).expect("HMAC accepts any key length");
        mac.update(&self.counter.to_le_bytes());
        self.counter = self.counter.wrapping_add(1);
        self.block.copy_from_slice(&mac.finalize().into_bytes());
        self.used = 0;
    }
}

impl RngCore for HmacDrbg {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }

    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut off = 0;
        while off < dest.len() {
            if self.used == self.block.len() {
                self.refill();
            }
            let n = (self.block.len() - self.used).min(dest.len() - off);
            dest[off..off + n].copy_from_slice(&self.block[self.used..self.used + n]);
            self.used += n;
            off += n;
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRng for HmacDrbg {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_per_seed_and_distinct_across_seeds() {
        let mut a = HmacDrbg::from_seed([7; 32]);
        let mut b = HmacDrbg::from_seed([7; 32]);
        let mut c = HmacDrbg::from_seed([8; 32]);
        let (mut x, mut y, mut z) = ([0u8; 100], [0u8; 100], [0u8; 100]);
        a.fill_bytes(&mut x);
        b.fill_bytes(&mut y);
        c.fill_bytes(&mut z);
        assert_eq!(x, y);
        assert_ne!(x, z);
        assert!(x.iter().any(|&v| v != 0));
    }

    #[test]
    fn small_reads_straddle_blocks() {
        let mut whole = HmacDrbg::from_seed([1; 32]);
        let mut piecewise = HmacDrbg::from_seed([1; 32]);
        let mut expect = [0u8; 70];
        whole.fill_bytes(&mut expect);
        let mut got = [0u8; 70];
        for chunk in got.chunks_mut(7) {
            piecewise.fill_bytes(chunk);
        }
        assert_eq!(expect, got);
        assert_eq!(whole.next_u32(), piecewise.next_u32());
        assert_eq!(whole.next_u64(), piecewise.next_u64());
    }

    #[test]
    fn nothing_the_session_boxes_wants_more_than_the_device_heap_aligns_to() {
        // The device heap (FreeRTOS heap_4 through freertos-rust) aligns to
        // 8; the simulator aborts on anything larger. The host must be built
        // with the soft AES/POLYVAL backends (.cargo/config.toml) for this
        // to hold on x86.
        assert!(core::mem::align_of::<HmacDrbg>() <= 8);
        assert!(core::mem::align_of::<crate::session::Boxed>() <= 8);
    }
}
