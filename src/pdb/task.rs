use core::cell::UnsafeCell;

use freertos_rust::{Duration, InterruptContext, Queue};

use super::protocol::{
    crc32_frame, CMD_INSTALL, CMD_PING, FRAME_MAGIC, STATUS_CRC_FAIL, STATUS_ERR, STATUS_OK,
};
use super::uart_transport::{PdbCoreCoordinator, UartTransport};

// ── UART1 RX queue ────────────────────────────────────────────────────────────
//
// The ISR drains the UART1 RX FIFO into this queue.  pdb_task blocks on
// receive(), sleeping at zero CPU cost while no data arrives.
//
// SAFETY: initialized once in run_pdb_task() before the interrupt is enabled;
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
    // The default 1/2 full (16 bytes) means short frames never trigger RXIM.
    p.UART1
        .uartifls()
        .modify(|_, w| unsafe { w.rxiflsel().bits(0b000) });

    // Enable both RXIM (FIFO threshold) and RTIM (receive timeout).
    // RTIM fires when the FIFO is non-empty but below the threshold and no
    // new character arrives for 32 bit periods (~278 µs at 115200 baud).
    // Without RTIM, trailing bytes of a short frame stay stuck in the FIFO.
    p.UART1
        .uartimsc()
        .modify(|_, w| w.rxim().set_bit().rtim().set_bit());

    // Unmask UART1_IRQ in the NVIC
    unsafe {
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::UART1_IRQ);
    }
}

// ── Low-level helpers ─────────────────────────────────────────────────────────

fn queue_read_byte() -> u8 {
    uart1_rx_queue().receive(Duration::infinite()).unwrap_or(0)
}

/// Read one byte with a 2-second timeout.  Returns `None` if no byte arrives,
/// which indicates the host has disconnected mid-stream.
pub(super) fn queue_read_byte_timeout() -> Option<u8> {
    uart1_rx_queue().receive(Duration::ms(2000)).ok()
}

fn queue_read_u32_le() -> u32 {
    let b0 = queue_read_byte() as u32;
    let b1 = queue_read_byte() as u32;
    let b2 = queue_read_byte() as u32;
    let b3 = queue_read_byte() as u32;
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}

/// Block until the 4-byte "PDBP" magic is found in the byte stream.
/// Re-syncs on any mismatch (discards unrecognised bytes).
fn wait_for_magic() -> bool {
    let mut matched = 0usize;
    // Give up after 64 KB of garbage without a frame start
    for _ in 0..65536usize {
        let b = queue_read_byte();
        if b == FRAME_MAGIC[matched] {
            matched += 1;
            if matched == FRAME_MAGIC.len() {
                return true;
            }
        } else {
            matched = if b == FRAME_MAGIC[0] { 1 } else { 0 };
        }
    }
    false
}

// ── Command handlers ──────────────────────────────────────────────────────────

fn handle_ping(len: u32) {
    // CMD_PING is a standard framed command with empty payload.
    // The host sends [PDBP][0x00][len=0][crc32]; consume the CRC.
    let wire_crc = queue_read_u32_le();
    let expected_crc = crc32_frame(CMD_PING, len, &[]);
    if wire_crc != expected_crc {
        UartTransport::send_pdbp_response(STATUS_CRC_FAIL, b"");
        return;
    }
    // Payload: "picodroid/2.0\0" (14 bytes) + max_papk_bytes (4 bytes LE)
    let mut ping_resp = [0u8; 18];
    ping_resp[..14].copy_from_slice(b"picodroid/2.0\0");
    ping_resp[14..18]
        .copy_from_slice(&(crate::packagemanager::flash::PAPK_MAX_DATA_SIZE as u32).to_le_bytes());
    UartTransport::send_pdbp_response(STATUS_OK, &ping_resp);
}

// ── pdb_task body ─────────────────────────────────────────────────────────────

pub fn run_pdb_task() -> ! {
    use crate::system::picodroid::pio::uart::{init, reconfigure};

    let q = Queue::new(256).expect("pdb uart1 queue alloc failed");
    unsafe { *UART1_RX_QUEUE.0.get() = Some(q) };

    init(1);
    reconfigure(1, 115_200, 8, 0, 1, 0);
    setup_uart1_rx_interrupt();

    loop {
        if !wait_for_magic() {
            continue;
        }
        let cmd = queue_read_byte();
        let len = queue_read_u32_le();
        match cmd {
            CMD_PING => handle_ping(len),
            CMD_INSTALL => {
                let mut transport = UartTransport;
                let mut coordinator = PdbCoreCoordinator;
                crate::packagemanager::install::run_install(&mut transport, &mut coordinator, len);
            }
            _ => UartTransport::send_pdbp_response(STATUS_ERR, b"unknown cmd"),
        }
    }
}
