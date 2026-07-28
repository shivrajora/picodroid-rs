// SPDX-License-Identifier: GPL-3.0-only
//! The device half of PDBP — the picodroid debug bridge.
//!
//! Everything here is protocol: resync on a frame magic, dispatch a command
//! byte, verify a CRC, answer. The bytes themselves come from
//! [`PdbTransport`], which is the only family-specific part — a USB CDC pipe
//! on this hardware, something else on the next.
//!
//! # Why this is shared
//!
//! The alternative is each family reimplementing a wire protocol whose host
//! side it does not control. `tools/pdb` speaks exactly one dialect; a family
//! that framed a sysmon response differently would not fail loudly, it would
//! print garbage task names. The constants already live in `pdb-protocol` so
//! both ends agree on them; this module is the other half of that argument.
//!
//! # Why generics rather than `__pd_*` shims
//!
//! One caller — the family's debug-bridge task — and no shared statics
//! needing family types. A generic parameter adds no link surface, needs no
//! cfg-gating on boards that ship no bridge at all, and monomorphises in the
//! family crate where that single caller already is. It also makes the whole
//! thing testable, which the `__pd_*` seam would not: see [`sysmon`]'s
//! golden-bytes test, which pins the layout `tools/pdb` parses.

pub mod framing;
pub mod input;
pub mod sysmon;

pub use framing::send_response;
pub use sysmon::{SysmonSample, SysmonSource, TaskSample, MAX_TASKS};

use pdb_protocol::{
    crc32_frame, greeting, CMD_INPUT, CMD_INSTALL, CMD_PING, CMD_SYSMON, FRAME_MAGIC,
    STATUS_CRC_FAIL, STATUS_ERR, STATUS_OK,
};

use crate::install::{CoreCoordinator, PapkFlash};

/// A byte pipe to the host, framed by this module.
///
/// Exactly the surface `hal/contract.rs` used to assert by hand for
/// `pdb_usb`, now a trait — so a signature that drifts fails to compile at
/// the impl rather than at a type-ascription block someone has to remember to
/// update.
pub trait PdbTransport {
    /// Bring the transport up. Called once, before the command loop.
    fn init(&mut self);

    /// Block until a byte arrives.
    fn read_byte(&mut self) -> u8;

    /// Read a byte, or `None` if none arrives within the transport's timeout.
    ///
    /// Used only while streaming an install. A family whose tick source stops
    /// during flash writes — this is real, see the RP2350 — must busy-wait on
    /// hardware here rather than block on an RTOS primitive that will never
    /// be serviced. That belongs inside the impl; the caller only needs to
    /// know a read can time out.
    fn read_byte_timeout(&mut self) -> Option<u8>;

    fn write_bytes(&mut self, data: &[u8]);

    /// Block until everything written has actually left the device. Called
    /// before a reset, where anything still buffered is lost.
    fn drain_tx(&mut self);

    /// Read a little-endian `u32`. The default reads four bytes; transports
    /// with a bulk path may override.
    fn read_u32_le(&mut self) -> u32 {
        let b0 = self.read_byte() as u32;
        let b1 = self.read_byte() as u32;
        let b2 = self.read_byte() as u32;
        let b3 = self.read_byte() as u32;
        b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
    }
}

/// Bytes of garbage tolerated while hunting for a frame start before giving
/// up and re-entering the loop. Bounded so a wedged or noisy line cannot spin
/// forever inside one call.
const RESYNC_LIMIT: usize = 65536;

/// Block until the frame magic is seen. `false` if [`RESYNC_LIMIT`] bytes
/// went by without one.
fn wait_for_magic(transport: &mut impl PdbTransport) -> bool {
    let mut matched = 0usize;
    for _ in 0..RESYNC_LIMIT {
        let b = transport.read_byte();
        if b == FRAME_MAGIC[matched] {
            matched += 1;
            if matched == FRAME_MAGIC.len() {
                return true;
            }
        } else {
            // A mismatch may itself be the start of the real magic.
            matched = usize::from(b == FRAME_MAGIC[0]);
        }
    }
    false
}

fn handle_ping(transport: &mut impl PdbTransport, flash: &impl PapkFlash, len: u32) {
    // A framed command with an empty payload: consume the trailing CRC so the
    // stream stays aligned whether or not it matches.
    let wire_crc = transport.read_u32_le();
    if wire_crc != crc32_frame(CMD_PING, len, &[]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return;
    }
    // The greeting layout is `pdb_protocol::greeting`'s; this build supplies
    // the two facts only it knows — slot capacity and framework-map-version.
    let mut buf = [0u8; greeting::GREETING_MAX];
    let total = greeting::encode(
        flash.max_data_size() as u32,
        crate::framework_map::FRAMEWORK_MAP_VERSION.as_bytes(),
        &mut buf,
    );
    send_response(transport, STATUS_OK, &buf[..total]);
}

/// Run the debug bridge. Never returns.
///
/// The family supplies the four things this cannot know: how bytes reach the
/// host, how to park the JVM core for a flash write, where task statistics
/// come from, and what the PAPK slot is.
pub fn run_pdb_task(
    mut transport: impl PdbTransport,
    mut coordinator: impl CoreCoordinator,
    mut sysmon_source: impl SysmonSource,
    mut flash: impl PapkFlash,
) -> ! {
    transport.init();

    loop {
        if !wait_for_magic(&mut transport) {
            continue;
        }
        let cmd = transport.read_byte();
        let len = transport.read_u32_le();
        match cmd {
            CMD_PING => handle_ping(&mut transport, &flash, len),
            CMD_INSTALL => {
                let mut framed = framing::Framed(&mut transport);
                crate::install::run_install(&mut framed, &mut coordinator, &mut flash, len);
            }
            CMD_SYSMON => sysmon::handle(&mut transport, &mut sysmon_source, len),
            CMD_INPUT => input::handle(&mut transport, len),
            _ => send_response(&mut transport, STATUS_ERR, b"unknown cmd"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records what was written, and replays a scripted read stream.
    pub(super) struct MockPipe {
        pub rx: alloc::vec::Vec<u8>,
        pub pos: usize,
        pub tx: alloc::vec::Vec<u8>,
    }

    impl MockPipe {
        pub fn new(rx: alloc::vec::Vec<u8>) -> Self {
            Self {
                rx,
                pos: 0,
                tx: alloc::vec::Vec::new(),
            }
        }
    }

    impl PdbTransport for MockPipe {
        fn init(&mut self) {}
        fn read_byte(&mut self) -> u8 {
            let b = self.rx.get(self.pos).copied().unwrap_or(0);
            self.pos += 1;
            b
        }
        fn read_byte_timeout(&mut self) -> Option<u8> {
            self.rx.get(self.pos).copied().inspect(|_| self.pos += 1)
        }
        fn write_bytes(&mut self, data: &[u8]) {
            self.tx.extend_from_slice(data)
        }
        fn drain_tx(&mut self) {}
    }

    #[test]
    fn magic_is_found_after_leading_garbage() {
        let mut rx = alloc::vec![b'x', b'P', b'D', 0xFF];
        rx.extend_from_slice(FRAME_MAGIC);
        let mut p = MockPipe::new(rx);
        assert!(wait_for_magic(&mut p));
    }

    /// A partial magic that restarts mid-way must still resync — `PDBPDBP`
    /// contains a false start, and a matcher that reset to zero on mismatch
    /// rather than reconsidering the current byte would miss it.
    #[test]
    fn magic_resyncs_on_a_false_start() {
        let mut p = MockPipe::new(alloc::vec![b'P', b'D', b'P', b'D', b'B', b'P']);
        assert!(wait_for_magic(&mut p));
    }

    #[test]
    fn giving_up_is_bounded_rather_than_infinite() {
        let mut p = MockPipe::new(alloc::vec![0u8; 8]);
        // Reads past the end yield zeros forever; the loop must still stop.
        assert!(!wait_for_magic(&mut p));
    }

    struct MockFlash;
    unsafe impl PapkFlash for MockFlash {
        fn max_data_size(&self) -> usize {
            0x000F_C000
        }
        unsafe fn erase_region(&mut self, _papk_len: usize) {}
        unsafe fn write_page(&mut self, _page_index: u32, _page: &[u8; 256]) -> bool {
            true
        }
        unsafe fn commit_metadata(&mut self, _len: u32) {}
        fn trigger_reset(&mut self) -> ! {
            unreachable!("no reset in a ping test")
        }
    }

    /// The greeting bytes themselves are pinned in `pdb_protocol::greeting`;
    /// what this build must still prove is the wiring — that `handle_ping`
    /// answers with *this* firmware's framework-map-version and the flash
    /// slot's real capacity, in a frame the host-side parser accepts.
    #[test]
    fn greeting_layout_is_stable() {
        let crc = crc32_frame(CMD_PING, 0, &[]);
        let mut pipe = MockPipe::new(crc.to_le_bytes().to_vec());
        handle_ping(&mut pipe, &MockFlash, 0);

        assert_eq!(&pipe.tx[..4], FRAME_MAGIC);
        assert_eq!(pipe.tx[4], STATUS_OK);
        let len = u32::from_le_bytes(pipe.tx[5..9].try_into().unwrap()) as usize;
        let payload = &pipe.tx[9..9 + len];

        let g = greeting::Greeting::parse(payload).unwrap();
        assert_eq!(g.version, greeting::PROTOCOL_VERSION);
        assert_eq!(g.max_papk as usize, MockFlash.max_data_size());
        assert_eq!(
            g.framework_map_version,
            crate::framework_map::FRAMEWORK_MAP_VERSION
        );
        assert!(!g.is_legacy());
    }
}
