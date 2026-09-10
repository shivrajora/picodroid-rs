// SPDX-License-Identifier: GPL-3.0-only
//! The simulator's boot memory budget — the engine, shared by every family.
//!
//! A device allocates its boot-time FreeRTOS structures (task stacks, TCBs,
//! queues) from the same heap arena the JVM lives in. The simulator models
//! that by charging the arena the same bytes, in boot order, so the long-lived
//! low-address blocks first-fit placement depends on exist here as they do on
//! hardware (docs/parity-audit.md MEM-04/M4).
//!
//! *What* is charged is family data — which tasks exist, how big their stacks
//! are, what a TCB costs — and arrives as a [`BootBudgetModel`] through
//! `register_sim_platform!`. *How* it is charged — the ledger, the
//! charge/release pairing, the `black_box` that keeps LLVM from eliding a
//! never-freed allocation, the reconciliation assert — is the same for every
//! family and lives here (docs/designs/porting-seam-2026-09.md E6).
//!
//! Most of the bytes arrive by the short route. The simulator runs the real
//! kernel, so a task that genuinely exists there ([`BootTask::sim_real`]) is
//! charged at the moment it is created, as the device charges it, and
//! released when it finishes. Only the tasks the simulator has no endpoint for
//! stay synthetic pre-charges ([`precharge`]). [`report`] asserts the two
//! routes still sum to the same figure.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::allocator;
use crate::rtos::{TaskKind, TaskSpec};

/// One boot-time task in a family's memory model: a stack plus a TCB.
pub struct BootTask {
    pub name: &'static str,
    /// Stack size in **bytes** — the seam's unit. A family whose kernel counts
    /// words converts once, when it builds its table.
    pub stack_bytes: u32,
    /// True when the simulator creates this task *for real* rather than
    /// modelling it. Those tasks charge the arena at creation, the way the
    /// device does; the rest stay synthetic pre-charges so the total keeps
    /// matching the measured device figure.
    ///
    /// False for tasks with no simulator endpoint (a debug bridge, a WiFi
    /// driver) and for the ones the kernel creates itself (`Tmr Svc`,
    /// `IDLE*`) — the latter's host allocations ride the `pvPortMalloc`
    /// bypass shim, so the arena would otherwise never hear about them.
    pub sim_real: bool,
}

/// A family's boot memory model — chip-gated data the engine charges.
pub struct BootBudgetModel {
    /// Tasks the device creates on the way up, in creation order.
    pub tasks: &'static [BootTask],
    /// Estimated TCB allocation per task.
    pub tcb_bytes: u32,
    /// Boot-time queues and misc kernel structures, as one calibrated bucket.
    pub queues_misc_bytes: u32,
    /// The stack a task of a kind gets when the caller does not name one.
    /// The same function the family's device `Rtos::spawn` resolves through,
    /// so the charge and the real allocation are one number by construction.
    pub default_stack_bytes: fn(TaskKind) -> u32,
}

impl BootBudgetModel {
    /// Total modelled boot overhead in bytes (stacks + TCBs + queue bucket).
    pub fn modeled_boot_bytes(&self) -> u32 {
        let stacks: u32 = self
            .tasks
            .iter()
            .map(|t| t.stack_bytes + self.tcb_bytes)
            .sum();
        stacks + self.queues_misc_bytes
    }

    /// The stack `spec` gets, in bytes: what it asked for, or the family's
    /// default for its kind.
    pub fn stack_bytes(&self, spec: &TaskSpec) -> u32 {
        spec.stack_bytes
            .unwrap_or_else(|| (self.default_stack_bytes)(spec.kind))
    }
}

/// The arena side of the boot budget — charges, and the matching releases.
///
/// A charge is a real `heap_4` allocation of the bytes the device would have
/// spent, so the long-lived low-address blocks first-fit placement depends on
/// exist in the arena as they do on hardware. Some of them are *releasable*:
/// a Java thread really runs and really exits, and the device reclaims its
/// stack and TCB when it does (`vTaskDelete(NULL)` in the spawn trampoline).
/// Modelling the spawn but not the exit would make a thread-churning app look
/// like a leak that hardware does not have.
mod model {
    use super::*;

    /// Live charges, as `(bytes, ptr)`. Every entry of a given size is
    /// interchangeable — they were allocated identically — so a release just
    /// takes the most recent match, which is also the one first-fit is most
    /// likely to want back.
    ///
    /// The `Vec` is host bookkeeping with no device counterpart, so its own
    /// growth is bypassed; only the charged blocks reach the arena.
    static CHARGES: Mutex<Vec<(u32, usize)>> = Mutex::new(Vec::new());

    /// Running total of every charge made, released or not. Read only by
    /// [`super::report`], which uses it to prove that "pre-charge some,
    /// create the rest for real" adds up to the same number an all-synthetic
    /// lane would charge — the thing the ±2 KB HIL assertion is calibrated
    /// against.
    static TOTAL: AtomicU32 = AtomicU32::new(0);

    pub fn total_charged() -> u32 {
        TOTAL.load(Ordering::Relaxed)
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
        TOTAL.fetch_add(bytes, Ordering::Relaxed);
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

    #[cfg(test)]
    pub fn outstanding() -> usize {
        lock().len()
    }
}

/// Perform the boot-budget allocations for real, in boot order, against the
/// heap_4 arena — leaked for process lifetime exactly as the device's task
/// stacks are. Called right after arming the allocator (the device allocates
/// these in its `start_tasks`).
///
/// The tasks marked [`BootTask::sim_real`] are left out: they get created for
/// real a moment later and charge themselves then, which is both closer to the
/// device and the only way their eventual exit can be modelled. Everything
/// else — the tasks with no simulator endpoint, the ones the kernel creates
/// itself, and the queue bucket — is pre-charged here, so the arena total is
/// the same figure either way.
pub fn precharge(model: &BootBudgetModel) {
    let mut charged = 0u32;
    let mut synthetic = 0usize;
    for t in model.tasks {
        if t.sim_real {
            continue;
        }
        model::charge(t.stack_bytes, t.name, false);
        model::charge(model.tcb_bytes, t.name, false);
        charged += t.stack_bytes + model.tcb_bytes;
        synthetic += 1;
    }
    model::charge(model.queues_misc_bytes, "boot queues", false);
    charged += model.queues_misc_bytes;

    println!(
        "[sim] heap: boot budget {} B pre-charged ({} modeled tasks + queues), \
         {} B to follow from {} real tasks — {} B total (device model)",
        charged,
        synthetic,
        model.modeled_boot_bytes() - charged,
        model.tasks.len() - synthetic,
        model.modeled_boot_bytes(),
    );
}

/// Check that boot charged the device's figure, and say so.
///
/// Called once, after the boot tasks exist and before the app runs. An
/// all-synthetic lane could not get this wrong — it would charge the table
/// straight — but most of the total is now charged from live spawn sites,
/// which is exactly where a drift would appear: a task the device creates
/// that the simulator quietly does not, or one sized from a different
/// constant. Catching that here, rather than at the ±2 KB HIL assertion a
/// nightly later, is the point.
pub fn report(model: &BootBudgetModel) {
    let charged = model::total_charged();
    let modeled = model.modeled_boot_bytes();
    println!(
        "[sim] heap: boot budget charged {charged} B of {modeled} B modeled \
         ({} tasks + queues)",
        model.tasks.len()
    );
    assert_eq!(
        charged, modeled,
        "boot budget drift: the simulator charged {charged} B but the device \
         model is {modeled} B. A `sim_real` task in the family's boot-budget \
         model was not created, or was created with a different stack size \
         than the model says (docs/parity-audit.md M4)."
    );
}

/// Charge one task spawn the way the device does — stack plus TCB from the
/// arena — and report the stack size in bytes.
///
/// The return value is what the kernel backing creates the real task from,
/// so the number charged and the number allocated are the same number by
/// construction (see `register_sim_platform!`).
pub fn charge_task_spawn(model: &BootBudgetModel, spec: &TaskSpec) -> u32 {
    let stack_bytes = model.stack_bytes(spec);
    // Tracked: the task has a real exit for `release_task_spawn` to run at.
    model::charge(stack_bytes + model.tcb_bytes, spec.name, true);
    stack_bytes
}

/// Release the charge [`charge_task_spawn`] made, when the task's body
/// returns. Pairs the boot budget's long-standing deliberate leak with the
/// reclaim the device performs in `vTaskDelete(NULL)`.
pub fn release_task_spawn(model: &BootBudgetModel, spec: &TaskSpec) {
    model::release(model.stack_bytes(spec) + model.tcb_bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_stack(kind: TaskKind) -> u32 {
        match kind {
            TaskKind::Jvm => 16_384,
            TaskKind::JvmChild => 4_096,
            TaskKind::BgWorker => 2_048,
            TaskKind::Sensor => 1_024,
            TaskKind::FsWorker => 512,
        }
    }

    static MODEL: BootBudgetModel = BootBudgetModel {
        tasks: &[
            BootTask {
                name: "a",
                stack_bytes: 1000,
                sim_real: false,
            },
            BootTask {
                name: "b",
                stack_bytes: 2000,
                sim_real: true,
            },
        ],
        tcb_bytes: 100,
        queues_misc_bytes: 50,
        default_stack_bytes: default_stack,
    };

    #[test]
    fn model_arithmetic_counts_every_task_and_the_bucket() {
        // (1000 + 100) + (2000 + 100) + 50 — distinct values, so a dropped
        // term or a swapped field is a different number.
        assert_eq!(MODEL.modeled_boot_bytes(), 3250);
    }

    #[test]
    fn stack_bytes_prefers_the_spec_over_the_default() {
        let named = TaskSpec {
            name: "n",
            kind: TaskKind::Jvm,
            priority: 1,
            stack_bytes: Some(777),
        };
        let unnamed = TaskSpec {
            name: "u",
            kind: TaskKind::Sensor,
            priority: 1,
            stack_bytes: None,
        };
        assert_eq!(MODEL.stack_bytes(&named), 777);
        assert_eq!(MODEL.stack_bytes(&unnamed), 1_024);
    }

    #[test]
    fn a_spawn_charge_is_released_by_size() {
        let spec = TaskSpec {
            name: "t",
            kind: TaskKind::JvmChild,
            priority: 1,
            stack_bytes: None,
        };
        let before = model::outstanding();
        assert_eq!(charge_task_spawn(&MODEL, &spec), 4_096);
        assert_eq!(model::outstanding(), before + 1);
        release_task_spawn(&MODEL, &spec);
        assert_eq!(model::outstanding(), before);
    }
}
