// SPDX-License-Identifier: GPL-3.0-only
//! The device half of PDBP — the picodroid debug bridge.
//!
//! Everything here is protocol: resync on a frame magic, dispatch a command
//! byte, verify a CRC, answer. The bytes themselves come from
//! [`PdbTransport`], which is the only family-specific part — a USB CDC pipe
//! on the RP boards, a socket in the simulator. What the firmware knows and
//! the protocol does not — the board, the serial, the package directory,
//! how to inject input — comes through [`PdbDevice`].
//!
//! # Why this is a crate
//!
//! `tools/pdb` speaks exactly one dialect; a family that framed a sysmon
//! response differently would not fail loudly, it would print garbage task
//! names. The constants live in `pdb-protocol` so both ends agree on them;
//! this crate is the other half of that argument, and it sits outside
//! `picodroid-core` so the host can test the loop and fuzz the byte stream
//! it reads (`docs/fuzzing.md`, target `pdb_stream`).
//!
//! # Why generics rather than `__pd_*` shims
//!
//! One caller — the family's debug-bridge task — and no shared statics
//! needing family types. A generic parameter adds no link surface, needs no
//! cfg-gating on boards that ship no bridge at all, and monomorphises in the
//! family crate where that single caller already is. The two multi-app
//! verbs are behind [`PdbDevice::MULTI_APP`], a constant, so a single-app
//! image folds them away as the `cfg` they replaced did.
//!
//! # One frame at a time
//!
//! [`serve_one`] reads and answers one frame; [`run_pdb_task`] loops over
//! it. A committed install or uninstall is reported as `reset: true` rather
//! than resetting inside, so a host test can watch the frame that would
//! have rebooted the device and carry on.

#![cfg_attr(not(any(test, feature = "vec-pipe")), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

extern crate alloc;

mod device;
pub mod framing;
pub mod input;
pub mod packages;
pub mod ping;
pub mod sysmon;
#[cfg(any(test, feature = "vec-pipe"))]
pub mod vec_pipe;

pub use device::{PdbDevice, UninstallRefusal};
pub use framing::send_response;
pub use sysmon::{SysmonSample, SysmonSource, TaskSample, MAX_TASKS};

use pd_install::{CoreCoordinator, PageBuf, PapkFlash};
use pdb_protocol::{
    CMD_INPUT, CMD_INSTALL, CMD_LIST, CMD_PING, CMD_SYSMON, CMD_UNINSTALL, FRAME_MAGIC, STATUS_ERR,
    STATUS_TOO_LARGE,
};

/// A byte pipe to the host, framed by this crate.
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
pub const RESYNC_LIMIT: usize = 65536;

/// Block until the frame magic is seen. `false` if [`RESYNC_LIMIT`] bytes
/// went by without one.
pub fn wait_for_magic(transport: &mut impl PdbTransport) -> bool {
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

/// Answer `STATUS_TOO_LARGE` to a request whose `len` exceeds what its verb
/// can carry, without reading the payload. `true` if refused.
///
/// The unread payload and CRC are then garbage to [`wait_for_magic`], which
/// skips them on the way to the next frame; draining `len` bytes instead
/// would block for as many bytes as a hostile `u32` names.
pub fn refuse_oversized(transport: &mut impl PdbTransport, cmd: u8, len: u32) -> bool {
    match pdb_protocol::max_request_payload(cmd) {
        Some(max) if len > max => {
            send_response(transport, STATUS_TOO_LARGE, b"payload too large");
            true
        }
        _ => false,
    }
}

/// What the loop carries from one frame to the next.
pub struct BridgeState {
    /// The install's flash-page staging buffer. The bridge owns it, so an
    /// install borrows it for the call and no two installs can share it.
    pub page: PageBuf,
    /// The previous sysmon sample, so CPU share can be reported as a rate.
    /// Protocol state, not a property of whatever produced the numbers: the
    /// host asks "since when?" and the answer is "your last query".
    pub prev_sample: Option<SysmonSample>,
}

impl BridgeState {
    pub const fn new() -> Self {
        Self {
            page: [0u8; 256],
            prev_sample: None,
        }
    }
}

impl Default for BridgeState {
    fn default() -> Self {
        Self::new()
    }
}

/// What one call of [`serve_one`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Served {
    /// [`RESYNC_LIMIT`] bytes went by without a frame magic; nothing sent.
    NoMagic,
    /// One frame was dispatched. `reset` when an install or uninstall
    /// committed and the caller must reboot the device.
    Answered { cmd: u8, reset: bool },
}

/// Read one frame off `transport` and answer it.
pub fn serve_one<T, C, S, F, D>(
    transport: &mut T,
    coordinator: &mut C,
    sysmon: &mut S,
    flash: &mut F,
    device: &mut D,
    state: &mut BridgeState,
) -> Served
where
    T: PdbTransport,
    C: CoreCoordinator,
    S: SysmonSource,
    F: PapkFlash,
    D: PdbDevice,
{
    if !wait_for_magic(transport) {
        return Served::NoMagic;
    }
    let cmd = transport.read_byte();
    let len = transport.read_u32_le();
    let mut reset = false;
    if refuse_oversized(transport, cmd, len) {
        return Served::Answered { cmd, reset };
    }
    match cmd {
        CMD_PING => ping::handle(transport, flash, device, len),
        CMD_INSTALL => {
            let mut framed = framing::Framed::<T, D>::new(transport);
            reset = pd_install::install(
                &mut framed,
                coordinator,
                flash,
                device,
                &mut state.page,
                D::FRAMEWORK_MAP_VERSION,
                len,
            );
        }
        CMD_SYSMON => sysmon::handle(transport, sysmon, device, &mut state.prev_sample, len),
        CMD_INPUT => input::handle(transport, device, len),
        CMD_LIST if D::MULTI_APP => packages::handle_list(transport, device, len),
        CMD_UNINSTALL if D::MULTI_APP => {
            reset = packages::handle_uninstall(transport, coordinator, flash, device, len);
        }
        _ => send_response(transport, STATUS_ERR, b"unknown cmd"),
    }
    Served::Answered { cmd, reset }
}

/// Run the debug bridge. Never returns.
///
/// The family supplies how bytes reach the host, how to park the JVM core
/// for a flash write, where task statistics come from and what the app
/// region is; the firmware supplies the rest through `device`.
pub fn run_pdb_task(
    mut transport: impl PdbTransport,
    mut coordinator: impl CoreCoordinator,
    mut sysmon: impl SysmonSource,
    mut flash: impl PapkFlash,
    mut device: impl PdbDevice,
) -> ! {
    transport.init();
    let mut state = BridgeState::new();
    loop {
        if let Served::Answered { reset: true, .. } = serve_one(
            &mut transport,
            &mut coordinator,
            &mut sysmon,
            &mut flash,
            &mut device,
            &mut state,
        ) {
            // The install or uninstall reported its outcome and drained the
            // pipe; the reboot it promised comes now.
            flash.trigger_reset()
        }
    }
}

#[cfg(test)]
pub(crate) mod test_device {
    //! A `PdbDevice` over a tiny in-memory directory, for this crate's tests.

    use alloc::string::String;
    use alloc::vec::Vec;

    use pd_install::{run_sectors, PackageDirectory, PapkFlash, Plan, PlanError};
    use pdb_protocol::input::InputEvent;

    use crate::{PdbDevice, UninstallRefusal};

    pub struct Stub<const MULTI: bool> {
        pub installed: Vec<(String, u32, u32)>,
        pub pending: Option<(String, u32, u32)>,
        pub injected: Vec<InputEvent>,
        pub sectors: u32,
    }

    impl<const MULTI: bool> Stub<MULTI> {
        pub fn new(sectors: u32) -> Self {
            Self {
                installed: Vec::new(),
                pending: None,
                injected: Vec::new(),
                sectors,
            }
        }
    }

    impl<const MULTI: bool> PackageDirectory for Stub<MULTI> {
        const CAN_COMPACT: bool = false;
        fn rescan(&mut self, _flash: &impl PapkFlash) {
            if let Some(p) = self.pending.take() {
                self.installed.push(p);
            }
        }
        fn mark_verified(&mut self, _flash: &mut impl PapkFlash) {}
        fn plan_install(
            &mut self,
            package: Option<&str>,
            papk_len: usize,
            _max_apps: usize,
        ) -> Result<Plan, PlanError> {
            let need = run_sectors(papk_len);
            // First fit after the last installed run.
            let first = self
                .installed
                .iter()
                .map(|(_, f, s)| f + s)
                .max()
                .unwrap_or(0);
            if papk_len == 0 || first + need > self.sectors {
                return Err(PlanError::TooLarge);
            }
            self.pending = Some((String::from(package.unwrap_or("")), first, need));
            Ok(Plan {
                first_sector: first,
                erase_before: (first, need),
                evict_after: None,
                flags: 0,
                seq: 1,
                compact_first: false,
            })
        }
        fn compact(&mut self, _flash: &mut impl PapkFlash) {}
        fn free_space(&self) -> (u32, u32) {
            let used: u32 = self.installed.iter().map(|(_, _, s)| s).sum();
            (self.sectors - used, self.sectors - used)
        }
        fn installed_count(&self) -> u32 {
            self.installed.len() as u32
        }
    }

    impl<const MULTI: bool> PdbDevice for Stub<MULTI> {
        const MULTI_APP: bool = MULTI;
        const FRAMEWORK_MAP_VERSION: &'static str = "0.40.0";
        fn board(&self) -> &str {
            "stub"
        }
        fn serial(&self, out: &mut [u8; pdb_protocol::greeting::SERIAL_MAX]) -> usize {
            out[..8].copy_from_slice(b"STUB0001");
            8
        }
        fn inject(&mut self, event: InputEvent) -> (u8, &'static [u8]) {
            self.injected.push(event);
            (pdb_protocol::STATUS_OK, b"")
        }
        fn write_list(&self, out: &mut dyn core::fmt::Write) {
            for (p, f, s) in &self.installed {
                let _ = writeln!(out, "{f}\t{p}\t1\t1.0\t{}\t-\t{p}", s * 4096);
            }
        }
        fn uninstall_target(&self, package: &str) -> Result<(u32, u32), UninstallRefusal> {
            self.installed
                .iter()
                .find(|(p, _, _)| p == package)
                .map(|(_, f, s)| (*f, *s))
                .ok_or(UninstallRefusal::NotInstalled)
        }
        fn package_removed(&mut self, package: &str) {
            self.installed.retain(|(p, _, _)| p != package);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vec_pipe::VecPipe;
    use alloc::vec::Vec;
    use pdb_protocol::{crc32_frame, greeting, STATUS_OK};

    #[test]
    fn magic_is_found_after_leading_garbage() {
        let mut rx = alloc::vec![b'x', b'P', b'D', 0xFF];
        rx.extend_from_slice(FRAME_MAGIC);
        let mut p = VecPipe::new(rx);
        assert!(wait_for_magic(&mut p));
    }

    /// A partial magic that restarts mid-way must still resync — `PDBPDBP`
    /// contains a false start, and a matcher that reset to zero on mismatch
    /// rather than reconsidering the current byte would miss it.
    #[test]
    fn magic_resyncs_on_a_false_start() {
        let mut p = VecPipe::new(alloc::vec![b'P', b'D', b'P', b'D', b'B', b'P']);
        assert!(wait_for_magic(&mut p));
    }

    #[test]
    fn giving_up_is_bounded_rather_than_infinite() {
        let mut p = VecPipe::new(alloc::vec![0u8; 8]);
        // Reads past the end yield zeros forever; the loop must still stop.
        assert!(!wait_for_magic(&mut p));
    }

    /// A `len` its verb cannot carry is answered before a byte of payload is
    /// read: draining `u32::MAX` bytes would park the bridge indefinitely.
    #[test]
    fn an_oversized_len_is_refused_unread() {
        let mut p = VecPipe::new(Vec::new());
        assert!(refuse_oversized(&mut p, CMD_INPUT, u32::MAX));
        assert_eq!(p.pos, 0, "nothing read");
        assert_eq!(&p.tx[..4], FRAME_MAGIC);
        assert_eq!(p.tx[4], STATUS_TOO_LARGE);

        let mut p = VecPipe::new(Vec::new());
        assert!(refuse_oversized(&mut p, CMD_PING, 1));
        let max_input = pdb_protocol::input::MAX_INPUT_PAYLOAD as u32;
        for (cmd, len) in [(CMD_PING, 0), (CMD_SYSMON, 0), (CMD_INPUT, max_input)] {
            let mut p = VecPipe::new(Vec::new());
            assert!(!refuse_oversized(&mut p, cmd, len), "cmd {cmd} len {len}");
            assert!(p.tx.is_empty());
        }
        // The install header's `len` is the PAPK size, bounded elsewhere.
        let mut p = VecPipe::new(Vec::new());
        assert!(!refuse_oversized(&mut p, CMD_INSTALL, u32::MAX));
    }

    fn frame(cmd: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = FRAME_MAGIC.to_vec();
        v.push(cmd);
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(payload);
        v.extend_from_slice(&crc32_frame(cmd, payload.len() as u32, payload).to_le_bytes());
        v
    }

    /// A PING through `serve_one`: the greeting carries the device's facts
    /// and, on a multi-app device, the directory's; a single-app device
    /// answers "unknown cmd" to LIST.
    #[test]
    fn serve_one_answers_ping_and_folds_the_multi_app_verbs() {
        use pd_install::mem_region::{MemRegion, NoCoordinator};
        struct NoSysmon;
        impl SysmonSource for NoSysmon {
            fn sample(&mut self, _out: &mut SysmonSample) -> bool {
                false
            }
        }
        let mut region = MemRegion::new(8, 8);
        let mut state = BridgeState::new();

        let mut multi = test_device::Stub::<true>::new(8);
        let mut rx = frame(CMD_PING, b"");
        rx.extend_from_slice(&frame(CMD_LIST, b""));
        let mut p = VecPipe::new(rx);
        let served = serve_one(
            &mut p,
            &mut NoCoordinator,
            &mut NoSysmon,
            &mut region,
            &mut multi,
            &mut state,
        );
        assert_eq!(
            served,
            Served::Answered {
                cmd: CMD_PING,
                reset: false
            }
        );
        assert_eq!(p.tx[4], STATUS_OK);
        let len = u32::from_le_bytes(p.tx[5..9].try_into().unwrap()) as usize;
        let g = greeting::Greeting::parse(&p.tx[9..9 + len]).unwrap();
        assert_eq!(g.board, "stub");
        assert_eq!(g.serial, "STUB0001");
        assert_eq!(g.framework_map_version, "0.40.0");
        assert_eq!(g.max_papk as usize, region.max_data_size());
        let apps = g.apps.expect("a multi-app greeting carries the directory");
        assert_eq!((apps.max, apps.installed), (8, 0));
        p.tx.clear();
        let served = serve_one(
            &mut p,
            &mut NoCoordinator,
            &mut NoSysmon,
            &mut region,
            &mut multi,
            &mut state,
        );
        assert_eq!(
            served,
            Served::Answered {
                cmd: CMD_LIST,
                reset: false
            }
        );
        assert_eq!(p.tx[4], STATUS_OK);

        let mut single = test_device::Stub::<false>::new(8);
        let mut p = VecPipe::new(frame(CMD_LIST, b""));
        serve_one(
            &mut p,
            &mut NoCoordinator,
            &mut NoSysmon,
            &mut region,
            &mut single,
            &mut state,
        );
        assert_eq!(p.tx[4], STATUS_ERR);
        assert!(p.tx.ends_with(b"unknown cmd"));
    }
}
