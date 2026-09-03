// SPDX-License-Identifier: GPL-3.0-only
//! Core placement for every FreeRTOS task this family creates.
//!
//! RP2040 and RP2350 run the SMP kernel with real dual-core scheduling
//! (`configNUMBER_OF_CORES 2`, `configRUN_MULTIPLE_PRIORITIES 1` — the
//! core-1 flash parker needs it). The JVM's shared heap is lock-free on the
//! strength of "one core interprets Java": `AtomicSection` covers the
//! compound heap operations, and everything else — every `putfield`, every
//! array store, `volatile` or not — is a plain store with no barrier, which
//! is correct only while every task that touches the heap runs on the same
//! core (docs/parity-audit.md THR-04 / X1; docs/quality-roadmap.md
//! § Cross-thread field visibility).
//!
//! The invariant is enforced three ways, and the tests at the bottom of
//! this file check all three, so it is verified rather than remembered:
//!
//! 1. **[`spawn`] is the only way to create a task**, and it takes the core
//!    as an argument: [`CORE0`] for everything, [`CORE1`] only for the tasks
//!    in [`CORE1_TASKS`]. No other file may write `Task::new()`,
//!    `.core_affinity(`, `xTaskCreate*`, `vTaskCoreAffinitySet` or
//!    `set_core_affinity`.
//! 2. **Create and pin are one scheduler-atomic step.** freertos-rust's
//!    builder calls `xTaskCreate` and only then `vTaskCoreAffinitySet`, and
//!    inside `xTaskCreate` the SMP scheduler (`prvYieldForTask`) hands the
//!    new task to any core running something of lower priority — an idle
//!    core 1 always qualifies — so a `Thread.start` child used to begin
//!    executing on core 1 and be evicted by IPI a microsecond later.
//!    [`spawn`] brackets the two calls in an `AtomicSection`
//!    (`vTaskSuspendAll`/`xTaskResumeAll`, the pair boot installs before the
//!    first task exists): with the scheduler suspended no core can switch
//!    context, so the mask is in place before the child can run anywhere.
//!    Neither call blocks, which is the section's one rule.
//!
//!    Not solved with `configTASK_DEFAULT_CORE_AFFINITY`: that needs
//!    `configIDLE_AFFINITY 1` beside it (an idle task inheriting a core-0
//!    mask leaves core 1 unschedulable), and pinning the idle tasks pins the
//!    *reaper* — only the main idle task, core 0's, runs
//!    `prvCheckTasksWaitingTermination`. Left free it floats to idle core 1
//!    while core 0 churns; pinned, a `Thread.start`/`join` loop that never
//!    lets core 0 idle leaks every finished child's 16 KB stack until
//!    `xTaskCreate` fails (threadparity on testbench_rp2350, 2026-08-31).
//!    `FreeRTOSConfig.h` must not set either; the config test checks.
//! 3. **Time slicing stays off.** A JVM task keeps its core until it
//!    blocks, so a switch between two JVM tasks is always a full context
//!    save at a kernel call, never mid-store (`task_priority.rs`, "One tier
//!    for all Java").
//!
//! Tasks this crate does not create: the kernel's idle tasks (assigned to
//! their cores at creation), the timer service (core 0 via
//! `configTIMER_SERVICE_TASK_CORE_AFFINITY` — its lvgl-tick callback shares
//! state with the UI task) and FreeRTOS+TCP's `IP-task`, which touches no
//! JVM state and declares its own placement in `FreeRTOSIPConfig.h`. The
//! simulator is exempt by construction: its POSIX port is single-core and
//! its spawn arm (`picodroid_core::hal::sim::rtos_freertos`) sets no
//! affinity.

/// Core 0 only. Every task that can touch the JVM heap, LVGL or the
/// executors: the JVM task, `Thread.start` children, the background pool,
/// the sensor sampler, the fs worker and the debug bridge.
#[cfg_attr(any(test, feature = "sim"), allow(dead_code))]
pub const CORE0: u32 = 0b01;

/// Core 1 only. Exactly the tasks in [`CORE1_TASKS`]: the flash parker,
/// which exists to hold core 1 still while core 0 has XIP off, and the
/// cyw43 driver task, whose PIO+DMA transport tolerates a loaded core 0
/// (docs/designs/cyw43-pio-transport.md). Neither touches JVM state.
#[cfg_attr(any(test, feature = "sim"), allow(dead_code))]
pub const CORE1: u32 = 0b10;

/// Task names (the first argument to [`spawn`], as a string literal)
/// allowed to pin to [`CORE1`]. Adding one means first arguing, at the
/// spawn site, that the task never touches the JVM heap, LVGL or the
/// executors — then listing it here.
#[cfg(test)]
const CORE1_TASKS: &[&str] = &["flashpark", "cyw43"];

/// `spawn` call sites the scan must find. Pinned like `EXPECTED_PROVIDERS`
/// in `gc_root_registration.rs`: a site that is added, moved or deleted
/// changes this number, so the scan cannot pass on an empty match.
#[cfg(test)]
const EXPECTED_SPAWNS: usize = 6;

/// Create a task pinned to `core`, atomically with respect to the
/// scheduler — see rule 2 in the module docs. `stack_words` is in FreeRTOS
/// words (`boot_budget` speaks bytes for the seam and words for the sizes
/// it hands this family; the ÷4 lives in `glue.rs`).
///
/// Pre-scheduler callers (`boot_tasks.rs`) pay nothing: both hooks are
/// no-ops until the scheduler runs, and a task created then cannot start
/// before its mask is set anyway.
#[cfg(all(not(any(test, feature = "sim")), feature = "family-rp"))]
pub fn spawn<F>(
    name: &str,
    stack_words: u16,
    priority: u8,
    core: u32,
    body: F,
) -> Result<freertos_rust::Task, freertos_rust::FreeRtosError>
where
    F: FnOnce(freertos_rust::Task) + Send + 'static,
{
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    freertos_rust::Task::new()
        .name(name)
        .stack_size(stack_words)
        .priority(freertos_rust::TaskPriority(priority))
        .core_affinity(core)
        .start(body)
}

#[cfg(test)]
#[path = "../../../test_support/source_scan.rs"]
mod source_scan;

/// Source scan — see the module docs for what it enforces and why it reads
/// text: the spawn sites live in `boot_tasks.rs` and `glue.rs::rtos_impl`,
/// both `cfg(not(test))`, so under `cargo test` there is nothing to call.
#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::source_scan::{read_stripped, sources};

    const SELF: &str = "task_affinity.rs";

    fn crate_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    }

    /// `path` relative to the repo root, for messages.
    fn rel(path: &Path) -> String {
        let root = crate_root();
        let root = root.parent().and_then(Path::parent).unwrap_or(&root);
        super::source_scan::rel(root, path)
    }

    /// Every file with one of `exts` under `dir`, except this one: it quotes
    /// every token the scans look for, in string literals no comment stripper
    /// removes, and its one real `Task::new()` is checked on its own by
    /// `spawn_is_scheduler_atomic`.
    fn own_sources(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
        sources(dir, exts, Some(SELF), out)
    }

    struct Site {
        file: PathBuf,
        /// The name argument when it is a string literal; `None` when it
        /// is an expression (`spec.name` in the `rtos_impl` arm).
        name: Option<String>,
        /// The core argument, whitespace removed.
        core: String,
    }

    /// Every `task_affinity::spawn(` call under `platforms/rp/src`.
    fn spawn_sites() -> Vec<Site> {
        let mut files = Vec::new();
        own_sources(&crate_root().join("src"), &["rs"], &mut files);
        files.sort();
        let mut sites = Vec::new();
        for file in files {
            let text = read_stripped(&file);
            let mut rest = text.as_str();
            while let Some(at) = rest.find("task_affinity::spawn(") {
                let args = &rest[at + "task_affinity::spawn(".len()..];
                // Arguments up to the body closure. Every site passes the
                // body as a closure, so `|` is the boundary; a site that
                // does not gets told so.
                let end = args.find('|').unwrap_or_else(|| {
                    panic!(
                        "{}: task_affinity::spawn call without a closure body — \
                         pass the body as `move |_| …` so the scan can read \
                         the core argument",
                        rel(&file)
                    )
                });
                let parts: Vec<&str> = args[..end].split(',').map(str::trim).collect();
                assert!(
                    parts.len() >= 4,
                    "{}: task_affinity::spawn needs (name, stack_words, priority, \
                     core, body); found {} arguments before the body",
                    rel(&file),
                    parts.len()
                );
                let name = parts[0]
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .map(str::to_string);
                sites.push(Site {
                    file: file.clone(),
                    name,
                    core: parts[3].split_whitespace().collect(),
                });
                rest = &args[end..];
            }
        }
        sites
    }

    /// Rule 1: every site names its core with the constant, and only
    /// `CORE1_TASKS` name core 1.
    #[test]
    fn every_task_names_its_core() {
        let sites = spawn_sites();
        assert_eq!(
            sites.len(),
            super::EXPECTED_SPAWNS,
            "found {} task_affinity::spawn sites under platforms/rp/src, \
             EXPECTED_SPAWNS = {}. A spawn site was added, moved or removed: \
             check it names its core, then update the constant.",
            sites.len(),
            super::EXPECTED_SPAWNS
        );

        let mut core1_seen: Vec<String> = Vec::new();
        for s in &sites {
            let label = format!(
                "{} (task {})",
                rel(&s.file),
                s.name.as_deref().unwrap_or("<expression>")
            );
            if s.core.ends_with("task_affinity::CORE0") {
                continue;
            }
            assert!(
                s.core.ends_with("task_affinity::CORE1"),
                "{label}: core argument `{}` — spell it task_affinity::CORE0 \
                 (or CORE1 for a task in CORE1_TASKS), never a literal or a \
                 variable, so the scan can tell which core it is. An \
                 unpinned JVM-adjacent task can interpret Java on core 1 \
                 against the lock-free shared heap (task_affinity.rs, THR-04).",
                s.core
            );
            let name = s.name.as_deref().unwrap_or_else(|| {
                panic!(
                    "{label}: a core-1 task must name itself with a string \
                     literal so CORE1_TASKS can vouch for it"
                )
            });
            assert!(
                super::CORE1_TASKS.contains(&name),
                "{label}: pinned to core 1 but not in CORE1_TASKS. Core 1 is \
                 for tasks that never touch the JVM heap, LVGL or the \
                 executors; argue that at the spawn site, then list it."
            );
            core1_seen.push(name.to_string());
        }
        for t in super::CORE1_TASKS {
            assert_eq!(
                core1_seen.iter().filter(|n| n == t).count(),
                1,
                "CORE1_TASKS lists {t:?} but the scan found {} core-1 sites by \
                 that name — the allowlist has rotted; fix it.",
                core1_seen.iter().filter(|n| n == t).count()
            );
        }
    }

    /// Rule 1, other half: `spawn` is the only way a task is created or
    /// moved. Covers this crate's Rust and C. The shared modules in
    /// picodroid-core are covered by that crate's own seam guard
    /// (`rtos::seam_guard`), which bans every direct kernel call there —
    /// one owner per tree, so a second family does not have to scan core.
    #[test]
    fn no_task_is_created_or_moved_outside_spawn() {
        let mut files = Vec::new();
        own_sources(&crate_root().join("src"), &["rs", "c", "h"], &mut files);
        files.sort();
        assert!(
            files.iter().any(|p| p.ends_with("glue.rs"))
                && files.iter().any(|p| p.ends_with("boot_tasks.rs")),
            "scanner found neither glue.rs nor boot_tasks.rs — the path layout \
             changed, not the code"
        );

        let banned = [
            "Task::new()",
            ".core_affinity(",
            "xTaskCreate",
            "vTaskCoreAffinitySet",
            "set_core_affinity(",
        ];
        let mut hits = Vec::new();
        for file in &files {
            let text = read_stripped(file);
            for token in banned {
                if text.contains(token) {
                    hits.push(format!("{}: {token}", rel(file)));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "tasks are created only by task_affinity::spawn (device) or \
             rtos::spawn (shared code, whose device arm calls it). Found:\n  {}",
            hits.join("\n  ")
        );
    }

    /// Rule 2: the one real builder chain, in `spawn`, sits inside an
    /// `AtomicSection` and pins with the `core` argument.
    #[test]
    fn spawn_is_scheduler_atomic() {
        let me = read_stripped(&crate_root().join("src").join(SELF));
        let body_at = me
            .find("pub fn spawn<F>(")
            .expect("task_affinity::spawn definition");
        // Up to the helper's closing brace at column 0, so the test
        // module's own string literals below are not part of the search.
        let body_end = me[body_at..]
            .find("\n}\n")
            .expect("task_affinity::spawn closing brace");
        let body = &me[body_at..body_at + body_end];
        let section = body
            .find("AtomicSection::enter()")
            .expect("spawn must enter an AtomicSection");
        let create = body.find("Task::new()").expect("spawn must build the task");
        assert!(
            section < create,
            "spawn must enter the AtomicSection before Task::new(), or the \
             child can be scheduled onto core 1 between xTaskCreate and \
             vTaskCoreAffinitySet"
        );
        assert!(
            body[create..]
                .find(".core_affinity(core)")
                .is_some_and(|pin| {
                    body[create..]
                        .find(".start(")
                        .is_some_and(|start| pin < start)
                }),
            "spawn must call .core_affinity(core) before .start("
        );
        assert_eq!(
            body.matches("Task::new()").count(),
            1,
            "exactly one builder chain belongs in task_affinity.rs"
        );
    }

    /// One `#define` per line, comments stripped, whitespace removed, so
    /// `( 1 << 0 )` and `(1<<0)` compare equal.
    fn defines(path: &Path) -> Vec<String> {
        read_stripped(path)
            .lines()
            .map(|l| l.split_whitespace().collect::<String>())
            .filter(|l| l.starts_with("#define"))
            .collect()
    }

    fn assert_defined(path: &Path, want: &str) {
        let key = want.split_whitespace().collect::<String>();
        let found = defines(path).into_iter().any(|l| l == key);
        assert!(
            found,
            "{}: expected `{want}`. task_affinity.rs explains why each of \
             these is load-bearing for the single-core JVM heap.",
            rel(path)
        );
    }

    fn assert_not_defined(path: &Path, symbol: &str) {
        let key = format!("#define{symbol}");
        let found = defines(path).into_iter().any(|l| l.starts_with(&key));
        assert!(
            !found,
            "{}: `{symbol}` must not be defined — see rule 2 in \
             task_affinity.rs: pinning the idle tasks pins the reaper, and \
             a Thread.start/join loop then leaks every finished child's stack.",
            rel(path)
        );
    }

    /// Rule 3 and the kernel side of rule 2: SMP with affinity on, the
    /// timer service on core 0, no time slicing, the idle tasks (and the
    /// default mask) left to the kernel; the one library task declares its
    /// placement.
    #[test]
    fn kernel_config_matches_the_model() {
        let cfg = crate_root().join("mcus/rp/FreeRTOSConfig.h");
        for want in [
            "#define configNUMBER_OF_CORES 2",
            "#define configUSE_CORE_AFFINITY 1",
            "#define configRUN_MULTIPLE_PRIORITIES 1",
            "#define configTIMER_SERVICE_TASK_CORE_AFFINITY ( 1 << 0 )",
            "#define configUSE_TIME_SLICING 0",
        ] {
            assert_defined(&cfg, want);
        }
        for symbol in ["configIDLE_AFFINITY", "configTASK_DEFAULT_CORE_AFFINITY"] {
            assert_not_defined(&cfg, symbol);
        }

        // FreeRTOS+TCP creates the IP task itself. Whatever it chooses, it
        // must choose, so the placement of every task in the system is
        // written down somewhere the scan can read.
        let ip = crate_root().join("src/hal/rp/port/FreeRTOSIPConfig.h");
        assert!(
            defines(&ip)
                .iter()
                .any(|l| l.starts_with("#defineipconfigIP_TASK_AFFINITY")),
            "{}: ipconfigIP_TASK_AFFINITY must be defined explicitly — see \
             task_affinity.rs",
            rel(&ip)
        );
    }

    #[test]
    fn masks_are_one_core_each() {
        assert_eq!(super::CORE0.count_ones(), 1);
        assert_eq!(super::CORE1.count_ones(), 1);
        assert_eq!(super::CORE0 & super::CORE1, 0);
        assert_eq!(super::CORE0 | super::CORE1, 0b11, "two cores, no more");
    }
}
