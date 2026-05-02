// SPDX-License-Identifier: GPL-3.0-only
//! DMA engine for SPI write-only transfers.
//!
//! Uses two channels per SPI peripheral:
//!   - TX channel: streams data from memory → SPI TX FIFO (SSPDR)
//!   - RX drain channel: drains SPI RX FIFO → dummy byte (prevents overrun)
//!
//! Completion is signalled by the RX drain channel's IRQ (via DMA_IRQ_0),
//! which gives the existing `SPI_DONE` semaphore from `spi.rs`.

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

/// Dummy byte that the RX drain channel writes into (discarded).
static mut DMA_DUMMY_SINK: u8 = 0;

/// Guard for one-time init — only accessed from task context during boot.
static mut DMA_INITED: bool = false;

// ── Initialisation ─────────────────────────────────────────────────────────

/// Release the DMA block from reset, pre-configure channels for SPI,
/// and enable the DMA_IRQ_0 interrupt.  Safe to call more than once
/// (second+ calls are no-ops).
pub fn init() {
    unsafe {
        if DMA_INITED {
            return;
        }
        DMA_INITED = true;
    }

    let p = unsafe { pac::Peripherals::steal() };

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
    p.DMA
        .inte0()
        .write(|w| unsafe { w.bits((1 << CH_SPI0_RX) | (1 << CH_SPI1_RX)) });

    // Set DMA_IRQ_0 priority to 0x10 (FreeRTOS-safe) and unmask
    unsafe {
        let nvic_ipr = 0xE000_E400 as *mut u8;
        let irqn = pac::Interrupt::DMA_IRQ_0 as u8;
        nvic_ipr.add(irqn as usize).write_volatile(0x10);
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::DMA_IRQ_0);
    }
}

fn configure_tx_channel(dma: &pac::DMA, ch: usize, dreq: u8) {
    let c = dma.ch(ch);
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
    let p = unsafe { pac::Peripherals::steal() };

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
    let dummy = (&raw mut DMA_DUMMY_SINK) as u32;

    // Configure RX drain channel (start first — it will wait on DREQ)
    let rx = p.DMA.ch(rx_ch);
    rx.ch_read_addr().write(|w| unsafe { w.bits(sspdr_addr) });
    rx.ch_write_addr().write(|w| unsafe { w.bits(dummy) });
    rx.ch_trans_count().write(|w| unsafe { w.bits(len) });
    rx.ch_ctrl_trig().modify(|_, w| w.en().set_bit());

    // Configure TX channel
    let tx = p.DMA.ch(tx_ch);
    tx.ch_read_addr().write(|w| unsafe { w.bits(src) });
    tx.ch_write_addr().write(|w| unsafe { w.bits(sspdr_addr) });
    tx.ch_trans_count().write(|w| unsafe { w.bits(len) });
    tx.ch_ctrl_trig().modify(|_, w| w.en().set_bit());

    cortex_m::asm::dsb();
}

/// Abort any in-progress DMA transfer for the given SPI peripheral.
/// Called from the timeout recovery path in `spi.rs`.
pub fn abort(spi_id: u8) {
    let p = unsafe { pac::Peripherals::steal() };

    let (tx_ch, rx_ch) = match spi_id {
        0 => (CH_SPI0_TX, CH_SPI0_RX),
        _ => (CH_SPI1_TX, CH_SPI1_RX),
    };

    let mask = (1u32 << tx_ch) | (1u32 << rx_ch);

    // Request abort for both channels
    p.DMA.chan_abort().write(|w| unsafe { w.bits(mask) });

    // Wait for abort to complete (channels become not busy)
    while p.DMA.ch(tx_ch).ch_ctrl_trig().read().busy().bit_is_set() {}
    while p.DMA.ch(rx_ch).ch_ctrl_trig().read().busy().bit_is_set() {}

    // Clear any pending DMA interrupts for these channels
    p.DMA.ints0().write(|w| unsafe { w.bits(mask) });

    // Disable SPI DMA requests
    match spi_id {
        0 => p.SPI0.sspdmacr().write(|w| unsafe { w.bits(0) }),
        _ => p.SPI1.sspdmacr().write(|w| unsafe { w.bits(0) }),
    }
}

// ── DMA completion ISR ─────────────────────────────────────────────────────

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn DMA_IRQ_0() {
    let p = unsafe { pac::Peripherals::steal() };
    let mut ctx = InterruptContext::new();
    let ints = p.DMA.ints0().read().bits();

    // SPI0 RX drain channel completed
    if ints & (1 << CH_SPI0_RX) != 0 {
        p.DMA.ints0().write(|w| unsafe { w.bits(1 << CH_SPI0_RX) }); // W1C
        p.SPI0.sspdmacr().write(|w| unsafe { w.bits(0) });
        if let Some(sem) = unsafe { (*SPI0_DONE.0.get()).as_ref() } {
            sem.give_from_isr(&mut ctx);
        }
    }

    // SPI1 RX drain channel completed
    if ints & (1 << CH_SPI1_RX) != 0 {
        p.DMA.ints0().write(|w| unsafe { w.bits(1 << CH_SPI1_RX) }); // W1C
        p.SPI1.sspdmacr().write(|w| unsafe { w.bits(0) });
        if let Some(sem) = unsafe { (*SPI1_DONE.0.get()).as_ref() } {
            sem.give_from_isr(&mut ctx);
        }
    }

    // ctx drops here → freertos_rs_isr_yield if a higher-priority task woke
}
