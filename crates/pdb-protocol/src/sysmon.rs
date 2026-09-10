// SPDX-License-Identifier: GPL-3.0-only
//! The `CMD_SYSMON` response — heap and per-task statistics.
//!
//! The device encodes with [`encode`] (plus [`encode_mem_diag`] for the
//! additive tail on mem-diag firmware); the host decodes with
//! [`SysmonView::parse`]. Where the numbers *come from* is an RTOS question
//! and stays in `picodroid-core` behind its `SysmonSource` trait, along with
//! the previous-sample state the CPU rates are computed against.
//!
//! [`tests::response_layout_is_frozen`] pins the layout byte for byte, so a
//! field that moves fails here rather than printing nonsense task names on
//! someone's terminal.

/// Most tasks reported in one response. Beyond this the table is truncated;
/// the header still carries the real count.
pub const MAX_TASKS: usize = 12;

/// Bytes of header before the per-task table.
pub const HEADER_LEN: usize = 20;
/// Bytes per task entry.
pub const ENTRY_LEN: usize = 28;
/// Bytes of the mem-diag tail, when the firmware builds one.
pub const MEM_DIAG_LEN: usize = 16;

/// Largest response [`encode`] plus [`encode_mem_diag`] can produce — size
/// wire buffers with this.
pub const RESPONSE_MAX: usize = HEADER_LEN + MAX_TASKS * ENTRY_LEN + MEM_DIAG_LEN;

/// Sentinel for "no previous sample, so no rate can be computed yet".
pub const CPU_PCT_UNAVAILABLE: u32 = 0xFFFF_FFFF;

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

/// The JVM heap block mem-diag firmware appends after the task table.
///
/// Additive, like the ping greeting's tail: an older host reads the declared
/// task count and ignores what follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemDiag {
    /// Live JVM heap bytes.
    pub live: u32,
    /// Post-GC floor, bytes.
    pub floor: u32,
    /// Allocations since boot.
    pub alloc_total: u32,
    /// Largest free native block, bytes.
    pub largest_free: u32,
}

/// Encode the mem-diag tail into `out[..MEM_DIAG_LEN]`, returning
/// [`MEM_DIAG_LEN`].
pub fn encode_mem_diag(d: &MemDiag, out: &mut [u8]) -> usize {
    out[0..4].copy_from_slice(&d.live.to_le_bytes());
    out[4..8].copy_from_slice(&d.floor.to_le_bytes());
    out[8..12].copy_from_slice(&d.alloc_total.to_le_bytes());
    out[12..16].copy_from_slice(&d.largest_free.to_le_bytes());
    MEM_DIAG_LEN
}

/// The wire meaning of a task-state byte (FreeRTOS `eTaskState` order).
pub fn state_name(state: u8) -> &'static str {
    match state {
        0 => "Running",
        1 => "Ready",
        2 => "Blocked",
        3 => "Suspended",
        4 => "Deleted",
        _ => "Unknown",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysmonError {
    /// Shorter than the fixed header; carries the actual length.
    TooShort(usize),
}

/// Zero-copy host-side view of a sysmon response.
///
/// [`parse`](Self::parse) validates only the fixed header: a truncated task
/// table is the host's to notice (via [`payload_len`](Self::payload_len) vs
/// [`expected_len`](Self::expected_len)) and report, so it can still show the
/// heap numbers it did receive.
pub struct SysmonView<'a> {
    payload: &'a [u8],
}

impl<'a> SysmonView<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, SysmonError> {
        if payload.len() < HEADER_LEN {
            return Err(SysmonError::TooShort(payload.len()));
        }
        Ok(SysmonView { payload })
    }

    fn word(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.payload[at..at + 4].try_into().unwrap())
    }

    pub fn uptime_ticks(&self) -> u32 {
        self.word(0)
    }
    pub fn free_heap(&self) -> u32 {
        self.word(4)
    }
    pub fn min_free_heap(&self) -> u32 {
        self.word(8)
    }
    pub fn total_run_time(&self) -> u32 {
        self.word(12)
    }

    /// The task count the device declared. Entries beyond what the payload
    /// actually carries make [`task`](Self::task) answer `None`.
    pub fn task_count(&self) -> usize {
        self.payload[16] as usize
    }

    /// Bytes a payload with the declared task count should occupy, tail
    /// excluded. More than [`payload_len`](Self::payload_len) means the
    /// response was truncated in the task table.
    pub fn expected_len(&self) -> usize {
        HEADER_LEN + self.task_count() * ENTRY_LEN
    }

    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }

    pub fn task(&self, i: usize) -> Option<TaskView<'a>> {
        if i >= self.task_count() {
            return None;
        }
        let base = HEADER_LEN + i * ENTRY_LEN;
        let entry = self.payload.get(base..base + ENTRY_LEN)?;
        Some(TaskView { entry })
    }

    /// The mem-diag tail, if this firmware appended one.
    pub fn mem_diag(&self) -> Option<MemDiag> {
        let b = self
            .payload
            .get(self.expected_len()..self.expected_len() + MEM_DIAG_LEN)?;
        Some(MemDiag {
            live: u32::from_le_bytes(b[0..4].try_into().unwrap()),
            floor: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            alloc_total: u32::from_le_bytes(b[8..12].try_into().unwrap()),
            largest_free: u32::from_le_bytes(b[12..16].try_into().unwrap()),
        })
    }
}

/// One task entry of a [`SysmonView`].
pub struct TaskView<'a> {
    entry: &'a [u8],
}

impl TaskView<'_> {
    /// Name with the NUL padding stripped; `"?"` if it is not UTF-8.
    pub fn name(&self) -> &str {
        let bytes = &self.entry[0..16];
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(16);
        core::str::from_utf8(&bytes[..end]).unwrap_or("?")
    }
    pub fn state(&self) -> u8 {
        self.entry[16]
    }
    pub fn current_priority(&self) -> u8 {
        self.entry[17]
    }
    pub fn base_priority(&self) -> u8 {
        self.entry[18]
    }
    pub fn stack_high_water(&self) -> u16 {
        u16::from_le_bytes([self.entry[20], self.entry[21]])
    }
    pub fn task_number(&self) -> u16 {
        u16::from_le_bytes([self.entry[22], self.entry[23]])
    }
    /// CPU share in tenths of a percent; `None` when the device had no
    /// previous sample to compute a rate against.
    pub fn cpu_permille(&self) -> Option<u32> {
        match u32::from_le_bytes(self.entry[24..28].try_into().unwrap()) {
            CPU_PCT_UNAVAILABLE => None,
            x => Some(x),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The mem-diag tail sits directly after the task table, each field a
    /// distinct value.
    #[test]
    fn mem_diag_tail_layout_is_frozen() {
        let d = MemDiag {
            live: 0x0101_0101,
            floor: 0x0202_0202,
            alloc_total: 0x0303_0303,
            largest_free: 0x0404_0404,
        };
        let mut out = [0u8; MEM_DIAG_LEN];
        assert_eq!(encode_mem_diag(&d, &mut out), MEM_DIAG_LEN);
        assert_eq!(&out[0..4], &0x0101_0101u32.to_le_bytes());
        assert_eq!(&out[4..8], &0x0202_0202u32.to_le_bytes());
        assert_eq!(&out[8..12], &0x0303_0303u32.to_le_bytes());
        assert_eq!(&out[12..16], &0x0404_0404u32.to_le_bytes());
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

    /// Encode → view round trip, mem-diag tail included: the two halves of
    /// this module must agree with each other before either can agree with
    /// the other end of the wire.
    #[test]
    fn encode_view_round_trip() {
        let s = SysmonSample {
            uptime_ticks: 111,
            free_heap: 222,
            min_free_heap: 333,
            total_run_time: 5_000,
            task_count: 2,
            tasks: {
                let mut t = [TaskSample::default(); MAX_TASKS];
                t[0] = TaskSample {
                    name: named("pdb"),
                    state: 1,
                    current_priority: 8,
                    base_priority: 6,
                    stack_high_water: 0x0123,
                    task_number: 3,
                    run_time: 500,
                };
                t[1] = TaskSample {
                    name: named("jvm"),
                    state: 0,
                    current_priority: 12,
                    base_priority: 11,
                    stack_high_water: 0x0456,
                    task_number: 4,
                    run_time: 4_500,
                };
                t
            },
        };
        let prev = sample_with(&[], 0);

        let mut out = [0u8; RESPONSE_MAX];
        let mut n = encode(&s, Some(&prev), &mut out);
        let d = MemDiag {
            live: 10,
            floor: 20,
            alloc_total: 30,
            largest_free: 40,
        };
        n += encode_mem_diag(&d, &mut out[n..]);

        let v = SysmonView::parse(&out[..n]).unwrap();
        assert_eq!(v.uptime_ticks(), 111);
        assert_eq!(v.free_heap(), 222);
        assert_eq!(v.min_free_heap(), 333);
        assert_eq!(v.total_run_time(), 5_000);
        assert_eq!(v.task_count(), 2);
        assert_eq!(v.payload_len(), v.expected_len() + MEM_DIAG_LEN);

        let t0 = v.task(0).unwrap();
        assert_eq!(t0.name(), "pdb");
        assert_eq!(t0.state(), 1);
        assert_eq!(t0.current_priority(), 8);
        assert_eq!(t0.base_priority(), 6);
        assert_eq!(t0.stack_high_water(), 0x0123);
        assert_eq!(t0.task_number(), 3);
        assert_eq!(t0.cpu_permille(), Some(100)); // 500 of 5000 ticks

        let t1 = v.task(1).unwrap();
        assert_eq!(t1.name(), "jvm");
        assert_eq!(t1.cpu_permille(), Some(900));
        assert!(v.task(2).is_none());

        assert_eq!(v.mem_diag(), Some(d));
    }

    #[test]
    fn a_view_of_a_short_payload_is_refused_not_misread() {
        assert_eq!(
            SysmonView::parse(&[0u8; 7]).err(),
            Some(SysmonError::TooShort(7))
        );
    }

    /// A payload truncated mid-table still exposes its header, declares its
    /// intended length, and answers `None` for the entries it cannot back.
    #[test]
    fn a_truncated_task_table_is_detectable_and_safe() {
        let s = sample_with(&[TaskSample::default(), TaskSample::default()], 1);
        let mut out = [0u8; RESPONSE_MAX];
        let n = encode(&s, None, &mut out);
        let cut = n - 10;

        let v = SysmonView::parse(&out[..cut]).unwrap();
        assert_eq!(v.task_count(), 2);
        assert!(v.payload_len() < v.expected_len());
        assert!(v.task(0).is_some());
        assert!(v.task(1).is_none());
        assert_eq!(v.mem_diag(), None);
    }

    /// `cpu_permille` distinguishes the sentinel from a real (huge) value.
    #[test]
    fn the_unavailable_sentinel_is_not_a_rate() {
        let s = sample_with(&[TaskSample::default()], 0);
        let mut out = [0u8; RESPONSE_MAX];
        let n = encode(&s, None, &mut out); // no prev → sentinel
        let v = SysmonView::parse(&out[..n]).unwrap();
        assert_eq!(v.task(0).unwrap().cpu_permille(), None);
    }
}
