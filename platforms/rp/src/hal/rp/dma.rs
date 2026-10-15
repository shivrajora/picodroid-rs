// SPDX-License-Identifier: GPL-3.0-only
//! DMA engine for SPI write-only transfers.
//!
//! Uses two channels per SPI peripheral:
//!   - TX channel: streams data from memory → SPI TX FIFO (SSPDR)
//!   - RX drain channel: drains SPI RX FIFO → dummy byte (prevents overrun)
//!
//! Completion is signalled by the RX drain channel's IRQ (via DMA_IRQ_0),
//! which gives the existing `SPI_DONE` semaphore from `spi.rs`.

#![deny(unsafe_op_in_unsafe_fn)]

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use freertos_rust::InterruptContext;

#[cfg(feature = "chip-rp2350")]
use rp235x_hal::pac;
#[cfg(feature = "chip-rp2040")]
use rp_pico::hal::pac;

use super::spi::{SPI0_DONE, SPI1_DONE};

// ── Static DMA channel assignments ─────────────────────────────────────────

const CH_SPI0_TX: usize = 0;
const CH_SPI0_RX: usize = 1;
const CH_SPI1_TX: usize = 2;
const CH_SPI1_RX: usize = 3;

/// Dummy byte that the RX drain channel writes into (discarded). Only the
/// DMA engine touches it; the atomic is for the address, not the access.
static DMA_DUMMY_SINK: AtomicU8 = AtomicU8::new(0);

/// Guard for one-time init — only accessed from task context during boot.
/// Load then store: one initialiser at a time (`spi::init_with_pins`'s
/// rule), and thumbv6m has no swap.
static DMA_INITED: AtomicBool = AtomicBool::new(false);

// ── Channel helpers (also used by `pio_spi`) ───────────────────────────────

/// Load a channel's read address, write address and transfer count, without
/// starting it: the caller triggers it afterwards with its own `CTRL_TRIG`
/// write, which picks the transfer size, the increments and the DREQ.
///
/// # Safety
///
/// Once the channel is triggered it reads `count` transfers from `read` and
/// writes them to `write`, in hardware, while the program runs on; the
/// caller's `CTRL_TRIG` settings give those the extents meant here. For as
/// long as it can (until it finishes, or is aborted and has gone not-busy):
/// - memory at `write` (when the address is memory, not a peripheral FIFO)
///   must be valid for those writes and touched by nothing else, and
/// - memory at `read` must stay valid for those reads.
///
/// `start_write` below spells the same three writes out: through this
/// helper, LLVM inlined `spi::write_raw_start` differently (+16 B on the
/// RP2040, −8 B on the RP2350), and that path's image is kept identical.
#[cfg(network_cyw43)]
#[inline(always)]
pub(crate) unsafe fn program(ch: &pac::dma::CH, read: u32, write: u32, count: u32) {
    // SAFETY: an address and a count are plain register values; what the
    // channel then does with them is this function's contract.
    unsafe {
        ch.ch_read_addr().write(|w| w.bits(read));
        ch.ch_write_addr().write(|w| w.bits(write));
        ch.ch_trans_count().write(|w| w.bits(count));
    }
}

/// Ask the channels in `mask` to abort. Returns at once: the caller waits
/// for them to go not-busy.
#[inline(always)]
pub(crate) fn request_abort(dma: &pac::DMA, mask: u32) {
    // SAFETY: CHAN_ABORT only stops channels, which ends the memory traffic
    // `program` was trusted with rather than starting any; every bit names a
    // channel (the RP2040's bits above 11 are ignored).
    dma.chan_abort().write(|w| unsafe { w.bits(mask) });
}

/// Acknowledge (write-1-to-clear) the `DMA_IRQ_0` interrupts in `mask`.
#[inline(always)]
pub(crate) fn ack_irq0(dma: &pac::DMA, mask: u32) {
    // SAFETY: INTS0 is write-1-to-clear: it drops pending interrupt flags
    // and touches no channel or memory.
    dma.ints0().write(|w| unsafe { w.bits(mask) });
}

/// Acknowledge (write-1-to-clear) the `DMA_IRQ_1` interrupts in `mask`.
#[cfg(network_cyw43)]
#[inline(always)]
pub(crate) fn ack_irq1(dma: &pac::DMA, mask: u32) {
    // SAFETY: as `ack_irq0`, for INTS1.
    dma.ints1().write(|w| unsafe { w.bits(mask) });
}

// ── Initialisation ─────────────────────────────────────────────────────────

/// Release the DMA block from reset, pre-configure channels for SPI,
/// and enable the DMA_IRQ_0 interrupt.  Safe to call more than once
/// (second+ calls are no-ops).
pub fn init() {
    if DMA_INITED.load(Ordering::Relaxed) {
        return;
    }
    DMA_INITED.store(true, Ordering::Relaxed);

    let p = crate::hal::chip::periph::steal();

    // Release DMA from reset
    p.RESETS.reset().modify(|_, w| w.dma().clear_bit());
    while p.RESETS.reset_done().read().dma().bit_is_clear() {}

    // ── SPI0 TX channel (CH0) ──────────────────────────────────────────
    configure_tx_channel(&p.DMA, CH_SPI0_TX, spi_dreq_tx(0));

    // ── SPI0 RX drain channel (CH1) ────────────────────────────────────
    configure_rx_channel(&p.DMA, CH_SPI0_RX, spi_dreq_rx(0));

    // ── SPI1 TX channel (CH2) ──────────────────────────────────────────
    configure_tx_channel(&p.DMA, CH_SPI1_TX, spi_dreq_tx(1));

    // ── SPI1 RX drain channel (CH3) ────────────────────────────────────
    configure_rx_channel(&p.DMA, CH_SPI1_RX, spi_dreq_rx(1));

    // Enable IRQ0 for both RX drain channels
    // SAFETY: routes channels 1 and 3 to DMA_IRQ_0, whose handler below
    // acts only on those two bits and gives semaphores through `SetOnce`
    // reads that tolerate an empty cell: sound for any bit pattern.
    p.DMA
        .inte0()
        .write(|w| unsafe { w.bits((1 << CH_SPI0_RX) | (1 << CH_SPI1_RX)) });

    // Set DMA_IRQ_0 priority to 0x10 (FreeRTOS-safe) and unmask
    crate::hal::chip::periph::enable_irq(pac::Interrupt::DMA_IRQ_0);
}

fn configure_tx_channel(dma: &pac::DMA, ch: usize, dreq: u8) {
    let c = dma.ch(ch);
    // SAFETY: TREQ_SEL and CHAIN_TO take raw numbers (a DREQ, this
    // channel), which `bits` masks to the field; EN stays clear, so nothing
    // is started — `start_write` arms the channel.
    c.ch_ctrl_trig().write(|w| unsafe {
        w.en().clear_bit();
        w.data_size().size_byte();
        w.incr_read().set_bit(); // read from memory buffer (increment)
        w.incr_write().clear_bit(); // write to fixed SPI SSPDR
        w.treq_sel().bits(dreq);
        w.chain_to().bits(ch as u8); // chain to self = no chaining
        w.irq_quiet().set_bit(); // TX channel does NOT raise IRQ
        w
    });
}

fn configure_rx_channel(dma: &pac::DMA, ch: usize, dreq: u8) {
    let c = dma.ch(ch);
    // SAFETY: as `configure_tx_channel`: raw DREQ and channel numbers, and
    // EN clear.
    c.ch_ctrl_trig().write(|w| unsafe {
        w.en().clear_bit();
        w.data_size().size_byte();
        w.incr_read().clear_bit(); // read from fixed SPI SSPDR
        w.incr_write().clear_bit(); // write to fixed dummy sink
        w.treq_sel().bits(dreq);
        w.chain_to().bits(ch as u8);
        w.irq_quiet().clear_bit(); // RX channel DOES raise completion IRQ
        w
    });
}

/// Return the DREQ number for SPI TX on the given SPI peripheral.
fn spi_dreq_tx(spi_id: u8) -> u8 {
    #[cfg(feature = "chip-rp2350")]
    {
        if spi_id == 0 {
            24
        } else {
            26
        } // SPI0_TX=24, SPI1_TX=26
    }
    #[cfg(feature = "chip-rp2040")]
    {
        if spi_id == 0 {
            16
        } else {
            18
        } // SPI0_TX=16, SPI1_TX=18
    }
}

/// Return the DREQ number for SPI RX on the given SPI peripheral.
fn spi_dreq_rx(spi_id: u8) -> u8 {
    #[cfg(feature = "chip-rp2350")]
    {
        if spi_id == 0 {
            25
        } else {
            27
        } // SPI0_RX=25, SPI1_RX=27
    }
    #[cfg(feature = "chip-rp2040")]
    {
        if spi_id == 0 {
            17
        } else {
            19
        } // SPI0_RX=17, SPI1_RX=19
    }
}

// ── Start a DMA write transfer ─────────────────────────────────────────────

/// Kick off a write-only DMA transfer for the given SPI peripheral.
/// The caller must hold the SPI lock.  Completion is signalled via
/// the SPI_DONE semaphore (same one the ISR path uses).
pub fn start_write(spi_id: u8, data: &[u8]) {
    let p = crate::hal::chip::periph::steal();

    let (tx_ch, rx_ch) = match spi_id {
        0 => (CH_SPI0_TX, CH_SPI0_RX),
        _ => (CH_SPI1_TX, CH_SPI1_RX),
    };

    // Get the SPI SSPDR address (data register)
    let sspdr_addr: u32 = match spi_id {
        0 => p.SPI0.sspdr().as_ptr() as u32,
        _ => p.SPI1.sspdr().as_ptr() as u32,
    };

    // Enable SPI DMA request signals
    match spi_id {
        0 => p
            .SPI0
            .sspdmacr()
            .write(|w| w.txdmae().set_bit().rxdmae().set_bit()),
        _ => p
            .SPI1
            .sspdmacr()
            .write(|w| w.txdmae().set_bit().rxdmae().set_bit()),
    }

    let len = data.len() as u32;
    let src = data.as_ptr() as u32;
    let dummy = DMA_DUMMY_SINK.as_ptr() as u32;

    // Configure RX drain channel (start first — it will wait on DREQ)
    let rx = p.DMA.ch(rx_ch);
    // SAFETY: `program`'s contract. The drain channel reads the SPI data
    // register and writes the one-byte static sink without incrementing
    // (`configure_rx_channel`): no memory but that static, which nothing
    // else reads or writes.
    unsafe {
        rx.ch_read_addr().write(|w| w.bits(sspdr_addr));
        rx.ch_write_addr().write(|w| w.bits(dummy));
        rx.ch_trans_count().write(|w| w.bits(len));
    }
    rx.ch_ctrl_trig().modify(|_, w| w.en().set_bit());

    // Configure TX channel
    let tx = p.DMA.ch(tx_ch);
    // SAFETY: `program`'s contract. The TX channel writes only the SPI
    // data register; from memory it reads `len` bytes at `src`, which is
    // `data`. A read cannot corrupt memory, and `write_raw_start`'s caller
    // keeps `data` alive and unchanged until `write_raw_finish` has seen
    // the channel retire, so the bytes clocked out are the ones asked for.
    unsafe {
        tx.ch_read_addr().write(|w| w.bits(src));
        tx.ch_write_addr().write(|w| w.bits(sspdr_addr));
        tx.ch_trans_count().write(|w| w.bits(len));
    }
    tx.ch_ctrl_trig().modify(|_, w| w.en().set_bit());

    cortex_m::asm::dsb();
}

/// Whether either of the SPI peripheral's channels is still busy. The wait
/// in `spi::write_raw_finish` asks this before believing a DMA write that
/// has not signalled is over.
pub fn busy(spi_id: u8) -> bool {
    let p = crate::hal::chip::periph::steal();
    let (tx_ch, rx_ch) = match spi_id {
        0 => (CH_SPI0_TX, CH_SPI0_RX),
        _ => (CH_SPI1_TX, CH_SPI1_RX),
    };
    p.DMA.ch(tx_ch).ch_ctrl_trig().read().busy().bit_is_set()
        || p.DMA.ch(rx_ch).ch_ctrl_trig().read().busy().bit_is_set()
}

/// Abort any in-progress DMA transfer for the given SPI peripheral.
/// Called from the timeout recovery path in `spi.rs`.
pub fn abort(spi_id: u8) {
    let p = crate::hal::chip::periph::steal();

    let (tx_ch, rx_ch) = match spi_id {
        0 => (CH_SPI0_TX, CH_SPI0_RX),
        _ => (CH_SPI1_TX, CH_SPI1_RX),
    };

    let mask = (1u32 << tx_ch) | (1u32 << rx_ch);

    // Request abort for both channels
    request_abort(&p.DMA, mask);

    // Wait for the abort to retire (the channels go not-busy). Normally a
    // few cycles; a channel wedged on a DREQ that never asserts (the RP2040
    // abort erratum) never clears, and this is the recovery path, so give
    // up loudly rather than hang the UI task in it.
    for ch in [tx_ch, rx_ch] {
        if picodroid_core::spin_until!(
            !p.DMA.ch(ch).ch_ctrl_trig().read().busy().bit_is_set(),
            1_000_000,
            "spi dma abort"
        )
        .is_err()
        {
            defmt::error!("dma: SPI{} channel {} did not abort", spi_id, ch);
        }
    }

    // Clear any pending DMA interrupts for these channels
    ack_irq0(&p.DMA, mask);

    // Disable SPI DMA requests (DMACR's reset value, 0)
    match spi_id {
        0 => p.SPI0.sspdmacr().write(|w| w),
        _ => p.SPI1.sspdmacr().write(|w| w),
    }
}

// ── DMA completion ISR ─────────────────────────────────────────────────────

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn DMA_IRQ_0() {
    let p = crate::hal::chip::periph::steal();
    let mut ctx = InterruptContext::new();
    let ints = p.DMA.ints0().read().bits();

    // SPI0 RX drain channel completed
    if ints & (1 << CH_SPI0_RX) != 0 {
        ack_irq0(&p.DMA, 1 << CH_SPI0_RX);
        p.SPI0.sspdmacr().write(|w| w); // reset value: DMA requests off
        if let Some(sem) = SPI0_DONE.get() {
            sem.give_from_isr(&mut ctx);
        }
    }

    // SPI1 RX drain channel completed
    if ints & (1 << CH_SPI1_RX) != 0 {
        ack_irq0(&p.DMA, 1 << CH_SPI1_RX);
        p.SPI1.sspdmacr().write(|w| w); // reset value: DMA requests off
        if let Some(sem) = SPI1_DONE.get() {
            sem.give_from_isr(&mut ctx);
        }
    }

    // ctx drops here → freertos_rs_isr_yield if a higher-priority task woke
}
