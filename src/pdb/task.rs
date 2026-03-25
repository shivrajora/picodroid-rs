use core::cell::UnsafeCell;
use core::sync::atomic::Ordering;

use freertos_rust::{Duration, InterruptContext, Queue};

use super::pending;
use super::protocol::{
    crc32_frame, CMD_INSTALL, CMD_PING, FRAME_MAGIC, STATUS_CRC_FAIL, STATUS_ERR, STATUS_OK,
    STATUS_TOO_LARGE,
};

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
    // Enable the UART1 RX interrupt in the UART's interrupt mask register.
    // RXIM (bit 4) fires when the RX FIFO transitions from empty to non-empty.
    p.UART1.uartimsc().modify(|_, w| w.rxim().set_bit());

    // Unmask UART1_IRQ in the NVIC
    unsafe {
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::UART1_IRQ);
    }
}

// ── Low-level helpers ─────────────────────────────────────────────────────────

fn queue_read_byte() -> u8 {
    uart1_rx_queue().receive(Duration::infinite()).unwrap_or(0)
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

fn send_response(status: u8, payload: &[u8]) {
    use crate::system::picodroid::pio::uart::write_byte;
    let len = payload.len() as u32;
    for b in FRAME_MAGIC {
        write_byte(1, *b);
    }
    write_byte(1, status);
    for b in len.to_le_bytes() {
        write_byte(1, b);
    }
    for b in payload {
        write_byte(1, *b);
    }
}

fn drain_queue_bytes(count: u32) {
    for _ in 0..count {
        queue_read_byte();
    }
}

// ── pdb_task body ─────────────────────────────────────────────────────────────

pub fn run_pdb_task() -> ! {
    use crate::system::picodroid::pio::uart::{init, reconfigure};

    // Create the RX queue (256-byte capacity to absorb burst traffic)
    let q = Queue::new(256).expect("pdb uart1 queue alloc failed");
    unsafe { *UART1_RX_QUEUE.0.get() = Some(q) };

    // Initialize UART1 hardware and enable the RX interrupt
    init(1);
    reconfigure(1, 115_200, 8, 0, 1, 0);
    setup_uart1_rx_interrupt();

    loop {
        if !wait_for_magic() {
            continue;
        }

        let cmd = queue_read_byte();
        let len = queue_read_u32_le();

        if len as usize > pending::MAX_PAPK_SIZE {
            drain_queue_bytes(len);
            send_response(STATUS_TOO_LARGE, b"");
            continue;
        }

        // Stream payload bytes directly into the static PAPK buffer
        let buf = unsafe { core::slice::from_raw_parts_mut(pending::buf_mut(), len as usize) };
        for b in buf.iter_mut() {
            *b = queue_read_byte();
        }

        let wire_crc = queue_read_u32_le();
        let computed = crc32_frame(cmd, len, buf);
        if computed != wire_crc {
            send_response(STATUS_CRC_FAIL, b"");
            continue;
        }

        match cmd {
            CMD_PING => {
                // Payload: "picodroid/1.0\0" (14 bytes) + max_papk_bytes (4 bytes LE)
                // The host reads max_papk_bytes to validate file size before install.
                let mut ping_resp = [0u8; 18];
                ping_resp[..14].copy_from_slice(b"picodroid/1.0\0");
                ping_resp[14..18].copy_from_slice(&(pending::MAX_PAPK_SIZE as u32).to_le_bytes());
                send_response(STATUS_OK, &ping_resp);
            }

            CMD_INSTALL => {
                // 1. Write to flash first for persistence (interrupts disabled inside)
                let flash_ok = unsafe { super::flash::write_papk_to_flash(buf) };
                if !flash_ok {
                    send_response(STATUS_ERR, b"flash write failed");
                    continue;
                }

                // 2. Deposit in RAM for immediate hot-swap and signal jvm_task
                pending::PAPK_LEN.store(len as usize, Ordering::Relaxed);
                pending::HAS_PENDING.store(true, Ordering::Relaxed);
                pending::STOP_JVM.store(true, Ordering::Relaxed);

                // 3. ACK — jvm_task will restart asynchronously
                send_response(STATUS_OK, b"");
            }

            _ => send_response(STATUS_ERR, b"unknown cmd"),
        }
    }
}
