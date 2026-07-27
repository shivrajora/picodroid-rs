// SPDX-License-Identifier: GPL-3.0-only
//! This family's half of the debug bridge: a byte pipe and a task-statistics
//! source. The protocol on top of them is `picodroid_core::pdb`.

use core::ffi::c_void;
use core::mem::MaybeUninit;

use freertos_rust::{freertos_rs_xTaskGetTickCount, FreeRtosUBaseType};
use picodroid_core::pdb::{PdbTransport, SysmonSample, SysmonSource, TaskSample, MAX_TASKS};

/// PDBP over this family's USB CDC endpoint.
///
/// Zero-sized: every byte lives in the static RX queue the ISR in
/// `hal::pdb_usb` owns.
pub struct CdcTransport;

impl PdbTransport for CdcTransport {
    fn init(&mut self) {
        crate::hal::pdb_usb::init()
    }

    fn read_byte(&mut self) -> u8 {
        crate::hal::pdb_usb::queue_read_byte()
    }

    fn read_byte_timeout(&mut self) -> Option<u8> {
        // Only reached while streaming an install. On RP2350
        // (`configTICK_CORE = 0`) every flash erase/program window disables
        // core 0 interrupts and freezes the FreeRTOS tick, so a tick-based
        // timeout would never fire — busy-wait on the hardware timer instead.
        //
        // The fork lives here rather than at the call site because it is an
        // answer to "how does a read time out on this chip", which is exactly
        // what the trait method is for.
        #[cfg(feature = "chip-rp2350")]
        {
            crate::hal::pdb_usb::queue_read_byte_busywait(2_000_000)
        }
        #[cfg(not(feature = "chip-rp2350"))]
        {
            crate::hal::pdb_usb::queue_read_byte_timeout()
        }
    }

    fn write_bytes(&mut self, data: &[u8]) {
        crate::hal::pdb_usb::write_bytes(data)
    }

    fn drain_tx(&mut self) {
        crate::hal::pdb_usb::drain_tx()
    }

    fn read_u32_le(&mut self) -> u32 {
        // The USB layer already assembles this from its queue, so take it
        // rather than paying four separate reads.
        crate::hal::pdb_usb::queue_read_u32_le()
    }
}

// ── Task statistics ──────────────────────────────────────────────────────────

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

/// `TaskStatus_t` as *this* build lays it out:
///   `configUSE_CORE_AFFINITY = 1`, `configNUMBER_OF_CORES = 2`,
///   `configSTACK_DEPTH_TYPE = StackType_t` (u32 on ARM),
///   `configRUN_TIME_COUNTER_TYPE = uint32_t`, `portSTACK_GROWTH = -1`,
///   `configRECORD_STACK_HIGH_ADDRESS` undefined.
///
/// Hand-written because the upstream `FreeRtosTaskStatusFfi` omits
/// `uxCoreAffinityMask` and narrows the high-water mark to u16, making it 36
/// bytes against the real 40. `uxTaskGetSystemState` writes the real size, so
/// the short version overflows the array and hard-faults.
///
/// This is why the sysmon *source* stays family-side while the encoder does
/// not: the struct is pinned to one `FreeRTOSConfig.h`, and no other family
/// can reuse a byte of it.
#[repr(C)]
struct TaskStatusC {
    handle: *const c_void,
    task_name: *const u8,
    task_number: u32,
    task_state: u32, // eTaskState — a C enum, 4 bytes on ARM
    current_priority: u32,
    base_priority: u32,
    run_time_counter: u32,
    stack_base: *const c_void,
    stack_high_water_mark: u32,
    core_affinity_mask: u32,
}

/// FreeRTOS task and heap statistics.
pub struct FreeRtosSysmon;

impl SysmonSource for FreeRtosSysmon {
    fn sample(&mut self, out: &mut SysmonSample) -> bool {
        let mut buf: [MaybeUninit<TaskStatusC>; MAX_TASKS] =
            [const { MaybeUninit::uninit() }; MAX_TASKS];
        let mut total_run_time: u32 = 0;

        // SAFETY: `buf` is MAX_TASKS entries of the layout above, and the
        // count passed is clamped to that.
        let count = unsafe {
            let n = uxTaskGetNumberOfTasks() as usize;
            uxTaskGetSystemState(
                buf.as_mut_ptr().cast(),
                n.min(MAX_TASKS) as FreeRtosUBaseType,
                &mut total_run_time,
            )
        } as usize;
        let count = count.min(MAX_TASKS);

        // SAFETY: plain FreeRTOS accessors.
        unsafe {
            out.uptime_ticks = freertos_rs_xTaskGetTickCount();
            out.free_heap = xPortGetFreeHeapSize();
            out.min_free_heap = xPortGetMinimumEverFreeHeapSize();
        }
        out.total_run_time = total_run_time;
        out.task_count = count as u8;

        for (slot, entry) in out.tasks.iter_mut().zip(buf.iter()).take(count) {
            // SAFETY: the first `count` entries were filled above.
            let t = unsafe { entry.assume_init_ref() };

            // The name is a C string of unknown length; copy up to the wire
            // field and stop at the NUL.
            let mut name = [0u8; 16];
            for (i, dst) in name.iter_mut().enumerate() {
                // SAFETY: FreeRTOS guarantees a NUL-terminated name.
                let b = unsafe { *t.task_name.add(i) };
                if b == 0 {
                    break;
                }
                *dst = b;
            }

            *slot = TaskSample {
                name,
                state: t.task_state as u8,
                current_priority: t.current_priority as u8,
                base_priority: t.base_priority as u8,
                stack_high_water: t.stack_high_water_mark as u16,
                task_number: t.task_number as u16,
                run_time: t.run_time_counter,
            };
        }
        true
    }
}
