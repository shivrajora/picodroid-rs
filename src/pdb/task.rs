use core::cell::UnsafeCell;
use core::sync::atomic::Ordering;

use freertos_rust::{Duration, InterruptContext, Queue};

use super::pending;
use super::protocol::{
    crc32_frame, CMD_INSTALL, CMD_PING, FRAME_MAGIC, STATUS_CRC_FAIL, STATUS_ERR, STATUS_OK,
    STATUS_READY, STATUS_TOO_LARGE,
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

// ── 256-byte page buffer for streaming flash writes ───────────────────────────
//
// Replaces the old 64 KB PAPK_BUF.  Used only during CMD_INSTALL handling.
// SAFETY: single-core; only pdb_task writes, no concurrent reader.
struct PageBufCell(UnsafeCell<[u8; 256]>);
unsafe impl Sync for PageBufCell {}
static PAGE_BUF: PageBufCell = PageBufCell(UnsafeCell::new([0u8; 256]));

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

/// Read one byte with a 2-second timeout.  Returns `None` if no byte arrives,
/// which indicates the host has disconnected mid-stream.
fn queue_read_byte_timeout() -> Option<u8> {
    uart1_rx_queue().receive(Duration::ms(2000)).ok()
}

fn queue_read_u32_le() -> u32 {
    let b0 = queue_read_byte() as u32;
    let b1 = queue_read_byte() as u32;
    let b2 = queue_read_byte() as u32;
    let b3 = queue_read_byte() as u32;
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}

/// Read a u32 LE with per-byte timeout.  Returns `None` if any byte times out.
fn queue_read_u32_le_timeout() -> Option<u32> {
    let b0 = queue_read_byte_timeout()? as u32;
    let b1 = queue_read_byte_timeout()? as u32;
    let b2 = queue_read_byte_timeout()? as u32;
    let b3 = queue_read_byte_timeout()? as u32;
    Some(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
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

    'cmd: loop {
        if !wait_for_magic() {
            continue;
        }

        let cmd = queue_read_byte();
        // For CMD_INSTALL the len field carries the PAPK size (no inline payload follows).
        // For CMD_PING the len field is 0 (empty payload frame).
        let len = queue_read_u32_le();

        match cmd {
            CMD_PING => {
                // CMD_PING is a standard framed command with empty payload.
                // The host sends [PDBP][0x00][len=0][crc32]; consume the CRC.
                let wire_crc = queue_read_u32_le();
                let expected_crc = crc32_frame(CMD_PING, len, &[]);
                if wire_crc != expected_crc {
                    send_response(STATUS_CRC_FAIL, b"");
                    continue 'cmd;
                }
                // Payload: "picodroid/2.0\0" (14 bytes) + max_papk_bytes (4 bytes LE)
                let mut ping_resp = [0u8; 18];
                ping_resp[..14].copy_from_slice(b"picodroid/2.0\0");
                ping_resp[14..18]
                    .copy_from_slice(&(super::flash::PAPK_MAX_DATA_SIZE as u32).to_le_bytes());
                send_response(STATUS_OK, &ping_resp);
            }

            CMD_INSTALL => {
                // ── Phase A: validate length and erase flash ──────────────────
                if len == 0 {
                    send_response(STATUS_ERR, b"");
                    continue 'cmd;
                }
                if len as usize > super::flash::PAPK_MAX_DATA_SIZE {
                    send_response(STATUS_TOO_LARGE, b"");
                    continue 'cmd;
                }

                // Erase the PAPK flash region (interrupts disabled, ~1.6 s).
                // The host is idle during this time (waiting for STATUS_READY),
                // so no UART data is lost.
                unsafe { super::flash::flash_erase_papk_region() };

                // Signal host that we are ready to receive the PAPK data stream.
                send_response(STATUS_READY, b"");

                // ── Phase B: stream PAPK bytes, write pages to flash ──────────
                //
                // The host now sends `len` raw bytes followed by a 4-byte CRC32.
                // CRC32 covers [CMD_INSTALL][len_LE][papk_bytes] — same formula
                // as the existing crc32_frame function.
                //
                // We compute the CRC incrementally as bytes arrive and write each
                // 256-byte page to flash immediately.  Interrupts are disabled for
                // ~1 ms per page write; the UART hardware FIFO (32 bytes) absorbs
                // the ~12 bytes that arrive during each write.
                //
                // A 2-second per-byte timeout detects host disconnect: if the host
                // drops the connection after STATUS_READY, pdb_task recovers and
                // returns to wait_for_magic() so the user can retry.

                let mut crc_hasher = crc32fast::Hasher::new();
                crc_hasher.update(&[CMD_INSTALL]);
                crc_hasher.update(&len.to_le_bytes());

                let mut bytes_remaining = len as usize;
                let mut page_index: u32 = 0;

                'pages: loop {
                    if bytes_remaining == 0 {
                        break 'pages;
                    }
                    let chunk = bytes_remaining.min(256);
                    let page = unsafe { &mut *PAGE_BUF.0.get() };

                    // Fill the chunk bytes from the queue (with disconnect detection).
                    for b in page[..chunk].iter_mut() {
                        match queue_read_byte_timeout() {
                            Some(byte) => *b = byte,
                            None => {
                                send_response(STATUS_ERR, b"stream timeout");
                                continue 'cmd;
                            }
                        }
                    }
                    // Pad the remainder of the page with 0xFF (erased flash value).
                    for b in page[chunk..].iter_mut() {
                        *b = 0xFF;
                    }

                    // Update CRC over the actual data bytes only (not padding).
                    crc_hasher.update(&page[..chunk]);

                    if !unsafe { super::flash::flash_write_page(page_index, page) } {
                        // Should never happen given the len validation above.
                        send_response(STATUS_ERR, b"flash write failed");
                        continue 'cmd;
                    }
                    page_index += 1;
                    bytes_remaining -= chunk;
                }

                // Read the 4-byte CRC32 that the host appended (also with timeout).
                let wire_crc = match queue_read_u32_le_timeout() {
                    Some(v) => v,
                    None => {
                        send_response(STATUS_ERR, b"stream timeout");
                        continue 'cmd;
                    }
                };
                let computed_crc = crc_hasher.finalize();

                if computed_crc != wire_crc {
                    send_response(STATUS_CRC_FAIL, b"");
                    // Flash data region was written but the metadata magic was
                    // never committed, so read_flash_papk() returns None on next
                    // boot and the device falls back to the baked-in APK.
                    continue 'cmd;
                }

                // Commit the atomic boot metadata (writes the magic last).
                unsafe { super::flash::flash_commit_metadata(len) };

                // Signal the JVM to exit gracefully before acknowledging.
                // Setting these before STATUS_OK ensures the JVM is already
                // stopping when the host polls PING after the reboot.
                pending::REBOOT_PENDING.store(true, Ordering::Relaxed);
                pending::STOP_JVM.store(true, Ordering::Relaxed);
                pending::notify_jvm();

                // Acknowledge the install.  pdb_task continues running so it
                // can respond to PING polls while the JVM winds down.
                send_response(STATUS_OK, b"");
            }

            _ => send_response(STATUS_ERR, b"unknown cmd"),
        }
    }
}
