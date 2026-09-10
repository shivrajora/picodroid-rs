// SPDX-License-Identifier: GPL-3.0-only
/// Block the calling task for `ms` — through the *kernel* when the caller is
/// a FreeRTOS task, matching the device's `vTaskDelay`.
///
/// This must not be a bare `std::thread::sleep` on a task: the kernel would
/// still consider the sleeping task Running, and with
/// `configUSE_TIME_SLICING=0` an equal-priority sibling then never gets a
/// yield point to run at — the second Java thread of two starves without
/// executing one instruction (threaddemo, nightly 2026-08-18). Non-task host
/// threads (control channel, pre-scheduler boot) keep the std sleep; kernel
/// delay from a non-task pthread would act on the kernel's notion of the
/// current task instead.
pub fn sleep(ms: u32) {
    if super::rtos::current_thread_is_task() {
        super::rtos::delay_ms(ms);
    } else {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
    }
}

pub fn elapsed_realtime_nanos() -> i64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    epoch.elapsed().as_nanos() as i64
}
