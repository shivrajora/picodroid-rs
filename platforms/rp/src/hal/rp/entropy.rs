// SPDX-License-Identifier: GPL-3.0-only
//! The RP family's answers to the two entropy questions shared code asks
//! (design D3 in docs/designs/network-seam-2026-09.md):
//!
//! - `picodroid_port_entropy32` for the FreeRTOS+TCP glue
//!   (`picodroid-core/net-freertos-tcp/net_init.c`): TCP initial sequence
//!   numbers, DHCP transaction ids, DNS ids. Never fails: RP2350, a
//!   hardware TRNG word when one is buffered (`trng.rs`, NET-6), else a
//!   timer-seeded LCG that every TRNG word XOR-mixes into; RP2040, which
//!   has no TRNG, the LCG alone (no RP2040 board has a network).
//! - `picodroid_port_entropy_bytes` for TLS key material
//!   (`picodroid-core/src/net/tls.rs`): TRNG bytes only, or nothing yet —
//!   a handshake seed must never come from the LCG.
//!
//! Two callers on two cores now (the IP task and a Java thread running a
//! handshake), so the TRNG's buffer is reached under the family's
//! critical section: rp235x-hal's `critical-section-impl`, a hardware
//! spinlock plus masked interrupts, which serialises both cores. The
//! LCG state stays a plain atomic.

use core::sync::atomic::{AtomicU32, Ordering};

static LCG_STATE: AtomicU32 = AtomicU32::new(0x1234_5678);

#[cfg(feature = "chip-rp2350")]
fn hardware_word() -> Option<u32> {
    critical_section::with(|_| super::trng::try_random_u32())
}

#[cfg(not(feature = "chip-rp2350"))]
fn hardware_word() -> Option<u32> {
    None
}

/// One random word for the shared stack glue (TCP initial sequence numbers,
/// DHCP transaction ids, DNS ids). Never fails.
#[no_mangle]
pub extern "C" fn picodroid_port_entropy32() -> u32 {
    let mut state = LCG_STATE.load(Ordering::Relaxed);
    if let Some(hw) = hardware_word() {
        state ^= hw;
        LCG_STATE.store(state, Ordering::Relaxed);
        return hw;
    }
    // Fallback: mix the free-running timer into the LCG.
    state ^= super::system_clock::elapsed_realtime_nanos() as u32;
    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    LCG_STATE.store(state, Ordering::Relaxed);
    state
}

/// Hardware entropy for a TLS handshake: fills the front of `buf` with
/// whatever the TRNG has harvested and returns the count — 0 while it is
/// still sampling (a 192-bit round takes ~64 ms), so the caller waits and
/// asks again. Never the LCG: key material needs the real source. Always 0
/// on the RP2040, which has no TRNG.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn picodroid_port_entropy_bytes(buf: *mut u8, len: usize) -> usize {
    if buf.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: the caller passes a live, writable buffer of `len` bytes.
    let out = unsafe { core::slice::from_raw_parts_mut(buf, len) };
    let mut filled = 0usize;
    for chunk in out.chunks_mut(4) {
        let Some(word) = hardware_word() else {
            break;
        };
        let bytes = word.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
        filled += chunk.len();
    }
    filled
}
