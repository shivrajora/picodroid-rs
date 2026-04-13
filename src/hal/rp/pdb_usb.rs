//! USB CDC ACM driver for PDB (Picodroid Debug Bridge).
//!
//! Replaces the UART1-based PDB transport with a USB bulk endpoint, eliminating
//! the Debug Probe hardware and providing ~100× faster installs.
//!
//! Both RP2040 and RP2350 share the same USB 1.1 controller at the same memory
//! addresses, so one driver covers both chips.

use core::cell::UnsafeCell;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

use freertos_rust::{Duration, InterruptContext, Queue};

// ── Register addresses ──────────────────────────────────────────────────────

const REGS: usize = 0x5011_0000;
const DPRAM: usize = 0x5010_0000;

// USBCTRL_REGS offsets
const ADDR_ENDP: usize = REGS;
const MAIN_CTRL: usize = REGS + 0x40;
const SIE_CTRL: usize = REGS + 0x4C;
const SIE_STATUS: usize = REGS + 0x50;
const INT_EP_CTRL: usize = REGS + 0x54;
const BUFF_STATUS: usize = REGS + 0x58;
const EP_STALL_ARM: usize = REGS + 0x68;
const USB_MUXING: usize = REGS + 0x74;
const USB_PWR: usize = REGS + 0x78;
const INTE: usize = REGS + 0x90;
const INTS: usize = REGS + 0x98;

// DPRAM offsets
const DP_EP1_IN_CTRL: usize = DPRAM + 0x08;
const DP_EP1_OUT_CTRL: usize = DPRAM + 0x0C;
const DP_EP2_IN_CTRL: usize = DPRAM + 0x10;
const DP_EP0_IN_BC: usize = DPRAM + 0x80;
const DP_EP0_OUT_BC: usize = DPRAM + 0x84;
const DP_EP1_IN_BC: usize = DPRAM + 0x88;
const DP_EP1_OUT_BC: usize = DPRAM + 0x8C;

// Data buffer offsets within DPRAM
const BUF_EP0: usize = 0x100;
const BUF_EP1_OUT: usize = 0x140;
const BUF_EP1_IN: usize = 0x180;

// ── Bit constants ───────────────────────────────────────────────────────────

// INTS / INTE
const INT_BUFF_STATUS: u32 = 1 << 4;
const INT_BUS_RESET: u32 = 1 << 12;
const INT_SETUP_REQ: u32 = 1 << 16;

// SIE_STATUS (W1C)
const SS_SETUP_REC: u32 = 1 << 17;
const SS_BUS_RESET: u32 = 1 << 19;

// SIE_CTRL
const SC_PULLUP_EN: u32 = 1 << 16;
const SC_EP0_INT_1BUF: u32 = 1 << 29;

// BUFF_STATUS
const BS_EP0_IN: u32 = 1 << 0;
const BS_EP0_OUT: u32 = 1 << 1;
const BS_EP1_IN: u32 = 1 << 2;
const BS_EP1_OUT: u32 = 1 << 3;

// EP buffer control (single-buffer fields)
const BC_AVAIL: u32 = 1 << 10;
const BC_STALL: u32 = 1 << 11;
const BC_DATA1: u32 = 1 << 13;
const BC_LAST: u32 = 1 << 14;
const BC_FULL: u32 = 1 << 15;

// EP control (DPRAM endpoint config registers)
const EC_ENABLE: u32 = 1 << 31;
const EC_IRQ_BUF: u32 = 1 << 29;
const EC_BULK: u32 = 2 << 26;
const EC_INTERRUPT: u32 = 3 << 26;

// ── USB descriptors ─────────────────────────────────────────────────────────

/// VID 0x1209 (pid.codes open-source), PID 0xCDC0 (picodroid CDC).
const DEVICE_DESC: [u8; 18] = [
    18, 0x01, 0x00, 0x02, 0x02, 0x00, 0x00, 64, 0x09, 0x12, 0xC0, 0xCD, 0x00, 0x01, 1, 2, 0, 1,
];

const CONFIG_DESC: [u8; 67] = [
    // Configuration
    9, 0x02, 67, 0, 2, 1, 0, 0x80, 250, // Interface 0: CDC Control (1 endpoint)
    9, 0x04, 0, 0, 1, 0x02, 0x02, 0x01, 0, // CDC Header FD
    5, 0x24, 0x00, 0x20, 0x01, // CDC Call Management FD
    5, 0x24, 0x01, 0x00, 0x01, // CDC ACM FD
    4, 0x24, 0x02, 0x02, // CDC Union FD
    5, 0x24, 0x06, 0x00, 0x01, // EP2 IN: interrupt, 8 bytes, 255ms
    7, 0x05, 0x82, 0x03, 8, 0, 255, // Interface 1: CDC Data (2 endpoints)
    9, 0x04, 1, 0, 2, 0x0A, 0x00, 0x00, 0, // EP1 OUT: bulk, 64 bytes
    7, 0x05, 0x01, 0x02, 64, 0, 0, // EP1 IN: bulk, 64 bytes
    7, 0x05, 0x81, 0x02, 64, 0, 0,
];

// String descriptor 0: language (English US)
const STR0: [u8; 4] = [4, 0x03, 0x09, 0x04];
// String 1: "Picodroid"
const STR1: [u8; 20] = [
    20, 0x03, b'P', 0, b'i', 0, b'c', 0, b'o', 0, b'd', 0, b'r', 0, b'o', 0, b'i', 0, b'd', 0,
];
// String 2: "PDB (USB CDC)"
const STR2: [u8; 28] = [
    28, 0x03, b'P', 0, b'D', 0, b'B', 0, b' ', 0, b'(', 0, b'U', 0, b'S', 0, b'B', 0, b' ', 0,
    b'C', 0, b'D', 0, b'C', 0, b')', 0,
];

// Line coding: 115200 8N1 (returned for GET_LINE_CODING)
const LINE_CODING: [u8; 7] = [0x00, 0xC2, 0x01, 0x00, 0x00, 0x00, 0x08];

// ── Static state ────────────────────────────────────────────────────────────

struct UsbState {
    pending_addr: u8,
    configured: bool,
    ep0_pid: bool,
    ep1_in_pid: bool,
    ep1_out_pid: bool,
    ep0_tx_src: usize,
    ep0_tx_len: u16,
    ep0_data_stage: bool,
    ep0_out_pending: bool,
}

static mut USB: UsbState = UsbState {
    pending_addr: 0,
    configured: false,
    ep0_pid: true,
    ep1_in_pid: false,
    ep1_out_pid: false,
    ep0_tx_src: 0,
    ep0_tx_len: 0,
    ep0_data_stage: false,
    ep0_out_pending: false,
};

/// Signalled by ISR when an EP1 IN transfer completes.
static EP1_IN_DONE: AtomicBool = AtomicBool::new(true);

// ── Flow-control counters ───────────────────────────────────────────────────
//
// These are accessed from both the USB ISR and the PDB task, but always on
// core 1 (ISR preempts task, never concurrent).  Volatile read/write is
// sufficient — no need for full atomics (which aren't available on M0+).

/// Approximate number of bytes in the RX queue (incremented by ISR, decremented by task).
static mut RX_QUEUE_LEVEL: u32 = 0;

/// Length of a deferred EP1 OUT packet whose data is still in DPRAM.
/// Non-zero means the ISR deferred the entire packet because the queue was
/// too full.  The task must drain this from DPRAM before re-arming.
static mut EP1_OUT_DEFERRED_LEN: u16 = 0;

/// Tracks whether the EP1 OUT endpoint is currently armed (AVAIL set).
/// Prevents double-arming which is unsafe while a transfer is in progress.
static mut EP1_OUT_ARMED: bool = false;

const RX_QUEUE_CAPACITY: u32 = 512;

struct QueueCell(UnsafeCell<Option<Queue<u8>>>);
unsafe impl Sync for QueueCell {}
static USB_RX_QUEUE: QueueCell = QueueCell(UnsafeCell::new(None));

fn rx_queue() -> &'static Queue<u8> {
    unsafe {
        (*USB_RX_QUEUE.0.get())
            .as_ref()
            .expect("USB_RX_QUEUE not initialised")
    }
}

// ── Low-level register helpers ──────────────────────────────────────────────

unsafe fn rr(addr: usize) -> u32 {
    read_volatile(addr as *const u32)
}

unsafe fn wr(addr: usize, val: u32) {
    write_volatile(addr as *mut u32, val);
}

unsafe fn dpram_write(offset: usize, data: &[u8]) {
    let base = (DPRAM + offset) as *mut u8;
    for (i, &b) in data.iter().enumerate() {
        base.add(i).write_volatile(b);
    }
}

// ── EP0 helpers ─────────────────────────────────────────────────────────────

unsafe fn ep0_send(data: &[u8], max_len: u16) {
    USB.ep0_data_stage = true;
    let len = data.len().min(max_len as usize);
    let chunk = len.min(64);
    dpram_write(BUF_EP0, &data[..chunk]);

    USB.ep0_tx_src = if len > chunk {
        data.as_ptr().add(chunk) as usize
    } else {
        0
    };
    USB.ep0_tx_len = (len - chunk) as u16;

    // First data-stage packet is always DATA1.
    USB.ep0_pid = false; // next packet will be DATA0
    wr(
        DP_EP0_IN_BC,
        BC_FULL | BC_DATA1 | BC_LAST | chunk as u32 | BC_AVAIL,
    );
}

/// Send a zero-length IN packet for the status stage (always DATA1).
unsafe fn ep0_zlp() {
    USB.ep0_data_stage = false;
    USB.ep0_tx_src = 0;
    USB.ep0_tx_len = 0;
    wr(DP_EP0_IN_BC, BC_FULL | BC_DATA1 | BC_LAST | BC_AVAIL);
}

unsafe fn ep0_stall() {
    wr(EP_STALL_ARM, 0x03); // stall EP0 IN + OUT
    wr(DP_EP0_IN_BC, BC_STALL);
    wr(DP_EP0_OUT_BC, BC_STALL);
}

unsafe fn arm_ep1_out() {
    core::ptr::write_volatile(&raw mut EP1_OUT_ARMED, true);
    let pid = if USB.ep1_out_pid { BC_DATA1 } else { 0 };
    wr(DP_EP1_OUT_BC, pid | 64 | BC_AVAIL);
}

// ── SETUP packet handlers ───────────────────────────────────────────────────

unsafe fn handle_setup() {
    wr(SIE_STATUS, SS_SETUP_REC);

    let lo = rr(DPRAM);
    let hi = rr(DPRAM + 4);
    let bm_rt = (lo & 0xFF) as u8;
    let b_req = ((lo >> 8) & 0xFF) as u8;
    let w_val = (lo >> 16) as u16;
    let _w_idx = hi as u16;
    let w_len = (hi >> 16) as u16;

    USB.ep0_pid = true;
    USB.ep0_out_pending = false;

    match (bm_rt, b_req) {
        (0x80, 0x06) => handle_get_descriptor(w_val, w_len),
        (0x00, 0x05) => {
            USB.pending_addr = (w_val & 0x7F) as u8;
            ep0_zlp();
        }
        (0x00, 0x09) => {
            if w_val as u8 == 1 {
                USB.configured = true;
                // Configure bulk + interrupt endpoints
                wr(
                    DP_EP1_OUT_CTRL,
                    EC_ENABLE | EC_IRQ_BUF | EC_BULK | BUF_EP1_OUT as u32,
                );
                wr(
                    DP_EP1_IN_CTRL,
                    EC_ENABLE | EC_IRQ_BUF | EC_BULK | BUF_EP1_IN as u32,
                );
                wr(DP_EP2_IN_CTRL, EC_ENABLE | EC_INTERRUPT | 0x1C0);
                USB.ep1_in_pid = false;
                USB.ep1_out_pid = false;
                EP1_IN_DONE.store(true, Ordering::Release);
                arm_ep1_out();
            }
            ep0_zlp();
        }
        (0x80, 0x00) => ep0_send(&[0, 0], w_len), // GET_STATUS
        (0x21, 0x20) => {
            // SET_LINE_CODING: receive 7 bytes from host, then ACK
            USB.ep0_out_pending = true;
            wr(DP_EP0_OUT_BC, BC_DATA1 | w_len.min(64) as u32 | BC_AVAIL);
        }
        (0xA1, 0x21) => ep0_send(&LINE_CODING, w_len), // GET_LINE_CODING
        (0x21, 0x22) => ep0_zlp(),                     // SET_CONTROL_LINE_STATE
        _ => ep0_stall(),
    }
}

unsafe fn handle_get_descriptor(w_val: u16, w_len: u16) {
    let desc_type = (w_val >> 8) as u8;
    let desc_idx = (w_val & 0xFF) as u8;
    match desc_type {
        1 => ep0_send(&DEVICE_DESC, w_len),
        2 => ep0_send(&CONFIG_DESC, w_len),
        3 => match desc_idx {
            0 => ep0_send(&STR0, w_len),
            1 => ep0_send(&STR1, w_len),
            2 => ep0_send(&STR2, w_len),
            _ => ep0_stall(),
        },
        6 => ep0_stall(), // DEVICE_QUALIFIER — full-speed only
        _ => ep0_stall(),
    }
}

// ── Buffer completion handlers ──────────────────────────────────────────────

unsafe fn handle_ep0_in_done() {
    if USB.pending_addr != 0 {
        wr(ADDR_ENDP, USB.pending_addr as u32);
        USB.pending_addr = 0;
    }
    if USB.ep0_tx_len > 0 {
        let chunk = (USB.ep0_tx_len as usize).min(64);
        let src = core::slice::from_raw_parts(USB.ep0_tx_src as *const u8, chunk);
        dpram_write(BUF_EP0, src);
        USB.ep0_tx_src += chunk;
        USB.ep0_tx_len -= chunk as u16;
        let pid = if USB.ep0_pid { BC_DATA1 } else { 0 };
        USB.ep0_pid = !USB.ep0_pid;
        wr(
            DP_EP0_IN_BC,
            BC_FULL | pid | BC_LAST | chunk as u32 | BC_AVAIL,
        );
        return;
    }
    if USB.ep0_data_stage {
        // IN data stage done — arm EP0 OUT for host's status ZLP
        wr(DP_EP0_OUT_BC, BC_DATA1 | 64 | BC_AVAIL);
    }
}

unsafe fn handle_ep0_out_done() {
    if USB.ep0_out_pending {
        USB.ep0_out_pending = false;
        // SET_LINE_CODING data received — send IN ZLP for status
        wr(DP_EP0_IN_BC, BC_FULL | BC_DATA1 | BC_LAST | BC_AVAIL);
    }
}

unsafe fn handle_ep1_out_data() {
    // Hardware cleared AVAIL after receiving — endpoint is no longer armed.
    core::ptr::write_volatile(&raw mut EP1_OUT_ARMED, false);

    let bc = rr(DP_EP1_OUT_BC);
    let len = (bc & 0x3FF) as usize;
    let level = core::ptr::read_volatile(&raw const RX_QUEUE_LEVEL);

    USB.ep1_out_pid = !USB.ep1_out_pid;

    // Flow control: if the queue cannot hold this entire packet, defer it.
    // The packet data stays in DPRAM until the task drains the queue and
    // calls drain_deferred_packet().  The endpoint is NOT re-armed, so the
    // USB host sees NAKs until we are ready.
    if level + len as u32 > RX_QUEUE_CAPACITY {
        core::ptr::write_volatile(&raw mut EP1_OUT_DEFERRED_LEN, len as u16);
        return; // don't push, don't re-arm
    }

    let mut ctx = InterruptContext::new();
    let base = (DPRAM + BUF_EP1_OUT) as *const u8;
    for i in 0..len {
        let byte = base.add(i).read_volatile();
        let _ = rx_queue().send_from_isr(&mut ctx, byte);
    }
    let new_level = level + len as u32;
    core::ptr::write_volatile(&raw mut RX_QUEUE_LEVEL, new_level);

    // Re-arm if there is room for another max-size packet.
    if new_level + 64 <= RX_QUEUE_CAPACITY {
        arm_ep1_out();
    }
    // else: don't re-arm; the task will re-arm after consuming bytes.
}

// ── ISR ─────────────────────────────────────────────────────────────────────

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn USBCTRL_IRQ() {
    unsafe {
        let ints = rr(INTS);

        if ints & INT_BUS_RESET != 0 {
            wr(SIE_STATUS, SS_BUS_RESET);
            wr(ADDR_ENDP, 0);
            USB.pending_addr = 0;
            USB.configured = false;
            USB.ep0_pid = true;
            USB.ep1_in_pid = false;
            USB.ep1_out_pid = false;
            core::ptr::write_volatile(&raw mut RX_QUEUE_LEVEL, 0);
            core::ptr::write_volatile(&raw mut EP1_OUT_DEFERRED_LEN, 0);
            core::ptr::write_volatile(&raw mut EP1_OUT_ARMED, false);
        }

        if ints & INT_SETUP_REQ != 0 {
            handle_setup();
        }

        if ints & INT_BUFF_STATUS != 0 {
            let bs = rr(BUFF_STATUS);
            wr(BUFF_STATUS, bs); // clear all W1C bits at once

            if bs & BS_EP0_IN != 0 {
                handle_ep0_in_done();
            }
            if bs & BS_EP0_OUT != 0 {
                handle_ep0_out_done();
            }
            if bs & BS_EP1_OUT != 0 {
                handle_ep1_out_data();
            }
            if bs & BS_EP1_IN != 0 {
                EP1_IN_DONE.store(true, Ordering::Release);
            }
        }
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Initialise the USB CDC driver and enable the DP pull-up.
pub fn init() {
    let q = Queue::new(512).expect("pdb usb rx queue");
    unsafe { *USB_RX_QUEUE.0.get() = Some(q) };

    // Release USBCTRL from reset.
    {
        #[cfg(feature = "chip-rp2350-hal")]
        use rp235x_hal::pac;
        #[cfg(feature = "chip-rp2040")]
        use rp_pico::hal::pac;

        let p = unsafe { pac::Peripherals::steal() };
        p.RESETS.reset().modify(|_, w| w.usbctrl().clear_bit());
        while p.RESETS.reset_done().read().usbctrl().bit_is_clear() {}
    }

    unsafe {
        // Clear DPRAM (4 KiB)
        let base = DPRAM as *mut u32;
        for i in 0..1024 {
            base.add(i).write_volatile(0);
        }

        wr(USB_MUXING, (1 << 0) | (1 << 3)); // TO_PHY | SOFTCON
        wr(USB_PWR, (1 << 2) | (1 << 3)); // VBUS_DETECT | VBUS_DETECT_OVERRIDE_EN
        wr(MAIN_CTRL, 1); // CONTROLLER_EN, device mode
        wr(SIE_CTRL, SC_EP0_INT_1BUF);
        wr(INT_EP_CTRL, (1 << 1) | (1 << 2)); // buffer IRQs for EP1 + EP2
        wr(INTE, INT_BUS_RESET | INT_SETUP_REQ | INT_BUFF_STATUS);

        // Connect: enable DP pull-up
        let sc = rr(SIE_CTRL);
        wr(SIE_CTRL, sc | SC_PULLUP_EN);
    }

    // NVIC: set priority and unmask
    {
        #[cfg(feature = "chip-rp2350-hal")]
        use rp235x_hal::pac;
        #[cfg(feature = "chip-rp2040")]
        use rp_pico::hal::pac;

        unsafe {
            let nvic_ipr = 0xE000_E400 as *mut u8;
            let irqn = pac::Interrupt::USBCTRL_IRQ as u8;
            nvic_ipr.add(irqn as usize).write_volatile(0x10);
            cortex_m::peripheral::NVIC::unmask(pac::Interrupt::USBCTRL_IRQ);
        }
    }
}

/// Called after consuming a byte from the RX queue.  Decrements the
/// level counter and, if the ISR deferred a packet, drains it from DPRAM
/// once there is room, then re-arms the endpoint.
fn on_byte_consumed() {
    // Prevent the USB ISR from preempting the RX_QUEUE_LEVEL
    // read-modify-write and the deferred-packet drain sequence.
    cortex_m::interrupt::free(|_| unsafe {
        let level = core::ptr::read_volatile(&raw const RX_QUEUE_LEVEL).saturating_sub(1);
        core::ptr::write_volatile(&raw mut RX_QUEUE_LEVEL, level);

        // Already armed — nothing to do.
        if core::ptr::read_volatile(&raw const EP1_OUT_ARMED) {
            return;
        }

        let deferred = core::ptr::read_volatile(&raw const EP1_OUT_DEFERRED_LEN);
        if deferred > 0 && level + deferred as u32 <= RX_QUEUE_CAPACITY {
            // Drain the deferred packet from DPRAM into the queue.
            // Safe: endpoint is not armed, so DPRAM won't be overwritten.
            let base = (DPRAM + BUF_EP1_OUT) as *const u8;
            for i in 0..deferred as usize {
                let byte = base.add(i).read_volatile();
                let _ = rx_queue().send(byte, Duration::zero());
            }
            core::ptr::write_volatile(&raw mut RX_QUEUE_LEVEL, level + deferred as u32);
            core::ptr::write_volatile(&raw mut EP1_OUT_DEFERRED_LEN, 0);
            arm_ep1_out();
        } else if deferred == 0 && level + 64 <= RX_QUEUE_CAPACITY {
            // No deferred packet, but the ISR skipped re-arm because the
            // queue was almost full.  Re-arm now that there is room.
            arm_ep1_out();
        }
    });
}

/// Read one byte from the USB CDC RX queue, blocking forever.
pub fn queue_read_byte() -> u8 {
    let b = rx_queue().receive(Duration::infinite()).unwrap_or(0);
    on_byte_consumed();
    b
}

/// Read one byte with a 2-second timeout.
pub fn queue_read_byte_timeout() -> Option<u8> {
    let b = rx_queue().receive(Duration::ms(2000)).ok();
    if b.is_some() {
        on_byte_consumed();
    }
    b
}

/// Read one byte using busy-wait with hardware timer timeout.
/// Works when the FreeRTOS tick is frozen (RP2350, core 0 parked).
#[cfg(feature = "chip-rp2350-hal")]
pub fn queue_read_byte_busywait(timeout_us: u32) -> Option<u8> {
    const TIMERAWL: usize = 0x400B_0000 + 0x28;
    let timer = || unsafe { read_volatile(TIMERAWL as *const u32) };
    let start = timer();
    loop {
        if let Ok(byte) = rx_queue().receive(Duration::zero()) {
            on_byte_consumed();
            return Some(byte);
        }
        if timer().wrapping_sub(start) >= timeout_us {
            return None;
        }
    }
}

/// Read a u32 in little-endian byte order from the USB CDC RX queue.
pub fn queue_read_u32_le() -> u32 {
    let b0 = queue_read_byte() as u32;
    let b1 = queue_read_byte() as u32;
    let b2 = queue_read_byte() as u32;
    let b3 = queue_read_byte() as u32;
    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
}

/// Wait for the previous EP1 IN transfer to complete.
fn wait_tx_ready() {
    while !EP1_IN_DONE.load(Ordering::Acquire) {
        cortex_m::asm::nop();
    }
}

/// Write a block of bytes to the USB CDC bulk IN endpoint.
/// Automatically splits into 64-byte USB packets.
pub fn write_bytes(data: &[u8]) {
    for chunk in data.chunks(64) {
        wait_tx_ready();
        EP1_IN_DONE.store(false, Ordering::Release);
        unsafe {
            dpram_write(BUF_EP1_IN, chunk);
            let pid = if USB.ep1_in_pid { BC_DATA1 } else { 0 };
            USB.ep1_in_pid = !USB.ep1_in_pid;
            wr(
                DP_EP1_IN_BC,
                BC_FULL | pid | BC_LAST | chunk.len() as u32 | BC_AVAIL,
            );
        }
    }
}

/// Write a single byte to the USB CDC bulk IN endpoint.
pub fn write_byte(byte: u8) {
    write_bytes(&[byte]);
}

/// Wait until the last USB IN transfer has completed.
pub fn drain_tx() {
    wait_tx_ready();
}
