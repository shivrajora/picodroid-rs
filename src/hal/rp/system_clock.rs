pub fn sleep(ms: u32) {
    freertos_rust::CurrentTask::delay(freertos_rust::Duration::ms(ms));
}

pub fn elapsed_realtime_nanos() -> i64 {
    0
}
