// SPDX-License-Identifier: GPL-3.0-only
//! `CMD_SYSMON` — heap and per-task statistics.
//!
//! The split here is between *where the numbers come from*, which is an RTOS
//! question answered by [`SysmonSource`], and *how they reach the host*,
//! which is a wire-format question answered by `pdb_protocol::sysmon` — the
//! layout, the encoder, and the golden test pinning them live there, next to
//! the decoder `tools/pdb` uses. What stays here besides the source trait is
//! the previous sample: the host asks "since when?" and the answer is "your
//! last query", which makes it protocol state, not a property of whatever
//! produced the numbers.

use pdb_protocol::sysmon::{encode, RESPONSE_MAX};
use pdb_protocol::{crc32_frame, CMD_SYSMON, STATUS_CRC_FAIL, STATUS_OK};

pub use pdb_protocol::sysmon::{SysmonSample, TaskSample, MAX_TASKS};

use super::{send_response, PdbTransport};

/// Where the numbers come from.
///
/// Family-specific by nature: the RP implementation is an FFI call into
/// FreeRTOS against a `TaskStatus_t` mirror pinned to that build's
/// `FreeRTOSConfig.h` — 40 bytes with core affinity, not 36 — which no other
/// family can reuse a byte of.
pub trait SysmonSource {
    /// Fill `out`. `false` if statistics are unavailable, in which case the
    /// host is told rather than shown zeros.
    fn sample(&mut self, out: &mut SysmonSample) -> bool;
}

/// The previous sample, kept so CPU share can be reported as a rate.
///
/// A `static` rather than state on the source: it belongs to the protocol
/// (the host asks "since when?" and the answer is "your last query"), not to
/// whatever produced the numbers.
static mut PREV: Option<SysmonSample> = None;

/// Handle `CMD_SYSMON`.
pub fn handle(transport: &mut impl PdbTransport, source: &mut impl SysmonSource, len: u32) {
    let wire_crc = transport.read_u32_le();
    if wire_crc != crc32_frame(CMD_SYSMON, len, &[]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return;
    }

    let mut sample = SysmonSample::default();
    if !source.sample(&mut sample) {
        send_response(transport, STATUS_OK, b"");
        return;
    }

    let mut resp = [0u8; RESPONSE_MAX];
    // SAFETY: single-threaded — only the debug-bridge task reaches this.
    let prev = unsafe { (*core::ptr::addr_of!(PREV)).as_ref() };
    let n = encode(&sample, prev, &mut resp);

    // Additive tail, like the ping greeting: an older host reads the declared
    // task count and ignores what follows. Shadowed rather than mutated so
    // the plain build has no write to `n` at all.
    //
    // Device-only: the snapshot it reads is published by the on-device
    // monitor, and a simulator has no debug bridge to serve this over anyway.
    #[cfg(all(feature = "mem-diag", not(feature = "sim")))]
    let n = {
        let (live, floor, alloc_total, largest_free) = crate::mem_diag::published_snapshot();
        let d = pdb_protocol::sysmon::MemDiag {
            live,
            floor,
            alloc_total,
            largest_free,
        };
        n + pdb_protocol::sysmon::encode_mem_diag(&d, &mut resp[n..])
    };

    send_response(transport, STATUS_OK, &resp[..n]);

    // SAFETY: as above.
    unsafe { PREV = Some(sample) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    struct Src(SysmonSample, bool);
    impl SysmonSource for Src {
        fn sample(&mut self, out: &mut SysmonSample) -> bool {
            *out = self.0;
            self.1
        }
    }

    #[test]
    fn a_bad_crc_is_rejected_before_sampling() {
        use crate::pdb::tests::MockPipe;
        let mut rx = Vec::new();
        rx.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // wrong CRC
        let mut p = MockPipe::new(rx);
        let mut src = Src(SysmonSample::default(), true);
        handle(&mut p, &mut src, 0);
        assert_eq!(p.tx[4], STATUS_CRC_FAIL);
    }

    #[test]
    fn an_unavailable_source_answers_rather_than_reporting_zeros() {
        use crate::pdb::tests::MockPipe;
        let mut rx = Vec::new();
        rx.extend_from_slice(&crc32_frame(CMD_SYSMON, 0, &[]).to_le_bytes());
        let mut p = MockPipe::new(rx);
        let mut src = Src(SysmonSample::default(), false);
        handle(&mut p, &mut src, 0);
        assert_eq!(p.tx[4], STATUS_OK);
        assert_eq!(&p.tx[5..9], &0u32.to_le_bytes(), "empty payload expected");
    }

    /// Two queries through the real handler, answered through the mock
    /// transport and read back with the shared host-side decoder: the first
    /// has no interval to rate against, the second does. This is the one test
    /// allowed to touch `PREV` — every other test in this module returns
    /// before the handler reads it, which is what keeps a parallel test run
    /// off the `static mut`.
    #[test]
    fn cpu_rates_appear_on_the_second_query() {
        use crate::pdb::tests::MockPipe;
        use pdb_protocol::sysmon::SysmonView;

        let task = |rt| {
            let mut t = [TaskSample::default(); MAX_TASKS];
            t[0] = TaskSample {
                name: *b"jvm\0\0\0\0\0\0\0\0\0\0\0\0\0",
                task_number: 5,
                run_time: rt,
                ..Default::default()
            };
            t
        };
        let crc = crc32_frame(CMD_SYSMON, 0, &[]).to_le_bytes();

        let mut p = MockPipe::new(crc.to_vec());
        let mut src = Src(
            SysmonSample {
                free_heap: 4_096,
                task_count: 1,
                total_run_time: 1_000,
                tasks: task(100),
                ..Default::default()
            },
            true,
        );
        handle(&mut p, &mut src, 0);
        let len = u32::from_le_bytes(p.tx[5..9].try_into().unwrap()) as usize;
        let v = SysmonView::parse(&p.tx[9..9 + len]).unwrap();
        assert_eq!(v.free_heap(), 4_096);
        assert_eq!(v.task(0).unwrap().name(), "jvm");
        assert_eq!(v.task(0).unwrap().cpu_permille(), None);

        // 400 of the 1000 elapsed ticks: 40.0%.
        let mut p = MockPipe::new(crc.to_vec());
        let mut src = Src(
            SysmonSample {
                task_count: 1,
                total_run_time: 2_000,
                tasks: task(500),
                ..Default::default()
            },
            true,
        );
        handle(&mut p, &mut src, 0);
        let len = u32::from_le_bytes(p.tx[5..9].try_into().unwrap()) as usize;
        let v = SysmonView::parse(&p.tx[9..9 + len]).unwrap();
        assert_eq!(v.task(0).unwrap().cpu_permille(), Some(400));
    }
}
