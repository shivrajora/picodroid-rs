// SPDX-License-Identifier: GPL-3.0-only
//! RP2350 hardware TRNG — real entropy for TCP ISNs and DHCP xids (NET-6).
//!
//! Non-blocking by design: each finished collection round yields a 192-bit
//! EHR that is buffered as six words; a caller arriving with the buffer
//! empty and the next round still sampling gets `None` and falls back to
//! the timer-seeded LCG in `entropy.rs` (which additionally XOR-mixes every
//! TRNG word we do hand out, so the fallback stream degrades gracefully
//! instead of staying predictable).
//!
//! Single-caller contract: only `entropy::picodroid_port_entropy32` reaches
//! this, and FreeRTOS+TCP calls it from the IP task alone — so the buffer
//! needs no locking. Keep it that way or add a critical section.

use rp235x_hal::pac;

use core::sync::atomic::{AtomicBool, Ordering};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// EHR words not yet handed out (single-caller; see module docs).
static mut BUFFER: [u32; 6] = [0; 6];
static mut AVAILABLE: usize = 0;

/// clk_sys cycles between ROSC samples. Conservative (slow) on purpose:
/// one 192-bit harvest takes 192 × 50000 / 150 MHz ≈ 64 ms, far more than
/// the von-Neumann/autocorrelation gates need — and connection setup and
/// DHCP are rare enough that the six-word buffer absorbs the latency.
const SAMPLE_CLOCKS: u32 = 50_000;

/// Program the sampling config and start the ROSC entropy source.
fn arm(trng: &pac::TRNG) {
    // Shortest inverter chain (0) is the datasheet-default source select.
    trng.trng_config()
        .write(|w| unsafe { w.rnd_src_sel().bits(0) });
    trng.sample_cnt1()
        .write(|w| unsafe { w.sample_cntr1().bits(SAMPLE_CLOCKS) });
    trng.rnd_source_enable().write(|w| w.rnd_src_en().set_bit());
}

fn ensure_init(p: &pac::Peripherals) {
    if INITIALIZED.swap(true, Ordering::Relaxed) {
        return;
    }
    p.RESETS.reset().modify(|_, w| w.trng().clear_bit());
    while p.RESETS.reset_done().read().trng().bit_is_clear() {}
    // The boot ROM may have left arbitrary state; start from scratch.
    p.TRNG
        .trng_sw_reset()
        .write(|w| w.trng_sw_reset().set_bit());
    arm(&p.TRNG);
}

/// Copy a finished 192-bit collection round into the buffer, if one is
/// ready. Returns false when still sampling or when a statistical health
/// test tripped (in which case the block is reset and re-armed — it halts
/// until software intervenes).
fn try_refill(trng: &pac::TRNG) -> bool {
    let isr = trng.rng_isr().read();
    if isr.autocorr_err().bit_is_set() || isr.crngt_err().bit_is_set() || isr.vn_err().bit_is_set()
    {
        trng.rng_icr().write(|w| {
            w.autocorr_err()
                .set_bit()
                .crngt_err()
                .set_bit()
                .vn_err()
                .set_bit()
                .ehr_valid()
                .set_bit()
        });
        trng.trng_sw_reset().write(|w| w.trng_sw_reset().set_bit());
        arm(trng);
        return false;
    }
    if trng.trng_valid().read().ehr_valid().bit_is_clear() {
        return false;
    }
    unsafe {
        BUFFER[0] = trng.ehr_data0().read().bits();
        BUFFER[1] = trng.ehr_data1().read().bits();
        BUFFER[2] = trng.ehr_data2().read().bits();
        BUFFER[3] = trng.ehr_data3().read().bits();
        BUFFER[4] = trng.ehr_data4().read().bits();
        BUFFER[5] = trng.ehr_data5().read().bits();
        AVAILABLE = 6;
    }
    // Reading the EHR restarts collection; clear the sticky valid flag so
    // the next poll reflects the new round.
    trng.rng_icr().write(|w| w.ehr_valid().set_bit());
    true
}

/// One hardware-random word, or `None` when no entropy is buffered yet
/// (the caller falls back to its LCG).
pub fn try_random_u32() -> Option<u32> {
    let p = unsafe { pac::Peripherals::steal() };
    ensure_init(&p);
    unsafe {
        if AVAILABLE == 0 && !try_refill(&p.TRNG) {
            return None;
        }
        AVAILABLE -= 1;
        Some(BUFFER[AVAILABLE])
    }
}
