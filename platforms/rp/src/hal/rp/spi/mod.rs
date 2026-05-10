// SPDX-License-Identifier: GPL-3.0-only
pub mod protocol;

use core::cell::UnsafeCell;

use freertos_rust::{Duration, InterruptContext, Semaphore};
use pico_jvm::array_heap::ArrayHeap;

use protocol::{
    clock_divisors, SpiOp, SpiXferState, MAX_STAGING_LEN, SMALL_XFER_THRESHOLD, SPI_FIFO_DEPTH,
};

use super::clock::PCLK_HZ;

// ── Per-peripheral statics ───────────────────────────────────────────────────

struct XferCell(UnsafeCell<SpiXferState>);
unsafe impl Sync for XferCell {}

pub(crate) struct SemCell(pub(crate) UnsafeCell<Option<Semaphore>>);
unsafe impl Sync for SemCell {}

static SPI0_STATE: XferCell = XferCell(UnsafeCell::new(SpiXferState::new()));
static SPI1_STATE: XferCell = XferCell(UnsafeCell::new(SpiXferState::new()));

pub(crate) static SPI0_DONE: SemCell = SemCell(UnsafeCell::new(None));
pub(crate) static SPI1_DONE: SemCell = SemCell(UnsafeCell::new(None));

static SPI0_LOCK: SemCell = SemCell(UnsafeCell::new(None));
static SPI1_LOCK: SemCell = SemCell(UnsafeCell::new(None));

fn spi_state(id: u8) -> &'static mut SpiXferState {
    unsafe {
        &mut *match id {
            0 => SPI0_STATE.0.get(),
            _ => SPI1_STATE.0.get(),
        }
    }
}

fn spi_done(id: u8) -> &'static Semaphore {
    unsafe {
        match id {
            0 => (*SPI0_DONE.0.get()).as_ref(),
            _ => (*SPI1_DONE.0.get()).as_ref(),
        }
        .expect("SPI done semaphore not initialised")
    }
}

fn spi_lock(id: u8) -> &'static Semaphore {
    unsafe {
        match id {
            0 => (*SPI0_LOCK.0.get()).as_ref(),
            _ => (*SPI1_LOCK.0.get()).as_ref(),
        }
        .expect("SPI lock semaphore not initialised")
    }
}

// ── ISR ──────────────────────────────────────────────────────────────────────

macro_rules! spi_isr_body {
    ($spi:expr, $state_static:expr, $done_static:expr) => {{
        let state = unsafe { &mut *$state_static.0.get() };
        let mut ctx = InterruptContext::new();

        // 1. Drain RX FIFO first (prevents overrun)
        while $spi.sspsr().read().rne().bit_is_set() && state.rx_idx < state.len {
            let byte = $spi.sspdr().read().data().bits() as u8;
            if state.op == SpiOp::FullDuplex {
                unsafe { *state.rx_ptr.add(state.rx_idx) = byte };
            }
            state.rx_idx += 1;
        }

        // 2. Check completion
        if state.rx_idx >= state.len {
            $spi.sspimsc().write(|w| unsafe { w.bits(0) });
            $spi.sspicr().write(|w| unsafe { w.bits(0x03) }); // clear RORIC + RTIC
            state.op = SpiOp::Idle;
            if let Some(sem) = unsafe { (*$done_static.0.get()).as_ref() } {
                sem.give_from_isr(&mut ctx);
            }
            return;
        }

        // 3. Refill TX FIFO
        while state.tx_idx < state.len && $spi.sspsr().read().tnf().bit_is_set() {
            let byte = unsafe { *state.tx_ptr.add(state.tx_idx) };
            $spi.sspdr()
                .write(|w| unsafe { w.data().bits(byte as u16) });
            state.tx_idx += 1;
        }

        // 4. All TX queued — disable TXIM, keep RTIM to catch final RX bytes
        if state.tx_idx >= state.len {
            $spi.sspimsc()
                .write(|w| w.rtim().set_bit().rorim().set_bit());
        }

        // 5. Clear receive timeout / overrun if they fired
        $spi.sspicr().write(|w| unsafe { w.bits(0x03) });

        // ctx drops here → freertos_rs_isr_yield if a higher-priority task woke
    }};
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn SPI0_IRQ() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    spi_isr_body!(&p.SPI0, SPI0_STATE, SPI0_DONE);
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn SPI1_IRQ() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    spi_isr_body!(&p.SPI1, SPI1_STATE, SPI1_DONE);
}

// ── Speed configuration ─────────────────────────────────────────────────────

macro_rules! apply_config {
    ($spi:expr, $freq_hz:expr, $mode:expr) => {{
        // 1. Disable the SSP controller before reconfiguring
        $spi.sspcr1().write(|w| unsafe { w.bits(0) });

        // 2. Clock prescale divisor (must be even, min 2)
        let (cpsdvsr, scr) = clock_divisors(PCLK_HZ, $freq_hz);
        $spi.sspcpsr()
            .write(|w| unsafe { w.cpsdvsr().bits(cpsdvsr) });

        // 3. SSPCR0: DSS=8-bit (0b0111), FRF=Motorola SPI (0b00),
        //    SPO=CPOL (mode bit 1), SPH=CPHA (mode bit 0), SCR in bits [15:8]
        let spo: u32 = (($mode >> 1) & 1) as u32;
        let sph: u32 = ($mode & 1) as u32;
        let cr0: u32 = ((scr as u32) << 8) | (sph << 7) | (spo << 6) | 0b0111;
        $spi.sspcr0().write(|w| unsafe { w.bits(cr0) });

        // 4. Re-enable: SSE=1, MS=0 (master mode)
        $spi.sspcr1().write(|w| w.sse().set_bit().ms().clear_bit());

        cortex_m::asm::dsb();
    }};
}

fn do_reconfigure(spi_id: u8, freq_hz: u32, mode: u32) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    match spi_id {
        0 => apply_config!(&p.SPI0, freq_hz, mode),
        _ => apply_config!(&p.SPI1, freq_hz, mode),
    }
}

// ── Polling helpers (small transfers) ────────────────────────────────────────

macro_rules! poll_write_raw {
    ($spi:expr, $data:expr) => {{
        for &byte in $data {
            while $spi.sspsr().read().tnf().bit_is_clear() {}
            $spi.sspdr()
                .write(|w| unsafe { w.data().bits(byte as u16) });
            while $spi.sspsr().read().rne().bit_is_clear() {}
            let _ = $spi.sspdr().read();
        }
        while $spi.sspsr().read().bsy().bit_is_set() {}
    }};
}

macro_rules! poll_transfer_raw {
    ($spi:expr, $tx:expr, $rx:expr) => {{
        for i in 0..$tx.len() {
            while $spi.sspsr().read().tnf().bit_is_clear() {}
            $spi.sspdr()
                .write(|w| unsafe { w.data().bits($tx[i] as u16) });
            while $spi.sspsr().read().rne().bit_is_clear() {}
            $rx[i] = $spi.sspdr().read().data().bits() as u8;
        }
        while $spi.sspsr().read().bsy().bit_is_set() {}
    }};
}

// ── ISR-driven transfer helpers ──────────────────────────────────────────────

macro_rules! start_isr_xfer {
    ($spi:expr, $state:expr) => {{
        // Seed TX FIFO (up to FIFO depth)
        let seed = $state.len.min(SPI_FIFO_DEPTH);
        for i in 0..seed {
            let byte = unsafe { *$state.tx_ptr.add(i) };
            $spi.sspdr()
                .write(|w| unsafe { w.data().bits(byte as u16) });
        }
        $state.tx_idx = seed;
        // Clear stale interrupts, enable TXIM + RTIM + RORIM
        $spi.sspicr().write(|w| unsafe { w.bits(0x03) }); // clear RORIC + RTIC
        $spi.sspimsc()
            .write(|w| w.txim().set_bit().rtim().set_bit().rorim().set_bit());
    }};
}

macro_rules! finish_isr_xfer {
    ($spi:expr, $spi_id:expr) => {{
        let done = spi_done($spi_id);
        if done.take(Duration::ms(5000)).is_err() {
            super::dma::abort($spi_id);
            $spi.sspimsc().write(|w| unsafe { w.bits(0) });
            while $spi.sspsr().read().rne().bit_is_set() {
                let _ = $spi.sspdr().read();
            }
            let state = spi_state($spi_id);
            state.op = SpiOp::Idle;
        }
        // Wait for last byte to finish shifting out
        while $spi.sspsr().read().bsy().bit_is_set() {}
    }};
}

// ── Public API ───────────────────────────────────────────────────────────────

/// True once `init(spi_id)` has run. Used to make `init` idempotent so display
/// init and `PeripheralManager.openSpi` can each request the same bus without
/// re-allocating semaphores.
fn is_initialised(spi_id: u8) -> bool {
    unsafe {
        match spi_id {
            0 => (*SPI0_LOCK.0.get()).is_some(),
            _ => (*SPI1_LOCK.0.get()).is_some(),
        }
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
    if is_initialised(spi_id) {
        return;
    }

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

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
            .write(|w| unsafe { w.funcsel().bits(1) }); // 1 = SPI
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

    match spi_id {
        0 => unsafe {
            *SPI0_DONE.0.get() = Some(done);
            *SPI0_LOCK.0.get() = Some(lock);
        },
        _ => unsafe {
            *SPI1_DONE.0.get() = Some(done);
            *SPI1_LOCK.0.get() = Some(lock);
        },
    }

    // Mask all SPI interrupts (ISR enables them per-transaction)
    macro_rules! setup_irq {
        ($spi:expr, $irq:expr) => {{
            $spi.sspimsc().write(|w| unsafe { w.bits(0) });
            unsafe {
                let nvic_ipr = 0xE000_E400 as *mut u8;
                let irqn = $irq as u8;
                nvic_ipr.add(irqn as usize).write_volatile(0x10);
                cortex_m::peripheral::NVIC::unmask($irq);
            }
        }};
    }
    match spi_id {
        0 => setup_irq!(&p.SPI0, pac::Interrupt::SPI0_IRQ),
        _ => setup_irq!(&p.SPI1, pac::Interrupt::SPI1_IRQ),
    }

    // Initialise DMA (idempotent — safe to call for each SPI peripheral)
    super::dma::init();
}

pub fn reconfigure(spi_id: u8, freq_hz: u32, mode: u32) {
    let lock = spi_lock(spi_id);
    let _ = lock.take(Duration::infinite());
    do_reconfigure(spi_id, freq_hz, mode);
    lock.give();
}

/// Write raw bytes from a Rust slice. Used by the display driver to stream pixel data.
pub fn write_raw(spi_id: u8, data: &[u8]) {
    if data.is_empty() {
        return;
    }

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    // Small transfer fast path: polling
    if data.len() <= SMALL_XFER_THRESHOLD {
        match spi_id {
            0 => poll_write_raw!(&p.SPI0, data),
            _ => poll_write_raw!(&p.SPI1, data),
        }
        return;
    }

    let lock = spi_lock(spi_id);
    let _ = lock.take(Duration::infinite());

    // Mask SPI interrupts — DMA handles completion via DMA_IRQ_0
    match spi_id {
        0 => p.SPI0.sspimsc().write(|w| unsafe { w.bits(0) }),
        _ => p.SPI1.sspimsc().write(|w| unsafe { w.bits(0) }),
    }

    super::dma::start_write(spi_id, data);

    match spi_id {
        0 => finish_isr_xfer!(&p.SPI0, spi_id),
        _ => finish_isr_xfer!(&p.SPI1, spi_id),
    }

    lock.give();
}

/// Full-duplex transfer with raw Rust slices.
pub fn transfer_raw(spi_id: u8, tx: &[u8], rx: &mut [u8]) {
    if tx.is_empty() {
        return;
    }

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    // Small transfer fast path: polling
    if tx.len() <= SMALL_XFER_THRESHOLD {
        match spi_id {
            0 => poll_transfer_raw!(&p.SPI0, tx, rx),
            _ => poll_transfer_raw!(&p.SPI1, tx, rx),
        }
        return;
    }

    let lock = spi_lock(spi_id);
    let _ = lock.take(Duration::infinite());

    let state = spi_state(spi_id);
    state.op = SpiOp::FullDuplex;
    state.tx_ptr = tx.as_ptr();
    state.rx_ptr = rx.as_mut_ptr();
    state.len = tx.len();
    state.tx_idx = 0;
    state.rx_idx = 0;

    match spi_id {
        0 => {
            start_isr_xfer!(&p.SPI0, state);
            finish_isr_xfer!(&p.SPI0, spi_id);
        }
        _ => {
            start_isr_xfer!(&p.SPI1, state);
            finish_isr_xfer!(&p.SPI1, spi_id);
        }
    }

    lock.give();
}

/// Full-duplex transfer via ArrayHeap. Returns len on success.
pub fn transfer(spi_id: u8, tx_idx: u16, rx_idx: u16, len: usize, arrays: &mut ArrayHeap) -> i32 {
    if len == 0 {
        return 0;
    }

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    let lock = spi_lock(spi_id);
    let _ = lock.take(Duration::infinite());

    if len > MAX_STAGING_LEN {
        lock.give();
        return -1;
    }

    let state = spi_state(spi_id);
    for i in 0..len {
        state.staging[i] = arrays.load(tx_idx, i).unwrap_or(0) as u8;
    }

    // Small transfer: polling with staging buffer
    if len <= SMALL_XFER_THRESHOLD {
        let mut rx_buf = [0u8; SMALL_XFER_THRESHOLD];
        match spi_id {
            0 => poll_transfer_raw!(&p.SPI0, &state.staging[..len], &mut rx_buf[..len]),
            _ => poll_transfer_raw!(&p.SPI1, &state.staging[..len], &mut rx_buf[..len]),
        }
        for (i, &b) in rx_buf[..len].iter().enumerate() {
            arrays.store(rx_idx, i, b as i32);
        }
        lock.give();
        return len as i32;
    }

    state.op = SpiOp::FullDuplex;
    state.tx_ptr = state.staging.as_ptr();
    state.rx_ptr = state.staging.as_mut_ptr();
    state.len = len;
    state.tx_idx = 0;
    state.rx_idx = 0;

    match spi_id {
        0 => {
            start_isr_xfer!(&p.SPI0, state);
            finish_isr_xfer!(&p.SPI0, spi_id);
        }
        _ => {
            start_isr_xfer!(&p.SPI1, state);
            finish_isr_xfer!(&p.SPI1, spi_id);
        }
    }

    let state = spi_state(spi_id);
    for i in 0..len {
        arrays.store(rx_idx, i, state.staging[i] as i32);
    }

    lock.give();
    len as i32
}

/// Write-only transfer via ArrayHeap. Returns len on success.
pub fn write(spi_id: u8, data_idx: u16, len: usize, arrays: &ArrayHeap) -> i32 {
    if len == 0 {
        return 0;
    }

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    let lock = spi_lock(spi_id);
    let _ = lock.take(Duration::infinite());

    if len > MAX_STAGING_LEN {
        lock.give();
        return -1;
    }

    let state = spi_state(spi_id);
    for i in 0..len {
        state.staging[i] = arrays.load(data_idx, i).unwrap_or(0) as u8;
    }

    // Small transfer: polling with staging buffer
    if len <= SMALL_XFER_THRESHOLD {
        match spi_id {
            0 => poll_write_raw!(&p.SPI0, &state.staging[..len]),
            _ => poll_write_raw!(&p.SPI1, &state.staging[..len]),
        }
        lock.give();
        return len as i32;
    }

    state.op = SpiOp::WriteOnly;
    state.tx_ptr = state.staging.as_ptr();
    state.rx_ptr = core::ptr::null_mut();
    state.len = len;
    state.tx_idx = 0;
    state.rx_idx = 0;

    match spi_id {
        0 => {
            start_isr_xfer!(&p.SPI0, state);
            finish_isr_xfer!(&p.SPI0, spi_id);
        }
        _ => {
            start_isr_xfer!(&p.SPI1, state);
            finish_isr_xfer!(&p.SPI1, spi_id);
        }
    }

    lock.give();
    len as i32
}
