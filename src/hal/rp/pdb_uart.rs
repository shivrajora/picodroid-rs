use core::cell::UnsafeCell;

use freertos_rust::{Duration, InterruptContext, Queue};

// ── UART1 RX queue ────────────────────────────────────────────────────────────
//
// The ISR drains the UART1 RX FIFO into this queue.  pdb_task blocks on
// receive(), sleeping at zero CPU cost while no data arrives.
//
// SAFETY: initialized once in init() before the interrupt is enabled;
// read-only after that from the ISR and task.
struct QueueCell(UnsafeCell<Option<Queue<u8>>>);
unsafe impl Sync for QueueCell {}
static UART1_RX_QUEUE: QueueCell = QueueCell(UnsafeCell::new(None));

fn uart1_rx_queue() -> &'static Queue<u8> {
    unsafe {
        (*UART1_RX_QUEUE.0.get())
            .as_ref()
            .expect("UART1_RX_QUEUE not initialised")
    }
}

// ── UART1 RX ISR ─────────────────────────────────────────────────────────────

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn UART1_IRQ() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;

    let p = unsafe { pac::Peripherals::steal() };
    let mut ctx = InterruptContext::new();

    // Drain all bytes currently in the UART1 RX FIFO
    while p.UART1.uartfr().read().rxfe().bit_is_clear() {
        let byte = p.UART1.uartdr().read().data().bits();
        let _ = uart1_rx_queue().send_from_isr(&mut ctx, byte);
    }
    // ctx drops here, calling freertos_rs_isr_yield if a higher-priority task woke
}

// ── UART1 interrupt setup ─────────────────────────────────────────────────────

fn setup_uart1_rx_interrupt() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;

    let p = unsafe { pac::Peripherals::steal() };

    // Lower the RX FIFO trigger to 1/8 full (4 bytes for a 32-byte FIFO).
    p.UART1
        .uartifls()
        .modify(|_, w| unsafe { w.rxiflsel().bits(0b000) });

    // Enable both RXIM (FIFO threshold) and RTIM (receive timeout).
    p.UART1
        .uartimsc()
        .modify(|_, w| w.rxim().set_bit().rtim().set_bit());

    // Set UART1_IRQ priority and unmask in the NVIC.
    //
    // On RP2350 (Cortex-M33), FreeRTOS uses BASEPRI to mask interrupts
    // during critical sections.  ISRs that call FreeRTOS API functions
    // (like xQueueSendFromISR) MUST have a hardware priority value >=
    // configMAX_SYSCALL_INTERRUPT_PRIORITY (0x10, i.e. level 1).
    // The default priority 0 is above this threshold and will fire
    // inside critical sections, deadlocking on the kernel spinlock.
    //
    // On RP2040 (Cortex-M0+), PRIMASK masks all interrupts so priority
    // doesn't matter, but setting it is harmless.
    unsafe {
        // Write the priority register directly: NVIC_IPRn.
        // On RP2350 (4 priority bits), 0x10 = level 1, the lowest
        // priority that FreeRTOS masks during critical sections.
        let nvic_ipr = 0xE000_E400 as *mut u8;
        let irqn = pac::Interrupt::UART1_IRQ as u8;
        nvic_ipr.add(irqn as usize).write_volatile(0x10);
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::UART1_IRQ);
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Initialize the PDB UART1 RX queue and interrupt.
/// Must be called once at the start of `run_pdb_task()`.
pub fn init() {
    let q = Queue::new(256).expect("pdb uart1 queue alloc failed");
    unsafe { *UART1_RX_QUEUE.0.get() = Some(q) };

    crate::hal::uart::init(1);
    crate::hal::uart::reconfigure(1, 115_200, 8, 0, 1, 0);
    setup_uart1_rx_interrupt();
}

/// Read one byte from the UART1 RX queue, blocking forever.
pub fn queue_read_byte() -> u8 {
    uart1_rx_queue().receive(Duration::infinite()).unwrap_or(0)
}

/// Read one byte with a 2-second timeout.  Returns `None` if no byte arrives.
pub fn queue_read_byte_timeout() -> Option<u8> {
    uart1_rx_queue().receive(Duration::ms(2000)).ok()
}

/// Read one byte from the UART1 RX queue, busy-waiting with a hardware µs
/// timer timeout.  Uses non-blocking `receive(Duration::zero())` so it
/// works even when the FreeRTOS tick is frozen (core 0 parked).
pub fn queue_read_byte_busywait(timeout_us: u32) -> Option<u8> {
    #[cfg(feature = "chip-rp2040")]
    const TIMERAWL: usize = 0x4005_4000 + 0x28;
    #[cfg(feature = "chip-rp2350")]
    const TIMERAWL: usize = 0x400B_0000 + 0x28;

    let timer = || unsafe { core::ptr::read_volatile(TIMERAWL as *const u32) };
    let start = timer();
    loop {
        if let Ok(byte) = uart1_rx_queue().receive(Duration::zero()) {
            return Some(byte);
        }
        if timer().wrapping_sub(start) >= timeout_us {
            return None;
        }
    }
}

/// Read a u32 in little-endian byte order from the UART1 RX queue.
pub fn queue_read_u32_le() -> u32 {
    let b0 = queue_read_byte() as u32;
    let b1 = queue_read_byte() as u32;
    let b2 = queue_read_byte() as u32;
    let b3 = queue_read_byte() as u32;
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}

/// Spin until the UART1 TX FIFO is empty AND the shift register has
/// finished transmitting the last byte (including stop bits).
pub fn drain_tx() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;

    let p = unsafe { pac::Peripherals::steal() };
    while p.UART1.uartfr().read().busy().bit_is_set() {}
}
