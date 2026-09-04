// SPDX-License-Identifier: GPL-3.0-only
//! `picodroid_port_entropy32` — the RP family's answer to the one entropy
//! question the shared FreeRTOS+TCP glue asks
//! (`picodroid-core/net-freertos-tcp/net_init.c`; design D3 in
//! docs/designs/network-seam-2026-09.md).
//!
//! RP2350: a hardware TRNG word when one is buffered (`trng.rs`, NET-6),
//! else a timer-seeded LCG that every TRNG word XOR-mixes into, so the
//! fallback stream stops being predictable after the first harvest. RP2040
//! has no TRNG, so the LCG alone serves there (no RP2040 board has a
//! network today).
//!
//! Single-caller contract: FreeRTOS+TCP calls both of its random hooks from
//! the IP task, so the LCG state needs no locking beyond plain atomics.

use core::sync::atomic::{AtomicU32, Ordering};

static LCG_STATE: AtomicU32 = AtomicU32::new(0x1234_5678);

#[cfg(feature = "chip-rp2350")]
fn hardware_word() -> Option<u32> {
    super::trng::try_random_u32()
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
