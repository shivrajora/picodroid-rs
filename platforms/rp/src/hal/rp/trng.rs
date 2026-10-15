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
//! Two callers on two cores reach this — `entropy.rs`'s word for the IP
//! task's TCP/DHCP/DNS ids and its bytes for a Java thread's TLS handshake —
//! and both come through `entropy::hardware_word`, inside the family's
//! critical section (a hardware spinlock plus masked interrupts). The buffer
//! lives behind that section's `Mutex`, and [`try_random_u32`] takes the
//! token, so it cannot be reached outside one.

use rp235x_hal::pac;

use core::cell::Cell;
use core::sync::atomic::{AtomicBool, Ordering};

use critical_section::{CriticalSection, Mutex};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// EHR words not yet handed out: the first `AVAILABLE` of them.
static BUFFER: Mutex<[Cell<u32>; 6]> = Mutex::new([const { Cell::new(0) }; 6]);
static AVAILABLE: Mutex<Cell<usize>> = Mutex::new(Cell::new(0));

/// clk_sys cycles between ROSC samples. Conservative (slow) on purpose:
/// one 192-bit harvest takes 192 × 50000 / 150 MHz ≈ 64 ms, far more than
/// the von-Neumann/autocorrelation gates need — and connection setup and
/// DHCP are rare enough that the six-word buffer absorbs the latency.
const SAMPLE_CLOCKS: u32 = 50_000;

/// Program the sampling config and start the ROSC entropy source.
fn arm(trng: &pac::TRNG) {
    // Shortest inverter chain (0) is the datasheet-default source select:
    // TRNG_CONFIG's reset value, which `write` starts from.
    trng.trng_config().write(|w| w);
    // SAFETY: SAMPLE_CNT1 takes a clock count the PAC has no range for
    // (`bits` masks to the field); the TRNG reaches no memory.
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
fn try_refill(trng: &pac::TRNG, cs: CriticalSection) -> bool {
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
    let buffer = BUFFER.borrow(cs);
    buffer[0].set(trng.ehr_data0().read().bits());
    buffer[1].set(trng.ehr_data1().read().bits());
    buffer[2].set(trng.ehr_data2().read().bits());
    buffer[3].set(trng.ehr_data3().read().bits());
    buffer[4].set(trng.ehr_data4().read().bits());
    buffer[5].set(trng.ehr_data5().read().bits());
    AVAILABLE.borrow(cs).set(6);
    // Reading the EHR restarts collection; clear the sticky valid flag so
    // the next poll reflects the new round.
    trng.rng_icr().write(|w| w.ehr_valid().set_bit());
    true
}

/// One hardware-random word, or `None` when no entropy is buffered yet
/// (the caller falls back to its LCG).
pub fn try_random_u32(cs: CriticalSection) -> Option<u32> {
    let p = crate::hal::chip::periph::steal();
    ensure_init(&p);
    let available = AVAILABLE.borrow(cs);
    if available.get() == 0 && !try_refill(&p.TRNG, cs) {
        return None;
    }
    available.set(available.get() - 1);
    Some(BUFFER.borrow(cs)[available.get()].get())
}
