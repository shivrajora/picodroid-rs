// SPDX-License-Identifier: GPL-3.0-only
pub mod xfer;

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use freertos_rust::{Duration, InterruptContext, Semaphore};
use picodroid_core::util::set_once::SetOnce;

use super::init_gate::{Claim, InitGate};

use xfer::{
    clock_divisors, may_queue_tx, stores_rx, transfer_finished, SpiOp, SpiXferState,
    SMALL_XFER_THRESHOLD, SPI_FIFO_DEPTH, SPI_XFER_POLL_MS, SPI_XFER_TIMEOUT_MS,
};

use super::clock::PCLK_HZ;

// `reg.write(|w| w)` below stores the register's reset value — for every
// register written that way here, 0: all interrupts masked, DMA requests
// off, the controller disabled. It is the PAC's safe spelling of the raw
// `bits(0)` it replaces, and compiles to the same store; `reset()` would
// too, but shifts LLVM's inlining around `write_raw_start` (+16 B on the
// RP2040).
//
// The raw writes that remain are the data register and two clock
// registers, whose numeric fields the PAC has no range for. The SPI block
// reaches no memory by itself — DMA does that, from `dma.rs` — so any value
// written is memory-safe; a wrong one garbles the bus.

// ── Per-peripheral statics ───────────────────────────────────────────────────

/// A transfer's state, shared by the task that runs the transfer and the
/// bus's interrupt handler. Atomics throughout, reached through `&` from
/// both sides; the turn-taking that keeps the two apart is described on
/// the type (`xfer::SpiXferState`).
static SPI0_STATE: SpiXferState = SpiXferState::new();
static SPI1_STATE: SpiXferState = SpiXferState::new();

/// Given by the completion interrupts (`SPIx_IRQ` here, `DMA_IRQ_0` in
/// `dma.rs`), taken by the transfer's task. Stored by `init_with_pins`.
pub(crate) static SPI0_DONE: SetOnce<Semaphore> = SetOnce::new();
pub(crate) static SPI1_DONE: SetOnce<Semaphore> = SetOnce::new();

/// The bus lock. Stored by `init_with_pins`.
static SPI0_LOCK: SetOnce<Semaphore> = SetOnce::new();
static SPI1_LOCK: SetOnce<Semaphore> = SetOnce::new();

/// A DMA write `write_raw_start` began and nobody has collected yet. While
/// set, the bus lock is held on the starter's behalf. Only the task that
/// started the write ever reads or clears this — the lock keeps everyone
/// else off the bus — so plain loads and stores are enough, which is also
/// all the M0+ has.
static SPI0_WRITE_PENDING: AtomicBool = AtomicBool::new(false);
static SPI1_WRITE_PENDING: AtomicBool = AtomicBool::new(false);

fn write_pending(id: u8) -> &'static AtomicBool {
    match id {
        0 => &SPI0_WRITE_PENDING,
        _ => &SPI1_WRITE_PENDING,
    }
}

/// Transfers the ISR left one or more bytes short of `len` while the
/// controller reported itself finished (see `finish_isr_xfer!`). Reported once
/// per boot; the count is readable from a debugger for the rest of the run.
///
/// Load-then-store rather than an atomic read-modify-write, for the same
/// reason as `SPI0_WRITE_PENDING` above: the M0+ has no RMW, and the bus lock
/// is held across the transfer that writes these, so there is one writer.
static SHORT_READS: AtomicU32 = AtomicU32::new(0);
static SHORT_READ_REPORTED: AtomicBool = AtomicBool::new(false);

/// RX overruns the ISR saw asserted (RORRIS) on entry, before it cleared
/// them. Written only from the ISR (one writer, load-then-store as above),
/// read by the short-read report so a fallback that fires is attributable:
/// a short read with this at 0 is not the FIFO losing a byte.
static RX_OVERRUNS: AtomicU32 = AtomicU32::new(0);

/// The task that started the pending DMA write (its raw handle), so that a
/// different task arriving at the bus does not collect a completion that
/// is not its own — it takes the lock instead, and blocks until the owner
/// has collected. Written under the lock by the starter, read by anyone.
static SPI0_WRITE_OWNER: AtomicUsize = AtomicUsize::new(0);
static SPI1_WRITE_OWNER: AtomicUsize = AtomicUsize::new(0);

fn write_owner(id: u8) -> &'static AtomicUsize {
    match id {
        0 => &SPI0_WRITE_OWNER,
        _ => &SPI1_WRITE_OWNER,
    }
}

/// Collect the pending DMA write if this task is the one that started it.
/// Every entry to the bus goes through here before taking the lock: the
/// starter holds the lock until it collects, so if it did not collect first
/// it would wait on itself; another task's write is that task's to
/// collect, and the lock is what keeps this one off the bus meanwhile.
fn collect_own_write(spi_id: u8) {
    if write_pending(spi_id).load(Ordering::Acquire)
        && write_owner(spi_id).load(Ordering::Relaxed) == picodroid_core::rtos::task_current()
    {
        write_raw_finish(spi_id);
    }
}

/// The bus's transfer state, for the task that holds the bus lock.
fn spi_state(id: u8) -> &'static SpiXferState {
    match id {
        0 => &SPI0_STATE,
        _ => &SPI1_STATE,
    }
}

fn spi_done(id: u8) -> &'static Semaphore {
    match id {
        0 => SPI0_DONE.get(),
        _ => SPI1_DONE.get(),
    }
    .expect("SPI done semaphore not initialised")
}

fn spi_lock(id: u8) -> &'static Semaphore {
    match id {
        0 => SPI0_LOCK.get(),
        _ => SPI1_LOCK.get(),
    }
    .expect("SPI lock semaphore not initialised")
}

// ── ISR ──────────────────────────────────────────────────────────────────────

macro_rules! spi_isr_body {
    ($spi:expr, $state_static:expr, $done_static:expr) => {{
        // The handler runs only while the transfer has the bus's interrupts
        // unmasked, which is its turn with the state (`xfer::SpiXferState`):
        // the description is read once here, and the counters go back at
        // each exit below.
        let state: &SpiXferState = &$state_static;
        let op = state.op();
        let len = state.len();
        let rx_len = state.rx_len();
        let tx_ptr = state.tx_ptr();
        let rx_ptr = state.rx_ptr();
        let mut tx_idx = state.tx_idx();
        let mut rx_idx = state.rx_idx();
        let mut ctx = InterruptContext::new();

        // 0. Count an overrun before anything clears it: steps 2 and 5 write
        //    RORIC, and a byte the FIFO dropped must not vanish with it.
        if $spi.sspris().read().rorris().bit_is_set() {
            RX_OVERRUNS.store(
                RX_OVERRUNS.load(Ordering::Relaxed).saturating_add(1),
                Ordering::Relaxed,
            );
        }

        // 1. Drain RX FIFO first (prevents overrun)
        while $spi.sspsr().read().rne().bit_is_set() && rx_idx < len {
            let byte = $spi.sspdr().read().data().bits() as u8;
            if stores_rx(op, rx_idx, rx_len) {
                // SAFETY: `rx_ptr` is `transfer_raw`'s `rx`, which outlives
                // the transfer (the task waits for it), and `rx_len` is that
                // slice's length, so `rx_idx < rx_len` keeps the write in
                // bounds. An `rx` shorter than `tx` has its tail discarded
                // here (UR-2).
                unsafe { *rx_ptr.add(rx_idx) = byte };
            }
            rx_idx += 1;
        }

        // 2. Check completion
        if rx_idx >= len {
            $spi.sspimsc().write(|w| w);
            $spi.sspicr() // clear RORIC + RTIC
                .write(|w| w.roric().clear_bit_by_one().rtic().clear_bit_by_one());
            state.set_progress(tx_idx, rx_idx);
            state.set_op(SpiOp::Idle);
            if let Some(sem) = $done_static.get() {
                sem.give_from_isr(&mut ctx);
            }
            return;
        }

        // 3. Refill TX FIFO — by bytes outstanding, not only by TNF. The RX
        //    FIFO is as deep as the TX FIFO, so more than SPI_FIFO_DEPTH bytes
        //    queued and not yet read back is an overrun waiting for one slow
        //    interrupt (xfer::may_queue_tx).
        while may_queue_tx(tx_idx, rx_idx, len) && $spi.sspsr().read().tnf().bit_is_set() {
            // SAFETY: `tx_ptr` is the transfer's `tx`, alive until it ends,
            // and `may_queue_tx` holds `tx_idx < len`, its length.
            let byte = unsafe { *tx_ptr.add(tx_idx) };
            $spi.sspdr().write(|w| {
                // SAFETY: one data byte (the note on raw writes at the top).
                unsafe { w.data().bits(byte as u16) }
            });
            tx_idx += 1;
        }
        state.set_progress(tx_idx, rx_idx);

        // 4. All TX queued — disable TXIM, keep RTIM to catch final RX bytes
        if tx_idx >= len {
            $spi.sspimsc()
                .write(|w| w.rtim().set_bit().rorim().set_bit());
        }

        // 5. Clear receive timeout / overrun if they fired
        $spi.sspicr()
            .write(|w| w.roric().clear_bit_by_one().rtic().clear_bit_by_one());

        // ctx drops here → freertos_rs_isr_yield if a higher-priority task woke
    }};
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn SPI0_IRQ() {
    let p = crate::hal::chip::periph::steal();
    spi_isr_body!(&p.SPI0, SPI0_STATE, SPI0_DONE);
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn SPI1_IRQ() {
    let p = crate::hal::chip::periph::steal();
    spi_isr_body!(&p.SPI1, SPI1_STATE, SPI1_DONE);
}

// ── Speed configuration ─────────────────────────────────────────────────────

macro_rules! apply_config {
    ($spi:expr, $freq_hz:expr, $mode:expr) => {{
        // 1. Disable the SSP controller before reconfiguring
        $spi.sspcr1().write(|w| w);

        // 2. Clock prescale divisor (must be even, min 2)
        let (cpsdvsr, scr) = clock_divisors(PCLK_HZ, $freq_hz);
        $spi.sspcpsr().write(|w| {
            // SAFETY: the even prescale divisor `clock_divisors` computed (the
            // note on raw writes at the top).
            unsafe { w.cpsdvsr().bits(cpsdvsr) }
        });

        // 3. SSPCR0: DSS=8-bit (0b0111), FRF=Motorola SPI (0b00),
        //    SPO=CPOL (mode bit 1), SPH=CPHA (mode bit 0), SCR in bits [15:8]
        let spo: u32 = (($mode >> 1) & 1) as u32;
        let sph: u32 = ($mode & 1) as u32;
        let cr0: u32 = ((scr as u32) << 8) | (sph << 7) | (spo << 6) | 0b0111;
        // SAFETY: the SSPCR0 value assembled just above (the note on raw
        // writes at the top).
        $spi.sspcr0().write(|w| unsafe { w.bits(cr0) });

        // 4. Re-enable: SSE=1, MS=0 (master mode)
        $spi.sspcr1().write(|w| w.sse().set_bit().ms().clear_bit());

        cortex_m::asm::dsb();
    }};
}

fn do_reconfigure(spi_id: u8, freq_hz: u32, mode: u32) {
    let p = crate::hal::chip::periph::steal();
    match spi_id {
        0 => apply_config!(&p.SPI0, freq_hz, mode),
        _ => apply_config!(&p.SPI1, freq_hz, mode),
    }
}

// ── Polling helpers (small transfers) ────────────────────────────────────────

// spin-ok: FIFO-depth polls, used only for transfers of at most
// SMALL_XFER_THRESHOLD bytes — an interrupt round trip costs more than the
// whole transfer (spi/xfer.rs).
macro_rules! poll_write_raw {
    ($spi:expr, $data:expr) => {{
        for &byte in $data {
            while $spi.sspsr().read().tnf().bit_is_clear() {}
            $spi.sspdr().write(|w| {
                // SAFETY: one data byte (the note on raw writes at the top).
                unsafe { w.data().bits(byte as u16) }
            });
            while $spi.sspsr().read().rne().bit_is_clear() {}
            let _ = $spi.sspdr().read();
        }
        while $spi.sspsr().read().bsy().bit_is_set() {}
    }};
}

// spin-ok: as poll_write_raw — at most SMALL_XFER_THRESHOLD bytes.
// Same length contract as the ISR path: every `tx` byte is clocked, and a
// received byte is kept only while `rx` has room for it (UR-2).
macro_rules! poll_transfer_raw {
    ($spi:expr, $tx:expr, $rx:expr) => {{
        for (i, &byte) in $tx.iter().enumerate() {
            while $spi.sspsr().read().tnf().bit_is_clear() {}
            $spi.sspdr().write(|w| {
                // SAFETY: one data byte (the note on raw writes at the top).
                unsafe { w.data().bits(byte as u16) }
            });
            while $spi.sspsr().read().rne().bit_is_clear() {}
            let got = $spi.sspdr().read().data().bits() as u8;
            if let Some(slot) = $rx.get_mut(i) {
                *slot = got;
            }
        }
        while $spi.sspsr().read().bsy().bit_is_set() {}
    }};
}

// ── ISR-driven transfer helpers ──────────────────────────────────────────────

macro_rules! start_isr_xfer {
    ($spi:expr, $state:expr, $tx:expr) => {{
        // Seed TX FIFO (up to FIFO depth)
        let seed = $tx.len().min(SPI_FIFO_DEPTH);
        for &byte in &$tx[..seed] {
            $spi.sspdr().write(|w| {
                // SAFETY: one data byte (the note on raw writes at the top).
                unsafe { w.data().bits(byte as u16) }
            });
        }
        $state.set_progress(seed, 0);
        // The state is the handler's from the unmask on: order its
        // description (relaxed stores) before that volatile write.
        core::sync::atomic::fence(Ordering::Release);
        // Clear stale interrupts, enable TXIM + RTIM + RORIM
        $spi.sspicr() // clear RORIC + RTIC
            .write(|w| w.roric().clear_bit_by_one().rtic().clear_bit_by_one());
        $spi.sspimsc()
            .write(|w| w.txim().set_bit().rtim().set_bit().rorim().set_bit());
    }};
}

macro_rules! finish_isr_xfer {
    ($spi:expr, $spi_id:expr) => {{
        let done = spi_done($spi_id);
        // The ISR ends a transfer by counting the bytes it has *received*, and
        // that count can stop short of `len` while the controller is finished:
        // measured on the testbench W slot, an XPT2046 poll (30 bytes at
        // 2 MHz) regularly ends with `rx_idx == len - 1`, the shifter idle,
        // the RX FIFO empty and neither the overrun nor the receive-timeout
        // status asserted. Whether the byte was dropped by the eight-deep FIFO
        // or its receive-timeout was cleared by step 5 of the ISR body as it
        // asserted is not settled — either way no further interrupt can come,
        // because the receive timeout only asserts on a FIFO that is not
        // empty.
        //
        // Waiting the cap out for that signal is what froze the UI task for a
        // full five seconds a time — and, since the JVM run lock, every Java
        // thread with it: the qa_life stall on this slot
        // (docs/qa-2026-09-13-followups.md item 1). So the wait also accepts
        // the controller's own account of being finished: everything queued
        // has been shifted out, the shifter is idle, the RX FIFO is empty.
        // What is lost then is one sample, not five seconds.
        //
        // A DMA write is a different question: the ISR path's byte counters
        // describe whatever ISR transfer ran last, and `BSY` alone drops for
        // a moment at every TX-FIFO underrun between bursts. So the kind in
        // `SpiXferState` picks the predicate, and a DMA write is finished only
        // once both channels have retired with the FIFO drained
        // (xfer::transfer_finished).
        let mut waited_ms: u32 = 0;
        let signalled = loop {
            if done.take(Duration::ms(SPI_XFER_POLL_MS)).is_ok() {
                break true;
            }
            waited_ms = waited_ms.saturating_add(SPI_XFER_POLL_MS);
            let st = spi_state($spi_id);
            let sr = $spi.sspsr().read();
            if transfer_finished(
                st.op(),
                st.tx_idx(),
                st.len(),
                super::dma::busy($spi_id),
                sr.tfe().bit_is_set(),
                sr.bsy().bit_is_set(),
                sr.rne().bit_is_set(),
            ) {
                // A transfer that ended in the gap between the take timing
                // out and the registers being read has given its credit by
                // now (the completion interrupt runs before this task does):
                // that is an ordinary completion, not the fallback. A 12.8 KB
                // band at 75 MHz takes 1.4 ms against a 2-tick take that can
                // expire after one, so graphicsbench hit this gap once a run
                // on pico_display2_w and logged a fallback that never was.
                if done.take(Duration::zero()).is_ok() {
                    break true;
                }
                // Once per boot: on a board where this is the steady state it
                // is one line per touch poll, and the count is what matters.
                let first = !SHORT_READ_REPORTED.load(Ordering::Relaxed);
                SHORT_READ_REPORTED.store(true, Ordering::Relaxed);
                if first {
                    if st.op() == SpiOp::DmaWrite {
                        defmt::warn!(
                            "spi{}: DMA write of {} bytes retired without its \
                             completion interrupt; completing it from the \
                             controller's state (further occurrences are silent)",
                            $spi_id,
                            st.len()
                        );
                    } else {
                        defmt::warn!(
                            "spi{}: transfer finished with {} of {} bytes read back \
                             ({} RX overruns so far); completing it from the \
                             controller's state (further occurrences are silent)",
                            $spi_id,
                            st.rx_idx(),
                            st.len(),
                            RX_OVERRUNS.load(Ordering::Relaxed)
                        );
                    }
                }
                SHORT_READS.store(
                    SHORT_READS.load(Ordering::Relaxed).saturating_add(1),
                    Ordering::Relaxed,
                );
                break false;
            }
            if waited_ms >= SPI_XFER_TIMEOUT_MS {
                defmt::error!(
                    "spi{}: transfer stalled {} ms (len={} tx={} rx={})",
                    $spi_id,
                    waited_ms,
                    st.len(),
                    st.tx_idx(),
                    st.rx_idx()
                );
                break false;
            }
        };
        if !signalled {
            super::dma::abort($spi_id);
            $spi.sspimsc().write(|w| w);
            while $spi.sspsr().read().rne().bit_is_set() {
                let _ = $spi.sspdr().read();
            }
            // A give that lands between the decision above and the mask would
            // otherwise be a stale credit the next transfer takes for its own.
            // One take clears it: the semaphore is binary, so it holds at most
            // one.
            let _ = done.take(Duration::zero());
        }
        // The ISR sets Idle itself; the DMA path has no ISR of its own, and
        // the next waiter must not read this transfer's kind as its own.
        spi_state($spi_id).set_op(SpiOp::Idle);
        // spin-ok: one byte-time drain after the interrupt-signalled completion
        while $spi.sspsr().read().bsy().bit_is_set() {}
    }};
}

// ── Public API ───────────────────────────────────────────────────────────────

// One initialiser per bus (`init_gate`): display init and
// `PeripheralManager.openSpi` can each request the same bus, and only the
// first to arrive programs it and fills the cells; a second arrival waits
// for it or returns.
static SPI0_INIT: InitGate = InitGate::new();
static SPI1_INIT: InitGate = InitGate::new();

fn init_gate(spi_id: u8) -> &'static InitGate {
    match spi_id {
        0 => &SPI0_INIT,
        _ => &SPI1_INIT,
    }
}

/// Configure GPIO pins for SPI function and start the controller at 1 MHz, MODE_0.
/// Uses the chip-default pin set for the bus.
pub fn init(spi_id: u8) {
    init_with_pins(spi_id, None, None, None);
}

/// Like `init`, but lets the caller pin SCK/MOSI/MISO to a specific pad. `None`
/// for any pin falls back to the chip default. Boards whose SPI signals aren't
/// on the default pads (e.g. Pimoroni Pico Enviro+ Pack uses GP18/GP19 for
/// SPI0 SCK/MOSI) wire these via `[display]` keys in board.toml.
pub fn init_with_pins(
    spi_id: u8,
    sck_override: Option<u8>,
    mosi_override: Option<u8>,
    miso_override: Option<u8>,
) {
    let gate = init_gate(spi_id);
    match gate.claim() {
        Claim::Ours => {}
        Claim::Busy => return gate.wait_done(),
        Claim::Done => return,
    }

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = crate::hal::chip::periph::steal();

    // Ensure IO_BANK0 and PADS_BANK0 are out of reset (idempotent)
    p.RESETS
        .reset()
        .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
    while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
    while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}

    // Release the appropriate SPI block from reset
    match spi_id {
        0 => {
            p.RESETS.reset().modify(|_, w| w.spi0().clear_bit());
            while p.RESETS.reset_done().read().spi0().bit_is_clear() {}
        }
        _ => {
            p.RESETS.reset().modify(|_, w| w.spi1().clear_bit());
            while p.RESETS.reset_done().read().spi1().bit_is_clear() {}
        }
    }

    // Route GPIO pins to SPI function (function select 1).
    // Default pin assignments (3-wire, CS managed separately via Gpio):
    //   SPI0 → SCK=GP2, MOSI(TX)=GP3, MISO(RX)=GP0
    //   SPI1 → SCK=GP10, MOSI(TX)=GP11, MISO(RX)=GP8
    let (default_sck, default_mosi, default_miso): (usize, usize, usize) = match spi_id {
        0 => (2, 3, 0),
        _ => (10, 11, 8),
    };
    let sck = sck_override.map(|v| v as usize).unwrap_or(default_sck);
    let mosi = mosi_override.map(|v| v as usize).unwrap_or(default_mosi);
    let miso = miso_override.map(|v| v as usize).unwrap_or(default_miso);
    for pin in [sck, mosi, miso] {
        p.IO_BANK0
            .gpio(pin)
            .gpio_ctrl()
            .write(|w| w.funcsel().spi()); // 1 = SPI
        p.PADS_BANK0.gpio(pin).write(|w| {
            #[cfg(feature = "chip-rp2350")]
            let w = w.iso().clear_bit();
            w.ie().set_bit().od().clear_bit()
        });
    }

    // Apply default configuration: 1 MHz, MODE_0
    do_reconfigure(spi_id, 1_000_000, 0);

    // Allocate completion semaphore (binary, starts empty)
    let done = Semaphore::new_binary().expect("spi done sem alloc");
    // Allocate lock semaphore (binary, starts given = unlocked)
    let lock = Semaphore::new_binary().expect("spi lock sem alloc");
    lock.give();

    // SAFETY: the cells are empty and this task is the bus's only
    // initialiser (the gate gave it `Claim::Ours`; every other caller waits
    // for `finish` or has returned), and nothing reads them yet: this bus's
    // interrupt is unmasked below, its DMA completion (`DMA_IRQ_0`, possibly
    // live for the other bus) only reads this bus's cell once a transfer
    // started by `write_raw_start` on this bus finishes, and every task path
    // reaches the cells through `spi_lock`, which panics on an empty cell
    // rather than racing it — so the caller's own first transfer comes after
    // this.
    match spi_id {
        // SAFETY: the argument above, for bus 0's cells.
        0 => unsafe {
            SPI0_DONE.set(done);
            SPI0_LOCK.set(lock);
        },
        // SAFETY: the argument above, for bus 1's cells.
        _ => unsafe {
            SPI1_DONE.set(done);
            SPI1_LOCK.set(lock);
        },
    }

    // Mask all SPI interrupts (ISR enables them per-transaction)
    macro_rules! setup_irq {
        ($spi:expr, $irq:expr) => {{
            $spi.sspimsc().write(|w| w);
            crate::hal::chip::periph::enable_irq($irq);
        }};
    }
    match spi_id {
        0 => setup_irq!(&p.SPI0, pac::Interrupt::SPI0_IRQ),
        _ => setup_irq!(&p.SPI1, pac::Interrupt::SPI1_IRQ),
    }

    // Initialise DMA (idempotent — safe to call for each SPI peripheral)
    super::dma::init();
    gate.finish();
}

pub fn reconfigure(spi_id: u8, freq_hz: u32, mode: u32) {
    collect_own_write(spi_id);
    let lock = spi_lock(spi_id);
    let _ = lock.take(Duration::infinite());
    do_reconfigure(spi_id, freq_hz, mode);
    lock.give();
}

/// Write raw bytes from a Rust slice. Used by the display driver to stream pixel data.
pub fn write_raw(spi_id: u8, data: &[u8]) {
    write_raw_start(spi_id, data);
    write_raw_finish(spi_id);
}

/// The first half of [`write_raw`]: a small write completes here by polling,
/// a large one is handed to DMA and left running. In the second case the bus
/// lock stays taken and [`write_raw_finish`] must follow before anything
/// else uses the peripheral — the caller keeps `data` alive and unchanged
/// until then. Calling this with a write already pending collects it first.
pub fn write_raw_start(spi_id: u8, data: &[u8]) {
    collect_own_write(spi_id);
    if data.is_empty() {
        return;
    }

    let p = crate::hal::chip::periph::steal();

    let lock = spi_lock(spi_id);
    let _ = lock.take(Duration::infinite());

    // Small transfer fast path: polling. Under the lock like every other
    // path — a lock-free poll could put its bytes into a DMA band another
    // task has running, or run while `reconfigure` has the controller
    // disabled (docs/scheduling-audit-2026-09.md, F18). On the UI task the
    // lock is always free here: the panel driver collects its in-flight
    // band before every command, and `write_raw_finish` above released it.
    if data.len() <= SMALL_XFER_THRESHOLD {
        match spi_id {
            0 => poll_write_raw!(&p.SPI0, data),
            _ => poll_write_raw!(&p.SPI1, data),
        }
        lock.give();
        return;
    }

    // Mask SPI interrupts — DMA handles completion via DMA_IRQ_0
    match spi_id {
        0 => p.SPI0.sspimsc().write(|w| w),
        _ => p.SPI1.sspimsc().write(|w| w),
    }

    // Record the kind (and clear the ISR path's counters) so the wait in
    // `write_raw_finish` judges this write by the DMA channels, not by
    // whatever full-duplex transfer the ISR ran last.
    spi_state(spi_id).begin(
        SpiOp::DmaWrite,
        data.as_ptr(),
        core::ptr::null_mut(),
        data.len(),
        0,
    );

    super::dma::start_write(spi_id, data);
    write_owner(spi_id).store(picodroid_core::rtos::task_current(), Ordering::Relaxed);
    write_pending(spi_id).store(true, Ordering::Release);
}

/// The second half of [`write_raw`]: block until the DMA write
/// [`write_raw_start`] left running has been clocked out to the last bit,
/// then release the bus. Returns at once when nothing is pending.
pub fn write_raw_finish(spi_id: u8) {
    let pending = write_pending(spi_id);
    if !pending.load(Ordering::Acquire) {
        return;
    }
    pending.store(false, Ordering::Release);

    let p = crate::hal::chip::periph::steal();

    match spi_id {
        0 => finish_isr_xfer!(&p.SPI0, spi_id),
        _ => finish_isr_xfer!(&p.SPI1, spi_id),
    }

    spi_lock(spi_id).give();
}

/// Full-duplex transfer with raw Rust slices.
///
/// Clocks every byte of `tx` out. The bytes read back land in `rx` in
/// order until it is full; any received after that are discarded, and an
/// `rx` longer than `tx` keeps its tail untouched. So the lengths need not
/// match, and neither slice is ever indexed past its end: the ISR and the
/// polling path both stop storing at `rx.len()` ([`xfer::stores_rx`]).
/// Callers that want embedded-hal's "pad `tx` with zeros to fill `rx`" go
/// through `RpSpiBus::transfer`, which does that on top of this.
pub fn transfer_raw(spi_id: u8, tx: &[u8], rx: &mut [u8]) {
    collect_own_write(spi_id);
    if tx.is_empty() {
        return;
    }

    let p = crate::hal::chip::periph::steal();

    let lock = spi_lock(spi_id);
    let _ = lock.take(Duration::infinite());

    // Small transfer fast path: polling, under the lock (see `write_raw_start`).
    if tx.len() <= SMALL_XFER_THRESHOLD {
        match spi_id {
            0 => poll_transfer_raw!(&p.SPI0, tx, rx),
            _ => poll_transfer_raw!(&p.SPI1, tx, rx),
        }
        lock.give();
        return;
    }

    let state = spi_state(spi_id);
    state.begin(
        SpiOp::FullDuplex,
        tx.as_ptr(),
        rx.as_mut_ptr(),
        tx.len(),
        rx.len(),
    );

    match spi_id {
        0 => {
            start_isr_xfer!(&p.SPI0, state, tx);
            finish_isr_xfer!(&p.SPI0, spi_id);
        }
        _ => {
            start_isr_xfer!(&p.SPI1, state, tx);
            finish_isr_xfer!(&p.SPI1, spi_id);
        }
    }

    lock.give();
}
