//! FreeRTOS task priority tiers.
//!
//! Layout (low → high, configMAX_PRIORITIES = 32):
//!   0          : FreeRTOS idle (reserved)
//!   1–10       : Background native services  (BG_1..BG_10)
//!   11–20      : JVM application tasks       (Android priority 1–10)
//!   21–30      : Real-time native tasks      (RT_1..RT_10)
//!   31         : FreeRTOS timer task         (configMAX_PRIORITIES - 1)

// All constants are public API; not every one will be used by the firmware itself.
#![allow(dead_code)]

pub const PRIORITY_BG_1: u8 = 1;
pub const PRIORITY_BG_2: u8 = 2;
pub const PRIORITY_BG_3: u8 = 3;
pub const PRIORITY_BG_4: u8 = 4;
pub const PRIORITY_BG_5: u8 = 5;
pub const PRIORITY_BG_6: u8 = 6;
pub const PRIORITY_BG_7: u8 = 7;
pub const PRIORITY_BG_8: u8 = 8;
pub const PRIORITY_BG_9: u8 = 9;
pub const PRIORITY_BG_10: u8 = 10;

pub const PRIORITY_JVM_MIN: u8 = 11; // Android MIN_PRIORITY  = 1
pub const PRIORITY_JVM_NORM: u8 = 15; // Android NORM_PRIORITY = 5
pub const PRIORITY_JVM_MAX: u8 = 20; // Android MAX_PRIORITY  = 10

pub const PRIORITY_RT_1: u8 = 21; // pdb task lives here
pub const PRIORITY_RT_2: u8 = 22;
pub const PRIORITY_RT_3: u8 = 23;
pub const PRIORITY_RT_4: u8 = 24;
pub const PRIORITY_RT_5: u8 = 25;
pub const PRIORITY_RT_6: u8 = 26;
pub const PRIORITY_RT_7: u8 = 27;
pub const PRIORITY_RT_8: u8 = 28;
pub const PRIORITY_RT_9: u8 = 29;
pub const PRIORITY_RT_10: u8 = 30;

/// Convert an Android-compatible thread priority (1–10) to a FreeRTOS priority.
/// Clamps out-of-range values to [`PRIORITY_JVM_MIN`]..[`PRIORITY_JVM_MAX`].
pub fn android_to_freertos_priority(android_priority: i32) -> u8 {
    let clamped = android_priority.clamp(1, 10) as u8;
    clamped + 10 // offset: Android 1 → FreeRTOS 11
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_android_to_freertos_priority() {
        assert_eq!(android_to_freertos_priority(1), PRIORITY_JVM_MIN); // 11
        assert_eq!(android_to_freertos_priority(5), PRIORITY_JVM_NORM); // 15
        assert_eq!(android_to_freertos_priority(10), PRIORITY_JVM_MAX); // 20
                                                                        // clamping
        assert_eq!(android_to_freertos_priority(0), PRIORITY_JVM_MIN); // clamps to 1 → 11
        assert_eq!(android_to_freertos_priority(11), PRIORITY_JVM_MAX); // clamps to 10 → 20
        assert_eq!(android_to_freertos_priority(-99), PRIORITY_JVM_MIN);
        assert_eq!(android_to_freertos_priority(99), PRIORITY_JVM_MAX);
    }
}
