// SPDX-License-Identifier: GPL-3.0-only
//! This family's boot memory model: the FreeRTOS structures the device
//! allocates from its heap arena at boot, as data.
//!
//! Single source by construction: the device task-spawn sites
//! (`boot_tasks.rs`, and every `Rtos::spawn` through [`default_stack_bytes`])
//! take their stack sizes from the constants here, and the simulator charges
//! its arena from [`MODEL`], which is built from the same constants — so the
//! two cannot disagree (docs/parity-audit.md MEM-04/M4). The charging itself
//! is `picodroid_core::hal::sim::boot_budget`, shared by every family
//! (docs/designs/porting-seam-2026-09.md E6); what is here is only what is
//! ours.
//!
//! Calibration: V4 (parity-audit Appendix A) measured 89,472 B consumed on
//! an idle RP2350 testbench. Stacks below account for 67,072 B; TCBs and
//! boot-time queues make up the remainder. `TCB_EST_BYTES` and
//! `QUEUES_MISC_BYTES` are calibrated estimates — the HIL boot-budget
//! assertion (parity harness §5.1) fails the nightly if the model drifts
//! more than ~2 KB from the measured device figure.

#[cfg(any(test, feature = "sim"))]
use picodroid_core::hal::sim::boot_budget::{BootBudgetModel, BootTask};

/// Per-chip JVM interpreter task stack, in FreeRTOS words (×4 bytes).
/// Consumed by `boot_tasks.rs`.
#[cfg(feature = "chip-rp2350")]
pub const JVM_STACK_WORDS: u16 = 8192;
#[cfg(not(feature = "chip-rp2350"))]
pub const JVM_STACK_WORDS: u16 = 4096;

/// PDB (debug bridge) task stack. Consumed by `boot_tasks.rs`.
pub const PDB_STACK_WORDS: u16 = 2048;
/// cyw43 WiFi task stack (network boards only). Consumed by `boot_tasks.rs`.
#[allow(dead_code)] // only read on network_cyw43 boards
pub const CYW43_STACK_WORDS: u16 = 2048;
/// LittleFS worker task stack. Consumed through [`default_stack_bytes`].
pub const FS_STACK_WORDS: u16 = 2048;
/// Core-1 flash parker task stack. Consumed by `boot_tasks.rs`.
/// The task only blocks on a notification and spins in a `.data` loop
/// (`hal/rp/core1_park.rs`); 256 words covers the freertos-rust trampoline
/// with a wide margin.
pub const FLASHPARK_STACK_WORDS: u16 = 256;
/// Sensor sampler task stack (sensor boards only). Consumed through
/// [`default_stack_bytes`]. Deepest chain is a driver call → I²C transfer +
/// fixed-point compensation + defmt frame; 1024 words leaves multiples of
/// headroom (verify via the one-shot stack-HWM debug log; bump to 1536 if
/// headroom drops below ~25%).
pub const SENSOR_STACK_WORDS: u16 = 1024;
/// Per-`Thread.start` FreeRTOS task stack ("jvm-t"). Consumed through
/// [`default_stack_bytes`]; charged per spawn, not at boot.
pub const JVM_THREAD_STACK_WORDS: u16 = 4096;
/// FreeRTOS idle/timer service stacks (`configMINIMAL_STACK_SIZE` /
/// `configTIMER_TASK_STACK_DEPTH` in FreeRTOSConfig.h).
#[cfg_attr(not(any(test, feature = "sim")), allow(dead_code))]
pub const MINIMAL_STACK_WORDS: u16 = 128;

/// Estimated TCB_t allocation per task (SMP build with core affinity).
/// Calibrated, not measured field-by-field — see module doc.
#[cfg_attr(not(any(test, feature = "sim")), allow(dead_code))]
pub const TCB_EST_BYTES: u32 = 120;
/// Boot-time queues and misc kernel structures (main queue, background-pool
/// queue, fs queue, timer command queue). Calibrated bucket — see module doc.
#[cfg_attr(not(any(test, feature = "sim")), allow(dead_code))]
pub const QUEUES_MISC_BYTES: u32 = 2048;

/// The stack a task of `kind` gets when the caller does not name one, in
/// **bytes**.
///
/// Both `Rtos` backings resolve `TaskSpec::stack_bytes: None` through here —
/// the device's directly, the simulator's through [`MODEL`] — so device and
/// simulator size the same task the same way and the arena charge can never
/// drift from the real allocation (docs/parity-audit.md M4). The seam speaks
/// bytes and FreeRTOS counts words; [`bytes`] and the ÷4 at each device spawn
/// site are the entirety of that conversion.
pub fn default_stack_bytes(kind: picodroid_core::rtos::TaskKind) -> u32 {
    use picodroid_core::rtos::TaskKind;
    match kind {
        TaskKind::Jvm => bytes(JVM_STACK_WORDS),
        TaskKind::JvmChild => bytes(JVM_THREAD_STACK_WORDS),
        TaskKind::BgWorker => picodroid_core::board_cfg::background_pool::POOL_STACK_BYTES,
        TaskKind::Sensor => bytes(SENSOR_STACK_WORDS),
        TaskKind::FsWorker => bytes(FS_STACK_WORDS),
    }
}

/// FreeRTOS counts stacks in words; the model, like the seam, speaks bytes.
const fn bytes(words: u16) -> u32 {
    words as u32 * 4
}

/// Tasks the device creates on the way up, in creation order (user tasks
/// from `start_tasks`, then the scheduler's timer-service and idle tasks),
/// plus the TCB and queue estimates — this family's memory model, as the
/// shared simulator engine consumes it.
///
/// The background pool's worker count and stack come from its generated
/// board config; the four entries here must match `POOL_THREADS = 4`, and
/// their size is read from the same constant the pool spawns with.
#[cfg(any(test, feature = "sim"))]
pub static MODEL: BootBudgetModel = BootBudgetModel {
    tasks: &[
        BootTask {
            name: "flashpark",
            stack_bytes: bytes(FLASHPARK_STACK_WORDS),
            // No simulator counterpart: host flash has no XIP window to park for.
            sim_real: false,
        },
        BootTask {
            name: "pdb",
            stack_bytes: bytes(PDB_STACK_WORDS),
            sim_real: false, // no simulator debug bridge
        },
        #[cfg(network_cyw43)]
        BootTask {
            name: "cyw43",
            stack_bytes: bytes(CYW43_STACK_WORDS),
            sim_real: false, // no simulator WiFi endpoint
        },
        BootTask {
            name: "fs",
            stack_bytes: bytes(FS_STACK_WORDS),
            sim_real: true, // sim_boot spawns the fs worker
        },
        #[cfg(any_sensor)]
        BootTask {
            name: "sensor",
            stack_bytes: bytes(SENSOR_STACK_WORDS),
            // Modeled rather than created. The device sampler exists to
            // drive real I²C parts; there are none on the host, so the
            // simulator keeps its own backing (`sampler.rs`'s `sim_backing`),
            // which fabricates plausible snapshots on a host thread and
            // publishes them through the seqlock mailbox — atomics only, so
            // it is one of the host-service threads §1.2 leaves outside the
            // kernel.
            sim_real: false,
        },
        BootTask {
            name: "jvm",
            stack_bytes: bytes(JVM_STACK_WORDS),
            sim_real: true, // sim_boot spawns it
        },
        BootTask {
            name: "Tmr Svc",
            stack_bytes: bytes(MINIMAL_STACK_WORDS),
            sim_real: false, // created by the kernel, allocated off-arena
        },
        BootTask {
            name: "IDLE0",
            stack_bytes: bytes(MINIMAL_STACK_WORDS),
            sim_real: false, // as above
        },
        BootTask {
            name: "IDLE1",
            stack_bytes: bytes(MINIMAL_STACK_WORDS),
            // As above, and doubly so: the POSIX port is single-core and
            // creates only one idle task. The device's second one is charged
            // anyway, because the arena models the *device*.
            sim_real: false,
        },
        BootTask {
            name: "jvm-bg",
            stack_bytes: picodroid_core::board_cfg::background_pool::POOL_STACK_BYTES,
            sim_real: true, // background_pool::spawn goes through the Rtos seam
        },
        BootTask {
            name: "jvm-bg",
            stack_bytes: picodroid_core::board_cfg::background_pool::POOL_STACK_BYTES,
            sim_real: true,
        },
        BootTask {
            name: "jvm-bg",
            stack_bytes: picodroid_core::board_cfg::background_pool::POOL_STACK_BYTES,
            sim_real: true,
        },
        BootTask {
            name: "jvm-bg",
            stack_bytes: picodroid_core::board_cfg::background_pool::POOL_STACK_BYTES,
            sim_real: true,
        },
    ],
    tcb_bytes: TCB_EST_BYTES,
    queues_misc_bytes: QUEUES_MISC_BYTES,
    default_stack_bytes,
};
