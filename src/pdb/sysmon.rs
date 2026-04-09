use core::ffi::c_void;
use core::mem::MaybeUninit;

use freertos_rust::{freertos_rs_xTaskGetTickCount, FreeRtosUBaseType};

use super::cdc_transport::CdcTransport;
use super::protocol::{crc32_frame, CMD_SYSMON, STATUS_CRC_FAIL, STATUS_OK};

// ── FFI ─────────────────────────────────────────────────────────────────────

extern "C" {
    fn xPortGetFreeHeapSize() -> u32;
    fn xPortGetMinimumEverFreeHeapSize() -> u32;
    fn uxTaskGetSystemState(
        pxTaskStatusArray: *mut TaskStatusC,
        uxArraySize: FreeRtosUBaseType,
        pulTotalRunTime: *mut u32,
    ) -> FreeRtosUBaseType;
    fn uxTaskGetNumberOfTasks() -> FreeRtosUBaseType;
}

/// C-compatible `TaskStatus_t` matching the FreeRTOS layout for this build:
///   configUSE_CORE_AFFINITY = 1, configNUMBER_OF_CORES = 2
///   configSTACK_DEPTH_TYPE  = StackType_t (u32 on ARM)
///   configRUN_TIME_COUNTER_TYPE = uint32_t
///   portSTACK_GROWTH = -1, configRECORD_STACK_HIGH_ADDRESS undefined
///
/// The upstream `FreeRtosTaskStatusFfi` is missing `uxCoreAffinityMask` and
/// uses u16 for `stack_high_water_mark`, so its size (36 bytes) does not match
/// the real C struct (40 bytes).  Using the wrong size causes buffer overflow
/// and a hard fault when `uxTaskGetSystemState` fills the array.
#[repr(C)]
struct TaskStatusC {
    handle: *const c_void,
    task_name: *const u8,
    task_number: u32,
    task_state: u32, // eTaskState (C enum = 4 bytes on ARM)
    current_priority: u32,
    base_priority: u32,
    run_time_counter: u32,
    stack_base: *const c_void,
    stack_high_water_mark: u32, // configSTACK_DEPTH_TYPE = StackType_t = u32
    core_affinity_mask: u32,    // present when SMP (configNUMBER_OF_CORES > 1)
}

// ── Snapshot for CPU % computation ──────────────────────────────────────────

const MAX_TASKS: usize = 12;

#[derive(Clone, Copy)]
struct TaskRuntime {
    task_number: u16,
    run_time: u32,
}

struct RuntimeSnapshot {
    valid: bool,
    total_run_time: u32,
    task_count: u8,
    tasks: [TaskRuntime; MAX_TASKS],
}

impl RuntimeSnapshot {
    const fn empty() -> Self {
        Self {
            valid: false,
            total_run_time: 0,
            task_count: 0,
            tasks: [TaskRuntime {
                task_number: 0,
                run_time: 0,
            }; MAX_TASKS],
        }
    }
}

/// Previous snapshot, cached from the last CMD_SYSMON query.
/// Only accessed from handle_sysmon() which runs in the PDB task on core 1.
static mut PREV_SNAPSHOT: RuntimeSnapshot = RuntimeSnapshot::empty();

// ── PDB command handler ─────────────────────────────────────────────────────

/// Handle CMD_SYSMON: collect system stats and send a binary response.
///
/// CPU % is computed from the delta between the current query and the previous
/// one.  The first query returns `0xFFFFFFFF` (N/A) for CPU %; subsequent
/// queries show the CPU usage over the interval since the last query.
pub fn handle_sysmon(len: u32) {
    // Validate CRC (empty payload)
    let wire_crc = crate::hal::pdb_usb::queue_read_u32_le();
    let expected_crc = crc32_frame(CMD_SYSMON, len, &[]);
    if wire_crc != expected_crc {
        CdcTransport::send_pdbp_response(STATUS_CRC_FAIL, b"");
        return;
    }

    // Collect current task state
    let mut task_buf: [MaybeUninit<TaskStatusC>; MAX_TASKS] =
        [const { MaybeUninit::uninit() }; MAX_TASKS];
    let mut total_run_time: u32 = 0;

    let task_count = unsafe {
        let n = uxTaskGetNumberOfTasks() as usize;
        uxTaskGetSystemState(
            task_buf.as_mut_ptr().cast(),
            n.min(MAX_TASKS) as FreeRtosUBaseType,
            &mut total_run_time,
        )
    } as usize;
    let task_count = task_count.min(MAX_TASKS);

    // Read previous snapshot for CPU % delta
    let prev = unsafe { &*core::ptr::addr_of!(PREV_SNAPSHOT) };

    // Build response: 20-byte header + task_count * 28-byte entries
    let resp_len = 20 + task_count * 28;
    let mut resp = [0u8; 20 + MAX_TASKS * 28];

    // Header
    let uptime = unsafe { freertos_rs_xTaskGetTickCount() };
    let free_heap = unsafe { xPortGetFreeHeapSize() };
    let min_free_heap = unsafe { xPortGetMinimumEverFreeHeapSize() };

    resp[0..4].copy_from_slice(&uptime.to_le_bytes());
    resp[4..8].copy_from_slice(&free_heap.to_le_bytes());
    resp[8..12].copy_from_slice(&min_free_heap.to_le_bytes());
    resp[12..16].copy_from_slice(&total_run_time.to_le_bytes());
    resp[16] = task_count as u8;
    // resp[17..20] = reserved (already zero)

    // Per-task entries
    for (i, entry) in task_buf.iter().enumerate().take(task_count) {
        let t = unsafe { entry.assume_init_ref() };
        let base = 20 + i * 28;

        // Task name (up to 16 bytes, null-padded)
        let name_ptr = t.task_name;
        let mut name_len = 0usize;
        while name_len < 16 {
            let b = unsafe { *name_ptr.add(name_len) };
            if b == 0 {
                break;
            }
            resp[base + name_len] = b;
            name_len += 1;
        }

        resp[base + 16] = t.task_state as u8;
        resp[base + 17] = t.current_priority as u8;
        resp[base + 18] = t.base_priority as u8;
        // resp[base + 19] = reserved
        resp[base + 20..base + 22].copy_from_slice(&(t.stack_high_water_mark as u16).to_le_bytes());
        resp[base + 22..base + 24].copy_from_slice(&(t.task_number as u16).to_le_bytes());

        // CPU % × 10 from delta with previous snapshot
        let cpu_pct_x10 = compute_cpu_pct(
            prev,
            t.task_number as u16,
            t.run_time_counter,
            total_run_time,
        );
        resp[base + 24..base + 28].copy_from_slice(&cpu_pct_x10.to_le_bytes());
    }

    CdcTransport::send_pdbp_response(STATUS_OK, &resp[..resp_len]);

    // Save current state as the previous snapshot for the next query
    let snap = unsafe { &mut *core::ptr::addr_of_mut!(PREV_SNAPSHOT) };
    snap.total_run_time = total_run_time;
    snap.task_count = task_count.min(MAX_TASKS) as u8;
    for (i, entry) in task_buf.iter().enumerate().take(snap.task_count as usize) {
        let t = unsafe { entry.assume_init_ref() };
        snap.tasks[i] = TaskRuntime {
            task_number: t.task_number as u16,
            run_time: t.run_time_counter,
        };
    }
    snap.valid = true;
}

/// Compute CPU % × 10 for a task by comparing current run-time against previous snapshot.
/// Returns 0xFFFFFFFF if no valid previous snapshot exists.
fn compute_cpu_pct(
    prev: &RuntimeSnapshot,
    task_number: u16,
    current_runtime: u32,
    current_total: u32,
) -> u32 {
    if !prev.valid {
        return 0xFFFF_FFFF;
    }

    let delta_total = current_total.wrapping_sub(prev.total_run_time);
    if delta_total == 0 {
        return 0;
    }

    // Find this task in the previous snapshot
    let prev_runtime = (0..prev.task_count as usize)
        .find(|&i| prev.tasks[i].task_number == task_number)
        .map(|i| prev.tasks[i].run_time)
        .unwrap_or(0);

    let delta_task = current_runtime.wrapping_sub(prev_runtime);

    // (delta_task * 1000) / delta_total, but avoid overflow by using u64
    ((delta_task as u64 * 1000) / delta_total as u64) as u32
}
