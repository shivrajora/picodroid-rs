// SPDX-License-Identifier: GPL-3.0-only
//! RTOS abstraction — the `pd-rtos` crate (the [`Rtos`] trait, its link-time
//! facade, and the JVM run lock), re-exported unchanged, plus the two things
//! that belong to this crate: the FreeRTOS pieces every FreeRTOS-hosted family
//! shares, and the guard that keeps kernel names out of shared code.

pub use pd_rtos::*;

// What FreeRTOS-hosted families share regardless of port — the one place
// outside `hal/sim` that names a kernel symbol (E7).
pub mod freertos;

#[cfg(test)]
use test_support::source_scan;

/// Non-simulator shared code reaches the RTOS only through this module.
///
/// A text scan, like `gc_root_scan` and the family's `task_affinity` rules:
/// the code it polices is device code, so under `cargo test` there is
/// nothing to call. What it bans is any FreeRTOS API name (`vTaskDelay`,
/// `xQueueSend`, `pvPortMalloc`, …), the `freertos_rust` crate, and the
/// task-creation and pinning calls, anywhere in `picodroid-core/src` or
/// `pd-rtos/src` except the simulator's own kernel backing, `rtos::freertos` (E7's one allowed
/// exception), and this file, whose literals below spell every token.
#[cfg(test)]
mod seam_guard {
    use std::path::{Path, PathBuf};

    use super::source_scan::{read_stripped, rel, sources};

    fn skipped(path: &Path) -> bool {
        let s = path.to_string_lossy();
        s.contains("/hal/sim/") || s.ends_with("/rtos/freertos.rs") || s.ends_with("/rtos/mod.rs")
    }

    /// A FreeRTOS API name: one or two lowercase type-prefix letters, then
    /// one of the kernel's object families, then an uppercase letter —
    /// `vTaskDelay`, `uxTaskGetNumberOfTasks`, `xQueueSend`, `pvPortMalloc`.
    /// Rust's own `TaskSpec`/`TaskKind` have no such prefix and do not match.
    fn is_kernel_symbol(word: &str) -> bool {
        let prefix = word.len()
            - word
                .trim_start_matches(|c: char| c.is_ascii_lowercase())
                .len();
        if prefix == 0 || prefix > 2 {
            return false;
        }
        let rest = &word[prefix..];
        ["Task", "Queue", "Semaphore", "Timer", "Port", "Event"]
            .iter()
            .any(|family| {
                rest.strip_prefix(family)
                    .is_some_and(|tail| tail.starts_with(|c: char| c.is_ascii_uppercase()))
            })
    }

    const BANNED_TOKENS: &[&str] = &[
        "freertos_rust",
        "Task::new()",
        ".core_affinity(",
        "set_core_affinity(",
        "CurrentTask::",
    ];

    #[test]
    fn non_sim_core_reaches_the_rtos_only_through_the_seam() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files: Vec<PathBuf> = Vec::new();
        sources(&src, &["rs"], None, &mut files);
        // The seam itself is a crate of its own and is held to the same rule:
        // it declares `__pd_rtos_*` symbols and must name no kernel.
        let seam = Path::new(env!("CARGO_MANIFEST_DIR")).join("../pd-rtos/src");
        sources(&seam, &["rs"], None, &mut files);
        files.retain(|p| !skipped(p));
        files.sort();
        for must in [
            "lifecycle.rs",
            "threads.rs",
            "executors/main_queue.rs",
            "sim_boot.rs",
            "pd-rtos/src/lib.rs",
            "pd-rtos/src/run_lock.rs",
        ] {
            assert!(
                files.iter().any(|p| p.ends_with(must)),
                "scanner did not find {must} — the layout changed, not the code"
            );
        }
        assert!(files.len() > 60, "scanner found only {} files", files.len());

        let mut hits = Vec::new();
        for file in &files {
            let text = read_stripped(file);
            for token in BANNED_TOKENS {
                if text.contains(token) {
                    hits.push(format!("{}: {token}", rel(&src, file)));
                }
            }
            for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if is_kernel_symbol(word) {
                    hits.push(format!("{}: {word}", rel(&src, file)));
                }
            }
        }
        hits.sort();
        hits.dedup();
        assert!(
            hits.is_empty(),
            "shared code names an RTOS primitive directly. Everything the \
             framework needs from a kernel goes through `crate::rtos` (the \
             `Rtos` trait and its facade), so a second family can register \
             its own; the simulator's backing and `rtos::freertos` are the \
             only exceptions. Found:\n  {}",
            hits.join("\n  ")
        );
    }

    #[test]
    fn the_symbol_matcher_knows_freertos_from_rust() {
        for yes in [
            "vTaskDelay",
            "xQueueSend",
            "pvPortMalloc",
            "uxTaskGetNumberOfTasks",
            "xTimerCreate",
        ] {
            assert!(is_kernel_symbol(yes), "{yes}");
        }
        for no in [
            "TaskSpec",
            "TaskKind",
            "task_current",
            "Timeout",
            "portable",
            "export",
        ] {
            assert!(!is_kernel_symbol(no), "{no}");
        }
    }
}
