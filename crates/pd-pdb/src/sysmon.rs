// SPDX-License-Identifier: GPL-3.0-only
//! `CMD_SYSMON` — heap and per-task statistics.
//!
//! The split here is between *where the numbers come from*, which is an RTOS
//! question answered by [`SysmonSource`], and *how they reach the host*,
//! which is a wire-format question answered by `pdb_protocol::sysmon` — the
//! layout, the encoder, and the golden test pinning them live there, next to
//! the decoder `tools/pdb` uses. What stays here besides the source trait is
//! the previous sample: the host asks "since when?" and the answer is "your
//! last query", which makes it protocol state — it rides in
//! [`crate::BridgeState`], not on whatever produced the numbers.

use pdb_protocol::sysmon::{encode, encode_mem_diag, RESPONSE_MAX};
use pdb_protocol::{crc32_frame, CMD_SYSMON, STATUS_CRC_FAIL, STATUS_OK};

pub use pdb_protocol::sysmon::{SysmonSample, TaskSample, MAX_TASKS};

use crate::{send_response, PdbDevice, PdbTransport};

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

/// Handle `CMD_SYSMON`. `prev` is the previous sample, kept so CPU share can
/// be reported as a rate; it is replaced by this one.
pub fn handle(
    transport: &mut impl PdbTransport,
    source: &mut impl SysmonSource,
    device: &impl PdbDevice,
    prev: &mut Option<SysmonSample>,
    len: u32,
) {
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
    let mut n = encode(&sample, prev.as_ref(), &mut resp);

    // Additive tail, like the ping greeting: an older host reads the declared
    // task count and ignores what follows. Only a build with the on-device
    // memory monitor has one to append.
    if let Some(d) = device.mem_diag() {
        n += encode_mem_diag(&d, &mut resp[n..]);
    }

    send_response(transport, STATUS_OK, &resp[..n]);
    *prev = Some(sample);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_device::Stub;
    use crate::vec_pipe::VecPipe;
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
        let mut rx = Vec::new();
        rx.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // wrong CRC
        let mut p = VecPipe::new(rx);
        let mut src = Src(SysmonSample::default(), true);
        let mut prev = None;
        handle(&mut p, &mut src, &Stub::<true>::new(8), &mut prev, 0);
        assert_eq!(p.tx[4], STATUS_CRC_FAIL);
        assert!(prev.is_none());
    }

    #[test]
    fn an_unavailable_source_answers_rather_than_reporting_zeros() {
        let mut rx = Vec::new();
        rx.extend_from_slice(&crc32_frame(CMD_SYSMON, 0, &[]).to_le_bytes());
        let mut p = VecPipe::new(rx);
        let mut src = Src(SysmonSample::default(), false);
        let mut prev = None;
        handle(&mut p, &mut src, &Stub::<true>::new(8), &mut prev, 0);
        assert_eq!(p.tx[4], STATUS_OK);
        assert_eq!(&p.tx[5..9], &0u32.to_le_bytes(), "empty payload expected");
    }

    /// Two queries through the real handler, answered through the mock
    /// transport and read back with the shared host-side decoder: the first
    /// has no interval to rate against, the second does.
    #[test]
    fn cpu_rates_appear_on_the_second_query() {
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
        let device = Stub::<true>::new(8);
        let mut prev = None;

        let mut p = VecPipe::new(crc.to_vec());
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
        handle(&mut p, &mut src, &device, &mut prev, 0);
        let len = u32::from_le_bytes(p.tx[5..9].try_into().unwrap()) as usize;
        let v = SysmonView::parse(&p.tx[9..9 + len]).unwrap();
        assert_eq!(v.free_heap(), 4_096);
        assert_eq!(v.task(0).unwrap().name(), "jvm");
        assert_eq!(v.task(0).unwrap().cpu_permille(), None);
        assert!(prev.is_some());

        // 400 of the 1000 elapsed ticks: 40.0%.
        let mut p = VecPipe::new(crc.to_vec());
        let mut src = Src(
            SysmonSample {
                task_count: 1,
                total_run_time: 2_000,
                tasks: task(500),
                ..Default::default()
            },
            true,
        );
        handle(&mut p, &mut src, &device, &mut prev, 0);
        let len = u32::from_le_bytes(p.tx[5..9].try_into().unwrap()) as usize;
        let v = SysmonView::parse(&p.tx[9..9 + len]).unwrap();
        assert_eq!(v.task(0).unwrap().cpu_permille(), Some(400));
    }
}
