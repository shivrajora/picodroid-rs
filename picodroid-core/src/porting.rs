// SPDX-License-Identifier: GPL-3.0-only
//! What a port provides — the one page a porter reads.
//!
//! A family crate (`platforms/<family>/`) binds this crate to a chip. Every
//! item it has to implement, register, or hand over is re-exported here, so
//! this module *is* the checklist and cannot fall behind the code: the test
//! at the bottom scans the seam files for every `pub trait` and every
//! exported macro and fails if one is missing here or in the porting guide.
//!
//! The list, in the order a bring-up meets it. "Trait + macro" items are
//! bound at link time — the macro emits the `__pd_*` symbols this crate's
//! facade calls, so a drifted signature fails to compile at your impl.
//!
//! 1. **The hardware layer (trait + macro).** Implement [`HalDisplay`],
//!    [`HalGpio`], [`HalClock`], [`HalTouch`], [`HalI2c`], [`HalAdc`],
//!    [`HalPwm`], [`HalSpi`] and [`HalUart`] for one type and register it with
//!    [`set_hal!`]. Types that cross: [`Pull`], [`EdgeTrigger`], [`GpioEvent`]
//!    (stamp `t_us` at *enqueue* time), [`NetError`]. `HalI2c` and `HalSpi`
//!    default their Java-array methods over the slice ones ([`array_io`]);
//!    write the slice pair and stop. [`GpioEventRing`] and [`TouchOverride`]
//!    are the edge queue and scripted-touch state machine every family
//!    needs — use them rather than writing your own.
//!    [`HalNet`] only on a board with `has_network`, via [`set_hal_net!`]
//!    (a FreeRTOS+TCP family registers core's `FreeRtosTcpNet` there —
//!    feature `freertos-tcp` — and writes no socket code);
//!    [`HalFs`] via [`set_hal_fs!`] — see item 4.
//! 2. **The kernel (trait + macro).** Implement [`Rtos`] and register with
//!    [`set_rtos!`]. Stack sizes are **bytes**; map [`TaskKind`] to your
//!    stack and core policy (a family that runs from the flash it writes must
//!    pin [`TaskKind::FsWorker`] to the core that does the writing); make
//!    `scheduler_running` a real scheduler-state query, never
//!    `task_current() != 0`; a `spawn` may refuse. Priorities come from
//!    [`task_priority`]. On FreeRTOS, call
//!    [`install_heap_atomic_hooks`] before the first task exists.
//! 3. **The hooks (trait + macro).** Implement [`PlatformHooks`] and
//!    register with [`set_platform_hooks!`]. `register_gc_roots` is required
//!    on purpose: an empty body is a decision. Keep your own
//!    `gc_root_registration.rs` with an `EXPECTED_PROVIDERS` and assert at
//!    boot that [`provider_count`] equals [`CORE_EXPECTED_PROVIDERS`] plus
//!    yours.
//! 4. **The filesystem.** Enable this crate's `littlefs` feature, implement
//!    [`FsBackingStore`] over your flash region (the block arithmetic is
//!    [`FsGeometry`]'s), call [`init_device`] then [`spawn_worker`] before the
//!    scheduler starts, and register `set_hal_fs!(LittleFsHal)` — or implement
//!    [`HalFs`] yourself over another filesystem.
//! 5. **The debug bridge and installer (generic parameters, optional).**
//!    Implement [`PdbTransport`] (a byte pipe — `read_byte_timeout` must
//!    busy-wait if your tick stops during flash writes), [`SysmonSource`],
//!    [`CoreCoordinator`] (park the JVM core before flash is touched), and
//!    [`PapkSlotFlash`] (three constants, erase, program, reset — [`PapkSlot`]
//!    turns it into the [`PapkFlash`] the installer wants). Hand the four to
//!    [`run_pdb_task`] from your bridge task. Read the installed app at boot
//!    with [`read_mapped`]. Wire layouts and the USB identity are
//!    `pdb_protocol`'s; never retype them.
//! 6. **The simulator.** One [`register_sim_platform!`] call with your GC
//!    roots, a `static` [`BootBudgetModel`] of the tasks your device creates
//!    at boot ([`BootTask`]), and the function that runs your app; it
//!    generates the simulator's `Rtos`, `PlatformHooks` and `sim_main()`.
//!    One [`declare_sim_global_allocator!`] call in `main.rs`. Your simulator
//!    Cargo feature must be named `sim`.
//! 7. **Boot, data and discipline.** Hand the app bytes to [`run_app`] and
//!    never construct a `Jvm` yourself. Your JVM task's supervisor loop owes:
//!    clear the stop flag, run, abort child delays, [`wake_all_parked`],
//!    drain live children, then park for a flash write or wait for the next
//!    install (`platforms/rp/src/boot_tasks.rs` is the reference; do not put
//!    a stop check in your HAL `sleep` — shared code owns it). Your `build.rs`
//!    emits the capability `cfg`s (`has_display`, `has_touch`, `has_buttons`,
//!    `has_network`, `network_<type>`, `network_link_<kind>`, `any_sensor`,
//!    `sensor_<kind>`) from
//!    `board.toml` through `build_support::board_cfg`, and reads
//!    [`board_cfg`]. LVGL is compiled by *this* crate's build script — never
//!    by a family. Logging is [`pd_info!`] and friends: defmt on device,
//!    `eprintln` on the host, so link a defmt sink.
//!
//! 8. **The network (optional, FreeRTOS+TCP families).** Enable this crate's
//!    `freertos-tcp` feature. Write a `NetworkInterface_<X>.c` against
//!    FreeRTOS+TCP's own `NetworkInterface_t` that defines
//!    `pxPicodroidNetLink_FillInterfaceDescriptor`; implement [`NetLink`]
//!    for the same chip; ship `FreeRTOSIPConfig_family.h` (your IP-task
//!    affinity) and `uint32_t picodroid_port_entropy32(void)`; spawn
//!    [`run_link_task`] from your boot on a task with your own core and
//!    stack; register `set_hal_net!(FreeRtosTcpNet)`. Compile it all with
//!    `build_support::network::build_freertos_tcp(&NetStackBuild { … })`.
//!    [`LinkKind`] is for logs — Java's link kind is the `network_link_<kind>`
//!    cfg from `board.toml`'s `network_type`. Reference: the RP family's
//!    `hal/rp/cyw43/` and `hal/rp/port/net/`.
//!
//! What is deliberately *not* here: anything under `platforms/rp/src/hal/rp`.
//! That is the reference implementation of the items above, not a contract.

// ── 1. hardware layer ──────────────────────────────────────────────────────
pub use crate::hal::array_io;
pub use crate::hal::event_ring::GpioEventRing;
pub use crate::hal::touch_override::{OverrideSample, TouchOverride};
pub use crate::hal::types::{EdgeTrigger, GpioEvent, LinkKind, NetError, NetErrorKind, Pull};
pub use crate::hal::{
    HalAdc, HalClock, HalDisplay, HalFs, HalGpio, HalI2c, HalNet, HalPwm, HalSpi, HalTouch,
    HalUart, NetLink,
};
pub use crate::{
    set_hal, set_hal_adc, set_hal_clock, set_hal_display, set_hal_fs, set_hal_gpio, set_hal_i2c,
    set_hal_net, set_hal_pwm, set_hal_spi, set_hal_touch, set_hal_uart,
};

// ── 2. kernel ──────────────────────────────────────────────────────────────
#[cfg(not(test))]
pub use crate::rtos::freertos::install_heap_atomic_hooks;
pub use crate::rtos::{RawMutex, RawQueue, RawSem, RawTask, Rtos, TaskKind, TaskSpec, Timeout};
pub use crate::set_rtos;
pub use crate::task_priority;

// ── 3. hooks ───────────────────────────────────────────────────────────────
pub use crate::gc_root_registration::EXPECTED_PROVIDERS as CORE_EXPECTED_PROVIDERS;
pub use crate::gc_roots::provider_count;
pub use crate::host::{NativeHeapStats, PlatformHooks};
pub use crate::set_platform_hooks;

// ── 4. filesystem ──────────────────────────────────────────────────────────
#[cfg(all(feature = "littlefs", not(test)))]
pub use crate::fs::{init_device, spawn_worker, FsBackingStore, FsGeometry, LittleFsHal};
#[cfg(all(feature = "freertos-tcp", has_network, not(any(test, feature = "sim"))))]
pub use crate::hal::freertos_tcp::{run_link_task, FreeRtosTcpNet};

// ── 5. debug bridge and installer ──────────────────────────────────────────
pub use crate::install::{
    read_mapped, run_install, CoreCoordinator, InstallError, InstallTransport, PapkFlash, PapkSlot,
    PapkSlotFlash, ReadError,
};
pub use crate::pdb::{
    run_pdb_task, PdbTransport, SysmonSample, SysmonSource, TaskSample, MAX_TASKS,
};

// ── 6. simulator ───────────────────────────────────────────────────────────
#[cfg(any(test, feature = "sim"))]
pub use crate::hal::sim::boot_budget::{BootBudgetModel, BootTask};
#[cfg(any(test, feature = "sim"))]
pub use crate::{declare_sim_global_allocator, register_sim_platform};

// ── 7. boot, data and discipline ───────────────────────────────────────────
pub use crate::board_cfg;
#[cfg(not(test))]
pub use crate::boot::run_app;
pub use crate::executors::main_queue::enqueue_wake;
pub use crate::threads::wake_all_parked;
pub use crate::{pd_debug, pd_error, pd_info, pd_trace, pd_warn};

#[cfg(test)]
#[path = "../../test_support/source_scan.rs"]
mod source_scan;

/// The checklist cannot drift from the code, or the guide from the checklist.
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use super::source_scan::{read_stripped, sources};

    /// Files whose `pub trait`s are porting seams — what a family implements.
    /// Other traits in this crate (driver buses, root providers) are internal.
    const SEAM_FILES: &[&str] = &[
        "hal/traits.rs",
        "rtos/mod.rs",
        "host.rs",
        "pdb/mod.rs",
        "pdb/sysmon.rs",
        "install/orchestrator.rs",
        "install/transport.rs",
        "install/slot.rs",
        "fs/mod.rs",
    ];

    /// Pinned like `EXPECTED_PROVIDERS`: a seam trait or exported macro that
    /// is added or removed changes this number, so the scan cannot pass on an
    /// empty match — and whoever changes it has to read this list.
    const EXPECTED_SEAM_ITEMS: usize = 42;

    fn src() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// Every `pub trait` in the seam files, plus every `#[macro_export]`
    /// macro anywhere in this crate.
    fn seam_items() -> BTreeSet<String> {
        let mut items = BTreeSet::new();
        for file in SEAM_FILES {
            let text = read_stripped(&src().join(file));
            for line in text.lines() {
                let line = line.trim();
                let Some(rest) = line
                    .strip_prefix("pub unsafe trait ")
                    .or_else(|| line.strip_prefix("pub trait "))
                else {
                    continue;
                };
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                items.insert(name);
            }
        }
        let mut files = Vec::new();
        sources(&src(), &["rs"], None, &mut files);
        for file in files {
            let text = read_stripped(&file);
            let mut lines = text.lines().peekable();
            while let Some(line) = lines.next() {
                if line.trim() != "#[macro_export]" {
                    continue;
                }
                let Some(next) = lines.next() else { break };
                if let Some(rest) = next.trim().strip_prefix("macro_rules! ") {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    items.insert(name);
                }
            }
        }
        items
    }

    fn names_missing_from(text: &str, items: &BTreeSet<String>) -> Vec<String> {
        // Whole-token match: `HalI2c` must not be satisfied by `HalI2cX`.
        let tokens: BTreeSet<&str> = text
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .collect();
        items
            .iter()
            .filter(|i| !tokens.contains(i.as_str()))
            .cloned()
            .collect()
    }

    #[test]
    fn checklist_is_complete() {
        let items = seam_items();
        assert_eq!(
            items.len(),
            EXPECTED_SEAM_ITEMS,
            "the set of seam traits and exported macros changed: {items:?}. \
             Add the new item to porting.rs (the re-exports and the numbered \
             list) and to the porting guide, then update EXPECTED_SEAM_ITEMS."
        );
        let me = std::fs::read_to_string(src().join("porting.rs")).unwrap();
        let missing = names_missing_from(&me, &items);
        assert!(
            missing.is_empty(),
            "seam items not named in porting.rs: {missing:?}. A porter reads \
             this module as the checklist; an item that exists in the crate but \
             not here is an obligation nobody wrote down."
        );
    }

    #[test]
    fn porting_guide_names_every_seam() {
        let guide = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../website/src/content/docs/reference/porting-guide.md");
        let text = std::fs::read_to_string(&guide)
            .unwrap_or_else(|e| panic!("read {}: {e}", guide.display()));
        let missing = names_missing_from(&text, &seam_items());
        assert!(
            missing.is_empty(),
            "the porting guide ({}) never mentions: {missing:?}. Every seam \
             trait and registration macro must be named there, so the guide \
             cannot describe an interface the code no longer has.",
            guide.display()
        );
    }
}
