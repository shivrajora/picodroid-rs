// SPDX-License-Identifier: GPL-3.0-only
//! PIO-based gSPI transport for the CYW43439 (Pico 2 W).
//!
//! Replaces the bit-banged transport (`port/net/cyw43_bus_spi.c`, deleted):
//! the PIO state machine clocks the bus from its own divider and performs the
//! half-duplex pin turnaround inside the program, so bus timing is independent
//! of what either CPU core executes — the property that lets the cyw43 task
//! run on core 1 while the JVM loads core 0 (see
//! `docs/designs/cyw43-pio-transport.md`).
//!
//! Semantics are ported from pico-sdk's `cyw43_bus_pio_spi.c` (program
//! `spi_gap01_sample0`, the default at these clock rates), which is the
//! normative reference for the vendored driver's expectations. The driver
//! calls exactly two transfer shapes:
//!
//!   write:  (tx, N, NULL, 0)  — host drives all N bytes
//!   read:   (buf, N, buf, N)  — command is the first 4 bytes, the chip
//!                               drives bytes [4..N); rx[0..4) is zeroed
//!
//! All lengths are multiples of 4. F1 response-delay padding is inside N
//! (`SPI_RESP_DELAY_F1` = 16, programmed by the driver); no dummy clocks are
//! ever added here.

use core::ffi::c_void;
use core::sync::atomic::{compiler_fence, Ordering};

use rp235x_hal::pac;

extern "C" {
    fn cyw43_delay_ms(ms: u32);
}

// ── Pins (must match CYW43_PIN_WL_* in cyw43_configport.h) ─────────────────

const PIN_WL_ON: u8 = 23; // WL_REG_ON, SIO output
const PIN_WL_D: u8 = 24; // shared DATA + active-high HOST_WAKE
const PIN_WL_CS: u8 = 25; // SIO output, active low
const PIN_WL_CLK: u8 = 29; // PIO side-set

const FUNCSEL_SIO: u8 = 5;
const FUNCSEL_PIO0: u8 = 6;

// ── PIO/DMA resources ──────────────────────────────────────────────────────

/// State machine 0 of PIO0; the whole block is otherwise unused, so the
/// program lives at instruction offset 0 (no relocation).
const SM: usize = 0;

/// SM clock divider. 150 MHz / 2 = 75 MHz SM clock; one bit costs two SM
/// cycles, so the bus runs at 37.5 MHz — pico-sdk's shipping rate on RP2350
/// (chip max is 50 MHz). If the board ever proves marginal here, fall back
/// to 3 (25 MHz) / 4 (18.75 MHz) before suspecting anything else.
const CLKDIV_INT: u16 = 2;

/// DMA channels 0-3 are statically owned by the display path (`dma.rs`);
/// these two run IRQ-quiet and never touch INTE0.
const DMA_CH_TX: usize = 4;
const DMA_CH_RX: usize = 5;

/// PIO0 SM0 DREQs on RP2350 (datasheet §12.6.4.1).
const TREQ_PIO0_TX0: u8 = 0;
const TREQ_PIO0_RX0: u8 = 4;

// ── PIO program (pico-sdk `spi_gap01_sample0`) ─────────────────────────────
//
//   0 lp:      out pins, 1     side 0   ; CLK low,  drive next TX bit
//   1          jmp x-- lp      side 1   ; CLK high, chip latches on rise
//   2 lp1_end: set pindirs, 0  side 0   ; CLK low,  DATA -> input (turnaround)
//   3          nop             side 1   ; CLK high, turnaround slack
//   4 lp2:     in pins, 1      side 0   ; CLK low,  sample BEFORE the pulse
//   5          jmp y-- lp2     side 1   ; CLK high
//   6 end:
//
// Side-set (1 bit, non-optional) drives CLK on every instruction — including
// stalled ones, which is what parks CLK low when the OSR runs dry at the end
// of a transfer. X/Y are loaded with count-1 (`jmp x--` tests then
// decrements). OUT/IN/SET all map to the single shared DATA pin.

/// Wrap top for write-only transfers: the 2-instruction out-loop.
const OFFSET_LP1_END: u8 = 2;
/// Wrap top for write+read transfers: the whole program.
const OFFSET_END: u8 = 6;

// Instructions injected via SM_INSTR while configuring a transfer. All have
// delay/side bits 0, so each exec also drives CLK low (side-set applies to
// exec'd instructions too).
const INSTR_OUT_X_32: u16 = 0x6020; // out x, 32   (bit-count word -> X)
const INSTR_OUT_Y_32: u16 = 0x6040; // out y, 32
const INSTR_JMP_START: u16 = 0x0000; // jmp 0
const INSTR_SET_PINDIRS_OUT: u16 = 0xE081; // set pindirs, 1  (DATA -> output)
const INSTR_SET_PINDIRS_IN: u16 = 0xE080; // set pindirs, 0  (DATA -> input)
const INSTR_MOV_PINS_NULL: u16 = 0xA003; // mov pins, null  (park OUT reg low)

/// One-time init guard — only touched from the cyw43 task during bring-up.
static mut PIO_INITED: bool = false;

/// Aligned bounce buffer for the two boot-time swap-register transfers,
/// whose 8-byte buffers live unaligned on the C stack (DMA runs 32-bit).
#[repr(align(4))]
struct Bounce([u8; 16]);
static mut BOUNCE: Bounce = Bounce([0; 16]);

// ── Register helpers ───────────────────────────────────────────────────────

fn steal() -> pac::Peripherals {
    unsafe { pac::Peripherals::steal() }
}

fn pin_funcsel(p: &pac::Peripherals, pin: u8, funcsel: u8) {
    p.IO_BANK0
        .gpio(pin as usize)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(funcsel) });
}

fn sm_exec(p: &pac::Peripherals, instr: u16) {
    p.PIO0
        .sm(SM)
        .sm_instr()
        .write(|w| unsafe { w.sm0_instr().bits(instr) });
}

fn sm_set_enabled(p: &pac::Peripherals, en: bool) {
    p.PIO0.ctrl().modify(|r, w| unsafe {
        let cur = r.sm_enable().bits();
        w.sm_enable().bits(if en {
            cur | (1 << SM)
        } else {
            cur & !(1 << SM)
        })
    });
}

/// Restart the SM (clears ISR/OSR, shift counters, latched delay/side-set
/// state — X/Y survive) and its clock divider phase.
fn sm_restart(p: &pac::Peripherals) {
    p.PIO0.ctrl().modify(|_, w| unsafe {
        w.sm_restart().bits(1 << SM);
        w.clkdiv_restart().bits(1 << SM)
    });
}

/// Per-transfer wrap selection is how one program serves both transfer
/// shapes (write-only wraps inside the out-loop and never reaches the
/// turnaround) — the reason this module drives the PAC directly instead of
/// rp235x-hal's typed PIO API, which cannot rewrite EXECCTRL at runtime.
fn set_wrap(p: &pac::Peripherals, top: u8) {
    p.PIO0.sm(SM).sm_execctrl().modify(|_, w| unsafe {
        w.wrap_bottom().bits(0);
        w.wrap_top().bits(top)
    });
}

/// Flush both FIFOs (toggling FJOIN_RX is the documented idiom).
fn clear_fifos(p: &pac::Peripherals) {
    let shiftctrl = p.PIO0.sm(SM).sm_shiftctrl();
    shiftctrl.modify(|_, w| w.fjoin_rx().set_bit());
    shiftctrl.modify(|_, w| w.fjoin_rx().clear_bit());
}

/// Push a word to the TX FIFO and move it into a scratch register via an
/// injected `out` — the standard trick for loading a >5-bit immediate.
fn load_scratch(p: &pac::Peripherals, out_instr: u16, value: u32) {
    p.PIO0.txf(SM).write(|w| unsafe { w.bits(value) });
    sm_exec(p, out_instr);
}

/// Spin until the channel finishes. Bounded: the longest legal transfer
/// (~2 KB at 37.5 MHz) takes ~450 us; the bound is >100x that, so a timeout
/// only fires on a genuine misconfiguration and surfaces as a -1 transfer
/// error instead of a wedged cyw43 task.
fn wait_dma_done(p: &pac::Peripherals, ch: usize) -> Result<(), ()> {
    let mut spins: u32 = 0;
    while p.DMA.ch(ch).ch_ctrl_trig().read().busy().bit_is_set() {
        spins += 1;
        if spins > 20_000_000 {
            return Err(());
        }
        core::hint::spin_loop();
    }
    Ok(())
}

fn abort_dma(p: &pac::Peripherals) {
    let mask = (1u32 << DMA_CH_TX) | (1u32 << DMA_CH_RX);
    p.DMA.chan_abort().write(|w| unsafe { w.bits(mask) });
    while p
        .DMA
        .ch(DMA_CH_TX)
        .ch_ctrl_trig()
        .read()
        .busy()
        .bit_is_set()
    {}
    while p
        .DMA
        .ch(DMA_CH_RX)
        .ch_ctrl_trig()
        .read()
        .busy()
        .bit_is_set()
    {}
}

/// Arm one DMA channel and trigger it. 32-bit transfers with byte-swap:
/// buffers are byte arrays in wire order (MSB-first per byte), and a
/// little-endian word load would otherwise reverse them.
#[allow(clippy::too_many_arguments)]
fn dma_start(
    p: &pac::Peripherals,
    ch: usize,
    read_addr: u32,
    write_addr: u32,
    words: u32,
    treq: u8,
    incr_read: bool,
    incr_write: bool,
) {
    let c = p.DMA.ch(ch);
    c.ch_read_addr().write(|w| unsafe { w.bits(read_addr) });
    c.ch_write_addr().write(|w| unsafe { w.bits(write_addr) });
    c.ch_trans_count().write(|w| unsafe { w.bits(words) });
    // Writing CTRL_TRIG with EN set starts the channel.
    c.ch_ctrl_trig().write(|w| unsafe {
        w.data_size().size_word();
        w.incr_read().bit(incr_read);
        w.incr_write().bit(incr_write);
        w.treq_sel().bits(treq);
        w.chain_to().bits(ch as u8); // self = no chaining
        w.bswap().set_bit();
        w.irq_quiet().set_bit(); // never raises DMA_IRQ_0 (display owns it)
        w.en().set_bit()
    });
}

// ── Port entry points (cyw43_spi.h) ────────────────────────────────────────

/// One-time PIO0/DMA setup. The vendored driver calls this before
/// `cyw43_spi_gpio_setup`/`cyw43_spi_reset`; all three are idempotent.
#[no_mangle]
pub extern "C" fn cyw43_spi_init(_self: *mut c_void) -> i32 {
    unsafe {
        if PIO_INITED {
            return 0;
        }
        PIO_INITED = true;
    }
    let p = steal();

    // Release PIO0 (and DMA — the display path normally does this, but do
    // not depend on init order) from reset.
    p.RESETS.reset().modify(|_, w| w.pio0().clear_bit());
    while p.RESETS.reset_done().read().pio0().bit_is_clear() {}
    p.RESETS.reset().modify(|_, w| w.dma().clear_bit());
    while p.RESETS.reset_done().read().dma().bit_is_clear() {}

    // Assemble and load the program at offset 0. `pio_asm!` runs the
    // assembler at compile time; this loop is just a copy.
    let asm = pio::pio_asm!(
        ".side_set 1",
        "lp:",
        "    out pins, 1     side 0",
        "    jmp x-- lp      side 1",
        "public lp1_end:",
        "    set pindirs, 0  side 0",
        "    nop             side 1",
        "lp2:",
        "    in pins, 1      side 0",
        "    jmp y-- lp2     side 1",
        "public end:",
    );
    debug_assert_eq!(asm.public_defines.lp1_end, OFFSET_LP1_END as i32);
    debug_assert_eq!(asm.public_defines.end, OFFSET_END as i32);
    for (i, instr) in asm.program.code.iter().enumerate() {
        p.PIO0
            .instr_mem(i)
            .write(|w| unsafe { w.bits(*instr as u32) });
    }

    let sm = p.PIO0.sm(SM);
    sm.sm_clkdiv().write(|w| unsafe {
        w.int().bits(CLKDIV_INT);
        w.frac().bits(0)
    });
    sm.sm_execctrl().write(|w| unsafe {
        w.wrap_bottom().bits(0);
        w.wrap_top().bits(OFFSET_END - 1);
        w.side_en().clear_bit(); // side-set is non-optional
        w.side_pindir().clear_bit()
    });
    // Shift LEFT in both directions = MSB-first on the wire; thresholds 0
    // encode 32. The SM never executes push/pull — auto machinery + DMA only.
    sm.sm_shiftctrl().write(|w| unsafe {
        w.autopull().set_bit();
        w.pull_thresh().bits(0);
        w.autopush().set_bit();
        w.push_thresh().bits(0);
        w.in_shiftdir().clear_bit();
        w.out_shiftdir().clear_bit()
    });
    sm.sm_pinctrl().write(|w| unsafe {
        w.out_base().bits(PIN_WL_D);
        w.out_count().bits(1);
        w.in_base().bits(PIN_WL_D);
        w.set_base().bits(PIN_WL_D);
        w.set_count().bits(1);
        w.sideset_base().bits(PIN_WL_CLK);
        w.sideset_count().bits(1)
    });

    // Bypass the 2-flop input synchroniser on DATA. Mandatory: `in pins`
    // samples the bit the chip presents *before* the clock pulse, and the
    // synchroniser would deliver it two SM cycles (one full bit) late.
    p.PIO0
        .input_sync_bypass()
        .modify(|r, w| unsafe { w.bits(r.bits() | (1 << PIN_WL_D)) });

    // Pads. DATA: input enable, schmitt, pull-DOWN — the line doubles as the
    // active-high host-wake, and a pull-up fakes "work pending" forever.
    // CLK: 12 mA drive + fast slew (pico-sdk does this for the clock only).
    // ISO must be cleared on RP2350 or the pad stays isolated.
    p.PADS_BANK0.gpio(PIN_WL_D as usize).write(|w| {
        w.iso().clear_bit();
        w.ie().set_bit();
        w.od().clear_bit();
        w.schmitt().set_bit();
        w.pue().clear_bit();
        w.pde().set_bit()
    });
    p.PADS_BANK0.gpio(PIN_WL_CLK as usize).write(|w| {
        w.iso().clear_bit();
        w.ie().set_bit();
        w.od().clear_bit();
        w.drive()._12m_a();
        w.slewfast().set_bit()
    });

    // Give CLK to the SM as an output driven low. SET temporarily maps to
    // CLK for the pindir exec, then the real mapping is restored and DATA is
    // parked as input with its output latch low (`mov pins, null`), ready
    // for the first turnaround.
    sm.sm_pinctrl().modify(|_, w| unsafe {
        w.set_base().bits(PIN_WL_CLK);
        w.set_count().bits(1)
    });
    sm_exec(&p, INSTR_SET_PINDIRS_OUT);
    sm.sm_pinctrl().modify(|_, w| unsafe {
        w.set_base().bits(PIN_WL_D);
        w.set_count().bits(1)
    });
    sm_exec(&p, INSTR_SET_PINDIRS_IN);
    sm_exec(&p, INSTR_MOV_PINS_NULL);

    pin_funcsel(&p, PIN_WL_D, FUNCSEL_PIO0);
    pin_funcsel(&p, PIN_WL_CLK, FUNCSEL_PIO0);

    // SM stays disabled between transfers.
    0
}

#[no_mangle]
pub extern "C" fn cyw43_spi_deinit(_self: *mut c_void) {
    let p = steal();
    sm_set_enabled(&p, false);
    p.SIO
        .gpio_out_clr()
        .write(|w| unsafe { w.bits(1u32 << PIN_WL_ON) });
}

/// SIO-controlled pins: REG_ON and CS outputs (CS deasserted high); the PIO
/// pins are (re-)asserted to their function. Idempotent.
#[no_mangle]
pub extern "C" fn cyw43_spi_gpio_setup() {
    let p = steal();

    for pin in [PIN_WL_ON, PIN_WL_CS] {
        pin_funcsel(&p, pin, FUNCSEL_SIO);
        p.PADS_BANK0.gpio(pin as usize).write(|w| {
            w.iso().clear_bit();
            w.ie().set_bit();
            w.od().clear_bit();
            w.schmitt().set_bit()
        });
    }
    p.SIO
        .gpio_out_set()
        .write(|w| unsafe { w.bits(1u32 << PIN_WL_CS) }); // CS deasserted
    p.SIO
        .gpio_oe_set()
        .write(|w| unsafe { w.bits((1u32 << PIN_WL_ON) | (1u32 << PIN_WL_CS)) });

    pin_funcsel(&p, PIN_WL_D, FUNCSEL_PIO0);
    pin_funcsel(&p, PIN_WL_CLK, FUNCSEL_PIO0);
}

/// Power-cycle the CYW43439, holding DATA driven low across the WL_REG_ON
/// rise (same as the bit-bang and pico-sdk). 250 ms post-power-on settle:
/// 50 ms is not reliably enough for first bus access.
#[no_mangle]
pub extern "C" fn cyw43_spi_reset() {
    let p = steal();

    pin_funcsel(&p, PIN_WL_D, FUNCSEL_SIO);
    p.SIO
        .gpio_out_clr()
        .write(|w| unsafe { w.bits(1u32 << PIN_WL_D) });
    p.SIO
        .gpio_oe_set()
        .write(|w| unsafe { w.bits(1u32 << PIN_WL_D) });

    p.SIO
        .gpio_out_clr()
        .write(|w| unsafe { w.bits(1u32 << PIN_WL_ON) });
    unsafe { cyw43_delay_ms(20) };
    p.SIO
        .gpio_out_set()
        .write(|w| unsafe { w.bits(1u32 << PIN_WL_ON) });
    unsafe { cyw43_delay_ms(250) };

    p.SIO
        .gpio_oe_clr()
        .write(|w| unsafe { w.bits(1u32 << PIN_WL_D) });
    pin_funcsel(&p, PIN_WL_D, FUNCSEL_PIO0);
}

/// The driver only ever requests polarity 0 (defensive re-sync after the
/// bus-control write); the program is CPOL=0 by construction.
#[no_mangle]
pub extern "C" fn cyw43_spi_set_polarity(_self: *mut c_void, _pol: i32) {}

/// gSPI transaction. No PRIMASK guard, deliberately: the SM and DMA complete
/// the frame autonomously if this task is preempted mid-transfer — the whole
/// point of the PIO port. Worst case a preemption delays CS deassert past an
/// already-completed frame, which the chip tolerates (CS framing, clock
/// parked low).
#[no_mangle]
pub unsafe extern "C" fn cyw43_spi_transfer(
    _self: *mut c_void,
    tx: *const u8,
    tx_length: usize,
    rx: *mut u8,
    rx_length: usize,
) -> i32 {
    if tx.is_null() || tx_length < 4 || !tx_length.is_multiple_of(4) {
        return -1;
    }
    let is_read = !rx.is_null();
    // The driver emits exactly two shapes (see module docs). If either
    // assert ever fires the vendored driver changed shape — revisit the
    // transfer arithmetic, do not silently adapt.
    if is_read && (!core::ptr::eq(rx, tx) || rx_length != tx_length || rx_length <= 4) {
        return -1;
    }

    // DMA runs 32-bit; bounce the (only) unaligned callers — the two 8-byte
    // boot-time swap-register transfers on the C stack.
    let mut bounced = false;
    let mut tx_ptr = tx;
    if (tx as usize) & 3 != 0 {
        if tx_length > 16 {
            return -1;
        }
        let b = &raw mut BOUNCE;
        core::ptr::copy_nonoverlapping(tx, (*b).0.as_mut_ptr(), tx_length);
        tx_ptr = (*b).0.as_ptr();
        bounced = true;
    }

    let p = steal();

    // Reset may have flipped DATA to SIO; re-assert cheap and always.
    pin_funcsel(&p, PIN_WL_D, FUNCSEL_PIO0);
    pin_funcsel(&p, PIN_WL_CLK, FUNCSEL_PIO0);

    // CS low — frame start.
    p.SIO
        .gpio_out_clr()
        .write(|w| unsafe { w.bits(1u32 << PIN_WL_CS) });

    sm_set_enabled(&p, false);

    let result = if is_read {
        // Command = first 4 bytes; the chip drives bytes [4..N).
        set_wrap(&p, OFFSET_END - 1);
        clear_fifos(&p);
        sm_exec(&p, INSTR_SET_PINDIRS_OUT);
        sm_restart(&p);
        load_scratch(&p, INSTR_OUT_X_32, 32 - 1);
        load_scratch(&p, INSTR_OUT_Y_32, (rx_length as u32 - 4) * 8 - 1);
        sm_exec(&p, INSTR_JMP_START);

        dma_start(
            &p,
            DMA_CH_TX,
            tx_ptr as u32,
            p.PIO0.txf(SM).as_ptr() as u32,
            1,
            TREQ_PIO0_TX0,
            true,
            false,
        );
        let rx_target = if bounced {
            (&raw mut BOUNCE) as u32 + 4
        } else {
            rx as u32 + 4
        };
        dma_start(
            &p,
            DMA_CH_RX,
            p.PIO0.rxf(SM).as_ptr() as u32,
            rx_target,
            (rx_length as u32 / 4) - 1,
            TREQ_PIO0_RX0,
            false,
            true,
        );
        cortex_m::asm::dsb();
        sm_set_enabled(&p, true);

        let ok = wait_dma_done(&p, DMA_CH_TX).and(wait_dma_done(&p, DMA_CH_RX));
        compiler_fence(Ordering::SeqCst);
        match ok {
            Ok(()) => {
                if bounced {
                    let b = &raw const BOUNCE;
                    core::ptr::copy_nonoverlapping(
                        (*b).0.as_ptr().add(4),
                        rx.add(4),
                        rx_length - 4,
                    );
                }
                // pico-sdk parity: the command-phase region of rx holds
                // garbage the caller must never see.
                core::ptr::write_bytes(rx, 0, 4);
                0
            }
            Err(()) => {
                abort_dma(&p);
                -1
            }
        }
    } else {
        // Write-only: wrap inside the out-loop; the turnaround and read
        // phase never execute, so DATA stays an output until the tail.
        set_wrap(&p, OFFSET_LP1_END - 1);
        clear_fifos(&p);
        sm_exec(&p, INSTR_SET_PINDIRS_OUT);
        sm_restart(&p);
        load_scratch(&p, INSTR_OUT_X_32, (tx_length as u32) * 8 - 1);
        sm_exec(&p, INSTR_JMP_START);

        dma_start(
            &p,
            DMA_CH_TX,
            tx_ptr as u32,
            p.PIO0.txf(SM).as_ptr() as u32,
            tx_length as u32 / 4,
            TREQ_PIO0_TX0,
            true,
            false,
        );
        cortex_m::asm::dsb();
        sm_set_enabled(&p, true);

        // DMA done only means the FIFO was filled; the frame has left the
        // shifter when the SM stalls on `out` again. W1C TXSTALL, then wait
        // for it to re-assert (bounded like the DMA wait).
        let mut ok = wait_dma_done(&p, DMA_CH_TX);
        if ok.is_ok() {
            p.PIO0
                .fdebug()
                .write(|w| unsafe { w.txstall().bits(1 << SM) });
            let mut spins: u32 = 0;
            while p.PIO0.fdebug().read().txstall().bits() & (1 << SM) == 0 {
                spins += 1;
                if spins > 20_000_000 {
                    ok = Err(());
                    break;
                }
                core::hint::spin_loop();
            }
        }
        match ok {
            Ok(()) => 0,
            Err(()) => {
                abort_dma(&p);
                -1
            }
        }
    };

    // Tail: park DATA as input (host-wake sensing between frames) with its
    // output latch low for the next turnaround, CS high, ~100 ns settle
    // before the driver samples host-wake (pico-sdk's IRQ_SAMPLE_DELAY_NS).
    sm_set_enabled(&p, false);
    sm_exec(&p, INSTR_SET_PINDIRS_IN);
    sm_exec(&p, INSTR_MOV_PINS_NULL);
    p.SIO
        .gpio_out_set()
        .write(|w| unsafe { w.bits(1u32 << PIN_WL_CS) });
    for _ in 0..16 {
        core::hint::spin_loop();
    }

    result
}
