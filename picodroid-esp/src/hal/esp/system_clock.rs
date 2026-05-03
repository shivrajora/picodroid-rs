// SPDX-License-Identifier: GPL-3.0-only
// ESP32-S3 system clock stubs — Milestone 1.
// Real delay via esp-hal and elapsed time via RTC are Milestone 2.

pub fn sleep(_ms: u32) {}

pub fn elapsed_realtime_nanos() -> i64 {
    0
}
