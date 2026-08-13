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
//! Most of those bytes now arrive by a shorter route. The simulator runs the
//! real kernel, so the tasks that genuinely exist there
//! ([`BootTask::sim_real`]) are charged at the moment they are created, as the
//! device charges them, and released when they finish. Only the tasks the
//! simulator has no endpoint for stay synthetic pre-charges.
//! [`report_boot_budget`] asserts the two routes still sum to the same figure.
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
/// Core-1 flash parker task stack. Consumed by `boot_tasks.rs`.
/// The task only blocks on a notification and spins in a `.data` loop
/// (`hal/rp/core1_park.rs`); 256 words covers the freertos-rust trampoline
/// with a wide margin.
pub const FLASHPARK_STACK_WORDS: u16 = 256;
/// Sensor sampler task stack (sensor boards only). Consumed by
/// `system/picodroid/hardware/sensors/sampler.rs`. Deepest chain is a
/// driver call → I²C transfer + fixed-point compensation + defmt frame;
/// 1024 words leaves multiples of headroom (verify via the one-shot
/// stack-HWM debug log; bump to 1536 if headroom drops below ~25%).
#[allow(dead_code)] // only read on any_sensor boards
pub const SENSOR_STACK_WORDS: u16 = 1024;
/// Per-`Thread.start` FreeRTOS task stack ("jvm-t"). Consumed by
/// `system/native_handler/os.rs`; charged per spawn, not at boot.
pub const JVM_THREAD_STACK_WORDS: u16 = 4096;
/// FreeRTOS idle/timer service stacks (`configMINIMAL_STACK_SIZE` /
/// `configTIMER_TASK_STACK_DEPTH` in FreeRTOSConfig.h).
#[cfg_attr(not(feature = "sim"), allow(dead_code))]
pub const MINIMAL_STACK_WORDS: u16 = 128;

/// Estimated TCB_t allocation per task (SMP build with core affinity).
/// Calibrated, not measured field-by-field — see module doc.
#[cfg_attr(not(feature = "sim"), allow(dead_code))]
pub const TCB_EST_BYTES: u32 = 120;
/// Boot-time queues and misc kernel structures (main queue, background-pool
/// queue, fs queue, timer command queue). Calibrated bucket — see module doc.
#[cfg_attr(not(feature = "sim"), allow(dead_code))]
pub const QUEUES_MISC_BYTES: u32 = 2048;

/// One boot-time FreeRTOS task: a stack allocation plus a TCB.
#[cfg_attr(not(feature = "sim"), allow(dead_code))]
pub struct BootTask {
    /// Read by the sim pre-charge diagnostics; device builds only consume
    /// the per-task constants directly.
    pub name: &'static str,
    pub stack_words: u16,
    /// True when the simulator creates this task *for real* rather than
    /// modelling it. Those tasks charge the arena at creation, the way the
    /// device does; the rest stay synthetic pre-charges so the total keeps
    /// matching the measured device figure.
    ///
    /// False for the two tasks with no simulator endpoint (`pdb`, `cyw43`) and
    /// for the two the kernel creates itself (`Tmr Svc`, `IDLE*`) — the
    /// latter's host allocations ride the `pvPortMalloc` bypass shim, so the
    /// arena would otherwise never hear about them.
    pub sim_real: bool,
}

/// Tasks the device creates on the way up, in creation order (user tasks
/// from `start_tasks`, then the scheduler's timer-service and idle tasks).
/// The background pool's worker count/stack come from its generated board
/// config; the values here must match `background_pool_config.rs`
/// (POOL_THREADS = 4, POOL_STACK_BYTES = 4096).
#[cfg_attr(not(feature = "sim"), allow(dead_code))]
pub const BOOT_TASKS: &[BootTask] = &[
    BootTask {
        name: "flashpark",
        stack_words: FLASHPARK_STACK_WORDS,
        // No simulator counterpart: host flash has no XIP window to park for.
        sim_real: false,
    },
    BootTask {
        name: "pdb",
        stack_words: PDB_STACK_WORDS,
        sim_real: false, // no simulator debug bridge
    },
    #[cfg(network_cyw43)]
    BootTask {
        name: "cyw43",
        stack_words: CYW43_STACK_WORDS,
        sim_real: false, // no simulator WiFi endpoint
    },
    BootTask {
        name: "fs",
        stack_words: FS_STACK_WORDS,
        sim_real: true, // sim_boot spawns the fs worker
    },
    #[cfg(any_sensor)]
    BootTask {
        name: "sensor",
        stack_words: SENSOR_STACK_WORDS,
        // Modeled rather than created. The device sampler exists to
        // drive real I²C parts; there are none on the host, so the simulator
        // keeps its own backing (`sampler.rs`'s `sim_backing`), which
        // fabricates plausible snapshots on a host thread and publishes them
        // through the seqlock mailbox — atomics only, so it is one of the
        // host-service threads §1.2 leaves outside the kernel.
        sim_real: false,
    },
    BootTask {
        name: "jvm",
        stack_words: JVM_STACK_WORDS,
        sim_real: true, // sim_boot spawns it
    },
    BootTask {
        name: "Tmr Svc",
        stack_words: MINIMAL_STACK_WORDS,
        sim_real: false, // created by the kernel, allocated off-arena
    },
    BootTask {
        name: "IDLE0",
        stack_words: MINIMAL_STACK_WORDS,
        sim_real: false, // as above
    },
    BootTask {
        name: "IDLE1",
        stack_words: MINIMAL_STACK_WORDS,
        // As above, and doubly so: the POSIX port is single-core and creates
        // only one idle task. The device's second one is charged anyway,
        // because the arena models the *device*.
        sim_real: false,
    },
    BootTask {
        name: "jvm-bg",
        stack_words: 1024,
        sim_real: true, // background_pool::spawn goes through the Rtos seam
    },
    BootTask {
        name: "jvm-bg",
        stack_words: 1024,
        sim_real: true,
    },
    BootTask {
        name: "jvm-bg",
        stack_words: 1024,
        sim_real: true,
    },
    BootTask {
        name: "jvm-bg",
        stack_words: 1024,
        sim_real: true,
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

/// The stack a task of `kind` gets when the caller does not name one, in
/// **bytes**.
///
/// Both `Rtos` backings resolve `TaskSpec::stack_bytes: None` through here, so
/// device and simulator size the same task the same way and the arena charge
/// can never drift from the real allocation (docs/parity-audit.md M4). The
/// seam speaks bytes and FreeRTOS counts words; the ×4 here and the ÷4 at each
/// spawn site are the entirety of that conversion.
pub fn default_stack_bytes(kind: picodroid_core::rtos::TaskKind) -> u32 {
    use picodroid_core::rtos::TaskKind;
    match kind {
        TaskKind::Jvm => JVM_STACK_WORDS as u32 * 4,
        TaskKind::JvmChild => JVM_THREAD_STACK_WORDS as u32 * 4,
        TaskKind::BgWorker => picodroid_core::board_cfg::background_pool::POOL_STACK_BYTES,
        TaskKind::Sensor => SENSOR_STACK_WORDS as u32 * 4,
        TaskKind::FsWorker => FS_STACK_WORDS as u32 * 4,
    }
}

/// Sim only: the arena side of the boot budget — charges, and (in the
/// the matching releases.
///
/// A charge is a real `heap_4` allocation of the bytes the device would have
/// spent, so the long-lived low-address blocks first-fit placement depends on
/// exist in the sim arena as they do on hardware. What is new here is that
/// some of them are *releasable*: a Java thread really runs and really exits, and the device reclaims its stack and TCB when it
/// does (`vTaskDelete(NULL)` in the spawn trampoline). Modelling the spawn but
/// not the exit would make a thread-churning app look like a leak that
/// hardware does not have.
#[cfg(feature = "sim")]
mod model {
    use std::sync::Mutex;

    use picodroid_core::hal::sim::allocator;

    /// Live charges, as `(bytes, ptr)`. Every entry of a given size is
    /// interchangeable — they were allocated identically — so a release just
    /// takes the most recent match, which is also the one first-fit is most
    /// likely to want back.
    ///
    /// The `Vec` is host bookkeeping with no device counterpart, so its own
    /// growth is bypassed; only the charged blocks reach the arena.
    static CHARGES: Mutex<Vec<(u32, usize)>> = Mutex::new(Vec::new());

    /// Running total of every charge made, released or not. Only read by
    /// [`super::report_boot_budget`], which uses it to prove that "pre-charge
    /// some, create the rest for real" adds up to the same number the
    /// all-synthetic lane charges — the thing the ±2 KB HIL assertion is
    /// calibrated against.
    static TOTAL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    pub fn total_charged() -> u32 {
        TOTAL.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn lock() -> std::sync::MutexGuard<'static, Vec<(u32, usize)>> {
        CHARGES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn layout(bytes: u32) -> std::alloc::Layout {
        std::alloc::Layout::from_size_align(bytes as usize, 8).expect("boot-budget layout")
    }

    /// Charge `bytes` to the arena. `track` records the block so
    /// [`release`] can give it back; untracked charges are the permanent
    /// ones (boot stacks the device never frees).
    pub fn charge(bytes: u32, what: &str, track: bool) -> *mut u8 {
        // Deliberately not bypassed: this allocation *is* the device model.
        let p = unsafe { std::alloc::alloc(layout(bytes)) };
        if p.is_null() {
            eprintln!("[sim] boot budget: arena could not fit {bytes} B for {what}");
            return p;
        }
        TOTAL.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
        if track {
            let _bypass = allocator::bypass();
            lock().push((bytes, p as usize));
        }
        // black_box makes the pointer escape — without it LLVM elides the
        // whole never-freed allocation in optimized builds and the arena is
        // never charged.
        std::hint::black_box(p)
    }

    /// Give back one block of exactly `bytes`, if one is outstanding.
    pub fn release(bytes: u32) {
        let taken = {
            let _bypass = allocator::bypass();
            let mut charges = lock();
            charges
                .iter()
                .rposition(|&(b, _)| b == bytes)
                .map(|i| charges.swap_remove(i).1)
        };
        if let Some(p) = taken {
            // Not bypassed either: `dealloc` routes by pointer range, and this
            // block came from the arena.
            unsafe { std::alloc::dealloc(p as *mut u8, layout(bytes)) };
        }
    }
}

/// Sim only: perform the boot-budget allocations for real, in boot order,
/// against the heap_4 arena — leaked for process lifetime exactly as the
/// device's task stacks are. Called from sim `main` right after arming the
/// allocator (the device allocates these in `start_tasks`).
///
/// The tasks marked [`BootTask::sim_real`] are left out: they get created for
/// real a moment later and charge themselves then, which is both closer to the
/// device and the only way their eventual exit can be modelled. Everything
/// else — the tasks with no simulator endpoint, the ones the kernel creates
/// itself, and the queue bucket — is pre-charged here, so the arena total is
/// the same figure either way.
#[cfg(feature = "sim")]
pub fn precharge_boot_budget() {
    let mut charged = 0u32;
    let mut synthetic = 0usize;
    for t in BOOT_TASKS {
        if t.sim_real {
            continue;
        }
        model::charge(t.stack_words as u32 * 4, t.name, false);
        model::charge(TCB_EST_BYTES, t.name, false);
        charged += t.stack_words as u32 * 4 + TCB_EST_BYTES;
        synthetic += 1;
    }
    model::charge(QUEUES_MISC_BYTES, "boot queues", false);
    charged += QUEUES_MISC_BYTES;

    println!(
        "[sim] heap: boot budget {} B pre-charged ({} modeled tasks + queues), \
         {} B to follow from {} real tasks — {} B total (device model)",
        charged,
        synthetic,
        modeled_boot_bytes() - charged,
        BOOT_TASKS.len() - synthetic,
        modeled_boot_bytes(),
    );
}

/// Check that boot charged the device's figure, and say so.
///
/// Called once, after the boot tasks exist and before the app runs. The
/// all-synthetic lane cannot get this wrong — it charges [`BOOT_TASKS`]
/// straight from the table — but most of it is now charged from live spawn
/// sites, which is exactly where a drift would appear: a task the
/// device creates that the simulator quietly does not, or one sized from a
/// different constant. Catching that here, rather than at the ±2 KB HIL
/// assertion a nightly later, is the point.
#[cfg(feature = "sim")]
pub fn report_boot_budget() {
    let charged = model::total_charged();
    let modeled = modeled_boot_bytes();
    println!(
        "[sim] heap: boot budget charged {charged} B of {modeled} B modeled \
         ({} tasks + queues)",
        BOOT_TASKS.len()
    );
    assert_eq!(
        charged, modeled,
        "boot budget drift: the simulator charged {charged} B but the device \
         model is {modeled} B. A BOOT_TASKS entry marked `sim_real` was not \
         created, or was created with a different stack size than the table \
         says (docs/parity-audit.md M4)."
    );
}

/// Sim only: charge one task spawn the way the device does — stack plus TCB
/// from the arena — and report the stack size in bytes.
///
/// The return value is what the FreeRTOS backing creates the real task from,
/// so the number charged and the number allocated are the same number by
/// construction (see `register_sim_platform!`). Under the `cargo test`
/// backing only a refused `Thread.start` reaches here and the value is
/// discarded.
#[cfg(feature = "sim")]
pub fn charge_task_spawn(spec: &picodroid_core::rtos::TaskSpec) -> u32 {
    let stack_bytes = spec
        .stack_bytes
        .unwrap_or_else(|| default_stack_bytes(spec.kind));
    // Tracked: the task has a real exit for `release_task_spawn` to run at.
    model::charge(stack_bytes + TCB_EST_BYTES, spec.name, true);
    stack_bytes
}

/// Sim only: release the charge [`charge_task_spawn`] made, when the task's
/// body returns. Pairs `boot_budget`'s long-standing deliberate leak with the
/// reclaim the device performs in `vTaskDelete(NULL)`.
#[cfg(feature = "sim")]
pub fn release_task_spawn(spec: &picodroid_core::rtos::TaskSpec) {
    let stack_bytes = spec
        .stack_bytes
        .unwrap_or_else(|| default_stack_bytes(spec.kind));
    model::release(stack_bytes + TCB_EST_BYTES);
}
