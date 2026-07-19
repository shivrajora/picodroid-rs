// SPDX-License-Identifier: GPL-3.0-only
//! Shared boot memory budget: the FreeRTOS structures the device allocates
//! from its heap arena at boot, which the simulator must pre-charge to model
//! the real JVM budget (docs/parity-audit.md MEM-04/M4).
//!
//! Single source by construction: the device task-spawn sites
//! (`hal/rp/boot.rs`, `fs/worker.rs`, `system/native_handler/os.rs`,
//! `system/executors/background_pool.rs` via its generated config) take
//! their stack sizes from these constants, and the sim walks [`BOOT_TASKS`]
//! performing *real arena allocations* in boot order — so the long-lived
//! low-address blocks that first-fit placement depends on exist in the sim
//! arena just as they do on hardware.
//!
//! Calibration: V4 (parity-audit Appendix A) measured 89,472 B consumed on
//! an idle RP2350 testbench. Stacks below account for 67,072 B; TCBs and
//! boot-time queues make up the remainder. `TCB_EST_BYTES` and
//! `QUEUES_MISC_BYTES` are calibrated estimates — the HIL boot-budget
//! assertion (parity harness §5.1) fails the nightly if the model drifts
//! more than ~2 KB from the measured device figure.

/// Per-chip JVM interpreter task stack, in FreeRTOS words (×4 bytes).
/// Consumed by `hal/rp/boot.rs`.
#[cfg(feature = "chip-rp2350")]
pub const JVM_STACK_WORDS: u16 = 8192;
#[cfg(not(feature = "chip-rp2350"))]
pub const JVM_STACK_WORDS: u16 = 4096;

/// PDB (debug bridge) task stack. Consumed by `hal/rp/boot.rs`.
pub const PDB_STACK_WORDS: u16 = 2048;
/// cyw43 WiFi task stack (network boards only). Consumed by `hal/rp/boot.rs`.
#[allow(dead_code)] // only read on network_cyw43 boards
pub const CYW43_STACK_WORDS: u16 = 2048;
/// LittleFS worker task stack. Consumed by `fs/worker.rs`.
pub const FS_STACK_WORDS: u16 = 2048;
/// Per-`Thread.start` FreeRTOS task stack ("jvm-t"). Consumed by
/// `system/native_handler/os.rs`; charged per spawn, not at boot.
pub const JVM_THREAD_STACK_WORDS: u16 = 4096;
/// FreeRTOS idle/timer service stacks (`configMINIMAL_STACK_SIZE` /
/// `configTIMER_TASK_STACK_DEPTH` in FreeRTOSConfig.h).
pub const MINIMAL_STACK_WORDS: u16 = 128;

/// Estimated TCB_t allocation per task (SMP build with core affinity).
/// Calibrated, not measured field-by-field — see module doc.
pub const TCB_EST_BYTES: u32 = 120;
/// Boot-time queues and misc kernel structures (main queue, background-pool
/// queue, fs queue, timer command queue). Calibrated bucket — see module doc.
pub const QUEUES_MISC_BYTES: u32 = 2048;

/// One boot-time FreeRTOS task: a stack allocation plus a TCB.
pub struct BootTask {
    /// Read by the sim pre-charge diagnostics; device builds only consume
    /// the per-task constants directly.
    #[allow(dead_code)]
    pub name: &'static str,
    pub stack_words: u16,
}

/// Tasks the device creates on the way up, in creation order (user tasks
/// from `start_tasks`, then the scheduler's timer-service and idle tasks).
/// The background pool's worker count/stack come from its generated board
/// config; the values here must match `background_pool_config.rs`
/// (POOL_THREADS = 4, POOL_STACK_BYTES = 4096).
pub const BOOT_TASKS: &[BootTask] = &[
    BootTask {
        name: "pdb",
        stack_words: PDB_STACK_WORDS,
    },
    #[cfg(network_cyw43)]
    BootTask {
        name: "cyw43",
        stack_words: CYW43_STACK_WORDS,
    },
    BootTask {
        name: "fs",
        stack_words: FS_STACK_WORDS,
    },
    BootTask {
        name: "jvm",
        stack_words: JVM_STACK_WORDS,
    },
    BootTask {
        name: "Tmr Svc",
        stack_words: MINIMAL_STACK_WORDS,
    },
    BootTask {
        name: "IDLE0",
        stack_words: MINIMAL_STACK_WORDS,
    },
    BootTask {
        name: "IDLE1",
        stack_words: MINIMAL_STACK_WORDS,
    },
    BootTask {
        name: "jvm-bg",
        stack_words: 1024,
    },
    BootTask {
        name: "jvm-bg",
        stack_words: 1024,
    },
    BootTask {
        name: "jvm-bg",
        stack_words: 1024,
    },
    BootTask {
        name: "jvm-bg",
        stack_words: 1024,
    },
];

/// Total modeled boot overhead in bytes (stacks + TCBs + queue bucket).
/// Consumed by the sim pre-charge banner and the HIL boot-budget assertion.
#[cfg_attr(not(feature = "sim"), allow(dead_code))]
pub fn modeled_boot_bytes() -> u32 {
    let stacks: u32 = BOOT_TASKS
        .iter()
        .map(|t| t.stack_words as u32 * 4 + TCB_EST_BYTES)
        .sum();
    stacks + QUEUES_MISC_BYTES
}

/// Sim only: perform the boot-budget allocations for real, in boot order,
/// against the heap_4 arena — leaked for process lifetime exactly as the
/// device's task stacks are. Called from sim `main` right after arming the
/// allocator (the device allocates these in `start_tasks`).
#[cfg(feature = "sim")]
pub fn precharge_boot_budget() {
    fn charge(bytes: u32, what: &str) {
        let layout = std::alloc::Layout::from_size_align(bytes as usize, 8).unwrap();
        let p = unsafe { std::alloc::alloc(layout) };
        assert!(
            !p.is_null(),
            "boot-budget pre-charge failed for {what}: the arena cannot fit \
             the device's boot allocations — heap cap too small"
        );
        // Leaked deliberately: device task stacks live for the process.
        // black_box makes the pointer escape — without it LLVM elides the
        // whole never-freed allocation in optimized builds and the arena is
        // never charged.
        std::hint::black_box(p);
    }
    for t in BOOT_TASKS {
        charge(t.stack_words as u32 * 4, t.name);
        charge(TCB_EST_BYTES, t.name);
    }
    charge(QUEUES_MISC_BYTES, "boot queues");
    println!(
        "[sim] heap: boot budget pre-charged {} B ({} tasks + queues; device model)",
        modeled_boot_bytes(),
        BOOT_TASKS.len()
    );
}

/// Sim only: charge one `Thread.start` the way the device does (task stack +
/// TCB from the arena). Leaked: without M7 the sim never runs or exits the
/// thread, and on device the stack lives until the Runnable returns.
#[cfg(feature = "sim")]
pub fn charge_thread_spawn() {
    let bytes = JVM_THREAD_STACK_WORDS as u32 * 4 + TCB_EST_BYTES;
    let layout = std::alloc::Layout::from_size_align(bytes as usize, 8).unwrap();
    let p = unsafe { std::alloc::alloc(layout) };
    if p.is_null() {
        eprintln!("[sim] Thread.start: arena could not fit the 16 KB device task stack");
    }
    std::hint::black_box(p); // see charge(): prevents elision of the leak
}
