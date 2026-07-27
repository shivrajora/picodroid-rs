// SPDX-License-Identifier: GPL-3.0-only
//! `CMD_SYSMON` — heap and per-task statistics.
//!
//! The split here is between *where the numbers come from*, which is an RTOS
//! question, and *how they reach the host*, which is a wire-format question.
//! [`SysmonSource`] is the first; everything else in this module is the
//! second, including the CPU-percentage arithmetic, which is a delta between
//! two samples and belongs with the thing that keeps the previous one.
//!
//! The response layout is a contract with `tools/pdb`, which parses it by
//! offset. [`tests::response_layout_is_frozen`] pins it byte for byte, so a
//! field that moves fails here rather than printing nonsense task names on
//! someone's terminal.

use pdb_protocol::{crc32_frame, CMD_SYSMON, STATUS_CRC_FAIL, STATUS_OK};

use super::{send_response, PdbTransport};

/// Most tasks reported in one response. Beyond this the table is truncated;
/// the header still carries the real count.
pub const MAX_TASKS: usize = 12;

/// Bytes of header before the per-task table.
const HEADER_LEN: usize = 20;
/// Bytes per task entry.
const ENTRY_LEN: usize = 28;
/// Bytes of the `mem-diag` tail, when built.
const MEM_DIAG_LEN: usize = 16;

/// Sentinel for "no previous sample, so no rate can be computed yet".
const CPU_PCT_UNAVAILABLE: u32 = 0xFFFF_FFFF;

/// One task, as the host sees it.
#[derive(Clone, Copy, Default)]
pub struct TaskSample {
    /// Name, NUL-padded. Longer names are truncated to fit the wire field.
    pub name: [u8; 16],
    pub state: u8,
    pub current_priority: u8,
    pub base_priority: u8,
    /// Stack head-room in words, as the RTOS reports it.
    pub stack_high_water: u16,
    /// Stable per-task id, used to match a task against the previous sample.
    pub task_number: u16,
    /// Cumulative run time in the RTOS's own units; only deltas are used.
    pub run_time: u32,
}

/// One whole-system sample.
#[derive(Clone, Copy)]
pub struct SysmonSample {
    pub uptime_ticks: u32,
    pub free_heap: u32,
    pub min_free_heap: u32,
    pub total_run_time: u32,
    pub task_count: u8,
    pub tasks: [TaskSample; MAX_TASKS],
}

impl Default for SysmonSample {
    fn default() -> Self {
        Self {
            uptime_ticks: 0,
            free_heap: 0,
            min_free_heap: 0,
            total_run_time: 0,
            task_count: 0,
            tasks: [TaskSample::default(); MAX_TASKS],
        }
    }
}

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

/// CPU share as tenths of a percent, over the interval since the previous
/// sample.
///
/// Wrapping subtraction throughout: RTOS run-time counters are free-running
/// `u32`s and will wrap during any long-lived session. A wrapped interval
/// still yields the right delta.
fn compute_cpu_pct(prev: &SysmonSample, task_number: u16, current: u32, current_total: u32) -> u32 {
    let delta_total = current_total.wrapping_sub(prev.total_run_time);
    if delta_total == 0 {
        return 0;
    }
    let prev_runtime = prev.tasks[..prev.task_count as usize]
        .iter()
        .find(|t| t.task_number == task_number)
        .map(|t| t.run_time)
        .unwrap_or(0);
    let delta_task = current.wrapping_sub(prev_runtime);
    // u64 so the ×1000 cannot overflow before the divide.
    ((delta_task as u64 * 1000) / delta_total as u64) as u32
}

/// Encode `sample` into `out`, returning the number of bytes written.
///
/// Layout, all little-endian:
///
/// ```text
/// [0..4]   uptime ticks      [4..8]   free heap
/// [8..12]  min free heap     [12..16] total run time
/// [16]     task count        [17..20] reserved
/// then task_count × 28:
/// [+0..16] name (NUL-padded) [+16] state  [+17] priority  [+18] base priority
/// [+19]    reserved          [+20..22] stack high-water   [+22..24] task number
/// [+24..28] CPU ‰
/// ```
pub fn encode(sample: &SysmonSample, prev: Option<&SysmonSample>, out: &mut [u8]) -> usize {
    let task_count = (sample.task_count as usize).min(MAX_TASKS);

    out[0..4].copy_from_slice(&sample.uptime_ticks.to_le_bytes());
    out[4..8].copy_from_slice(&sample.free_heap.to_le_bytes());
    out[8..12].copy_from_slice(&sample.min_free_heap.to_le_bytes());
    out[12..16].copy_from_slice(&sample.total_run_time.to_le_bytes());
    out[16] = task_count as u8;
    out[17..20].fill(0);

    for (i, t) in sample.tasks.iter().take(task_count).enumerate() {
        let base = HEADER_LEN + i * ENTRY_LEN;
        out[base..base + 16].copy_from_slice(&t.name);
        out[base + 16] = t.state;
        out[base + 17] = t.current_priority;
        out[base + 18] = t.base_priority;
        out[base + 19] = 0;
        out[base + 20..base + 22].copy_from_slice(&t.stack_high_water.to_le_bytes());
        out[base + 22..base + 24].copy_from_slice(&t.task_number.to_le_bytes());

        let cpu = match prev {
            Some(p) => compute_cpu_pct(p, t.task_number, t.run_time, sample.total_run_time),
            None => CPU_PCT_UNAVAILABLE,
        };
        out[base + 24..base + 28].copy_from_slice(&cpu.to_le_bytes());
    }

    HEADER_LEN + task_count * ENTRY_LEN
}

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

    let mut resp = [0u8; HEADER_LEN + MAX_TASKS * ENTRY_LEN + MEM_DIAG_LEN];
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
        resp[n..n + 4].copy_from_slice(&live.to_le_bytes());
        resp[n + 4..n + 8].copy_from_slice(&floor.to_le_bytes());
        resp[n + 8..n + 12].copy_from_slice(&alloc_total.to_le_bytes());
        resp[n + 12..n + 16].copy_from_slice(&largest_free.to_le_bytes());
        n + MEM_DIAG_LEN
    };

    send_response(transport, STATUS_OK, &resp[..n]);

    // SAFETY: as above.
    unsafe { PREV = Some(sample) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn named(name: &str) -> [u8; 16] {
        let mut n = [0u8; 16];
        let b = name.as_bytes();
        n[..b.len()].copy_from_slice(b);
        n
    }

    fn sample_with(tasks: &[TaskSample], total: u32) -> SysmonSample {
        let mut s = SysmonSample {
            total_run_time: total,
            task_count: tasks.len() as u8,
            ..Default::default()
        };
        s.tasks[..tasks.len()].copy_from_slice(tasks);
        s
    }

    /// `tools/pdb` reads this response by fixed offset. Any field that moves
    /// breaks it silently — a shifted name field prints garbage, a shifted
    /// priority prints a plausible wrong number. So the bytes are pinned
    /// literally, and a layout change has to be made here on purpose.
    #[test]
    fn response_layout_is_frozen() {
        let s = SysmonSample {
            uptime_ticks: 0x1122_3344,
            free_heap: 0x0000_1000,
            min_free_heap: 0x0000_0800,
            total_run_time: 0x0001_0000,
            task_count: 1,
            tasks: {
                let mut t = [TaskSample::default(); MAX_TASKS];
                // Every field gets a distinct value on purpose: with two
                // fields sharing one, swapping them is invisible and the
                // test silently stops guarding the layout.
                t[0] = TaskSample {
                    name: named("jvm"),
                    state: 2,
                    current_priority: 15,
                    base_priority: 9,
                    stack_high_water: 0x0ABC,
                    task_number: 7,
                    run_time: 0,
                };
                t
            },
        };

        let mut out = [0u8; HEADER_LEN + MAX_TASKS * ENTRY_LEN];
        let n = encode(&s, None, &mut out);
        assert_eq!(n, HEADER_LEN + ENTRY_LEN);

        assert_eq!(&out[0..4], &0x1122_3344u32.to_le_bytes());
        assert_eq!(&out[4..8], &0x0000_1000u32.to_le_bytes());
        assert_eq!(&out[8..12], &0x0000_0800u32.to_le_bytes());
        assert_eq!(&out[12..16], &0x0001_0000u32.to_le_bytes());
        assert_eq!(out[16], 1);
        assert_eq!(&out[17..20], &[0, 0, 0]);

        assert_eq!(&out[20..36], &named("jvm"));
        assert_eq!(out[36], 2);
        assert_eq!(out[37], 15);
        assert_eq!(out[38], 9);
        assert_eq!(out[39], 0);
        assert_eq!(&out[40..42], &0x0ABCu16.to_le_bytes());
        assert_eq!(&out[42..44], &7u16.to_le_bytes());
        // No previous sample, so the rate is explicitly unavailable rather
        // than a misleading zero.
        assert_eq!(&out[44..48], &CPU_PCT_UNAVAILABLE.to_le_bytes());
    }

    #[test]
    fn cpu_share_is_a_rate_over_the_interval() {
        let t = |num, rt| TaskSample {
            task_number: num,
            run_time: rt,
            ..Default::default()
        };
        let prev = sample_with(&[t(1, 1_000), t(2, 0)], 10_000);
        // Task 1 used 250 of the 1000 ticks that elapsed: 25.0%.
        let now = sample_with(&[t(1, 1_250), t(2, 0)], 11_000);

        let mut out = [0u8; HEADER_LEN + MAX_TASKS * ENTRY_LEN];
        encode(&now, Some(&prev), &mut out);
        assert_eq!(
            u32::from_le_bytes(out[44..48].try_into().unwrap()),
            250,
            "CPU share is reported in tenths of a percent"
        );
    }

    /// Run-time counters are free-running u32s and wrap on any long session.
    /// A subtraction that panicked or produced a huge value here would make
    /// sysmon useless exactly when a device has been up long enough to be
    /// worth inspecting.
    #[test]
    fn a_wrapped_runtime_counter_still_yields_the_right_delta() {
        let t = |num, rt| TaskSample {
            task_number: num,
            run_time: rt,
            ..Default::default()
        };
        let prev = sample_with(&[t(1, u32::MAX - 100)], u32::MAX - 400);
        // 500 ticks elapsed in total, of which this task used 200.
        let now = sample_with(&[t(1, (u32::MAX - 100).wrapping_add(200))], 99);

        let mut out = [0u8; HEADER_LEN + MAX_TASKS * ENTRY_LEN];
        encode(&now, Some(&prev), &mut out);
        assert_eq!(u32::from_le_bytes(out[44..48].try_into().unwrap()), 400);
    }

    #[test]
    fn a_task_absent_from_the_previous_sample_reads_as_all_new_time() {
        let t = |num, rt| TaskSample {
            task_number: num,
            run_time: rt,
            ..Default::default()
        };
        let prev = sample_with(&[t(1, 0)], 0);
        let now = sample_with(&[t(9, 100)], 1_000); // task 9 is new
        let mut out = [0u8; HEADER_LEN + MAX_TASKS * ENTRY_LEN];
        encode(&now, Some(&prev), &mut out);
        assert_eq!(u32::from_le_bytes(out[44..48].try_into().unwrap()), 100);
    }

    /// Two queries with no time between them must not divide by zero.
    #[test]
    fn a_zero_length_interval_reports_zero_rather_than_dividing() {
        let t = TaskSample {
            task_number: 1,
            run_time: 5,
            ..Default::default()
        };
        let prev = sample_with(&[t], 1_000);
        let now = sample_with(&[t], 1_000);
        let mut out = [0u8; HEADER_LEN + MAX_TASKS * ENTRY_LEN];
        encode(&now, Some(&prev), &mut out);
        assert_eq!(u32::from_le_bytes(out[44..48].try_into().unwrap()), 0);
    }

    /// More tasks than the table holds must truncate to the buffer, and the
    /// declared count must match what was actually written — otherwise the
    /// host walks off the end of the payload.
    #[test]
    fn an_oversized_task_list_truncates_consistently() {
        let mut s = SysmonSample {
            task_count: (MAX_TASKS + 5) as u8,
            ..Default::default()
        };
        for (i, t) in s.tasks.iter_mut().enumerate() {
            t.task_number = i as u16;
        }
        let mut out = [0u8; HEADER_LEN + MAX_TASKS * ENTRY_LEN];
        let n = encode(&s, None, &mut out);
        assert_eq!(n, HEADER_LEN + MAX_TASKS * ENTRY_LEN);
        assert_eq!(out[16] as usize, MAX_TASKS);
    }

    /// A long task name is truncated to the wire field, never overruns it.
    #[test]
    fn a_long_task_name_fills_the_field_without_overrunning() {
        let s = sample_with(
            &[TaskSample {
                name: named("0123456789abcdef"),
                ..Default::default()
            }],
            0,
        );
        let mut out = [0u8; HEADER_LEN + MAX_TASKS * ENTRY_LEN];
        encode(&s, None, &mut out);
        assert_eq!(&out[20..36], b"0123456789abcdef");
        // The next field must still be the state byte, not name spill.
        assert_eq!(out[36], 0);
    }

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
}
