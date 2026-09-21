// SPDX-License-Identifier: GPL-3.0-only
//! Text guards for the 2026-09-13 QA fixes whose code a host `cargo test`
//! cannot reach.
//!
//! `lifecycle`, `native_handler` and the `graphics` tree are `cfg(not(test))`
//! — they reach LVGL, the HAL and the JVM natives — so under `cargo test`
//! there is nothing to call. Behaviourally these fixes are covered by the
//! seven `qa_*` apps in the sim (the `qa` lane of `scripts/pre-commit`); what
//! is checked here is the *shape* each fix put in place, so a refactor that
//! quietly drops it fails in seconds rather than in the next QA round. The
//! same reasoning and the same helpers as `rtos`'s seam guard,
//! `porting`'s checklist and `native_handler::alloc_scan`.

/// Text-scan helpers, shared with the other guards in the workspace.
#[cfg(test)]
use test_support::source_scan;

#[cfg(test)]
mod tests {
    use super::source_scan::read_stripped;
    use std::path::{Path, PathBuf};

    fn src(rel: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel)
    }

    /// The body of `fn <name>` — up to the next top-level `fn`, or the end
    /// of the file. Comment-stripped, so a quoted or commented call neither
    /// trips a rule nor satisfies one. The name may carry generics
    /// (`dispatch_with<B: GraphicsBackend>`), so the match stops at the name.
    fn function_body(path: &Path, name: &str) -> String {
        let text = read_stripped(path);
        let needle = std::format!("fn {name}");
        let start = text
            .find(&needle)
            .unwrap_or_else(|| panic!("{} declares no {needle}", path.display()));
        let rest = &text[start + needle.len()..];
        let end = ["\nfn ", "\npub fn ", "\npub(crate) fn ", "\npub(super) fn "]
            .iter()
            .filter_map(|m| rest.find(m))
            .min();
        match end {
            Some(end) => rest[..end].to_string(),
            None => rest.to_string(),
        }
    }

    // J-fix 7b97387b: the task that runs the app is recorded as the UI task
    // before `Application.onCreate`, not only when an Activity loop starts.
    // Until then every task passed for the UI task, so a background-pool job
    // in an Activity-less app saw `Thread.currentThread()` named "main".
    #[test]
    fn run_application_records_the_ui_task_before_on_create() {
        let body = function_body(&src("lifecycle/mod.rs"), "run_application");
        let note = body
            .find("note_ui_task()")
            .expect("run_application must record the UI task (QA 2026-09-13)");
        let on_create = body
            .find("m::onCreate")
            .expect("run_application must invoke Application.onCreate");
        assert!(
            note < on_create,
            "note_ui_task() must run before Application.onCreate, or an \
             Activity-less app's worker threads all pass for the UI task"
        );
    }

    // J-fix 6acb47a8: an exception out of a main-queue Runnable is logged.
    // It used to come back as a JvmError the UI loop discarded, so a chain of
    // posted steps stopped with nothing in the log to say why.
    #[test]
    fn a_failed_main_queue_runnable_is_logged() {
        let body = function_body(&src("lifecycle/mod.rs"), "run_activity");
        assert!(
            body.contains("mainExecutor Runnable error"),
            "run_activity must log an exception escaping a main-queue \
             Runnable, not discard it (QA 2026-09-13)"
        );
    }

    // J-fix 0f5ab30e: every enqueue from a native arm checks the result. A
    // dropped `finish()` or `stopService()` is an app that silently never
    // ends; the arms turn a full queue into an IllegalStateException.
    #[test]
    fn every_native_enqueue_checks_whether_the_op_was_queued() {
        for file in ["native_handler/mod.rs", "native_handler/app_services.rs"] {
            let path = src(file);
            let text = read_stripped(&path);
            let lines: Vec<&str> = text.lines().map(str::trim).collect();
            for (n, line) in lines.iter().enumerate() {
                if !line.contains("enqueue_op(") || line.starts_with("pub fn enqueue_op") {
                    continue;
                }
                // The answer is kept when the call is negated in place
                // (`!handler.enqueue_op(…)`, anywhere in a condition) or
                // bound (`let queued = …`, possibly on the line rustfmt
                // wrapped the call away from).
                let negated = line.split("enqueue_op(").next().is_some_and(|before| {
                    before
                        .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '.')
                        .ends_with('!')
                });
                let opener = if n > 0 { lines[n - 1] } else { "" };
                let bound = |l: &str| l.contains("let queued");
                if negated || bound(line) || bound(opener) {
                    continue;
                }
                panic!(
                    "{}:{}: `{}` drops the enqueue result; a full queue must \
                     become an IllegalStateException (QA 2026-09-13)",
                    file,
                    n + 1,
                    line
                );
            }
        }
    }

    // J-fix b105d077: a click-listener registration the fixed-size table
    // cannot hold is an IllegalStateException the app can read. It used to be
    // a line in the log and a view that never clicked.
    #[test]
    fn a_refused_click_listener_registration_throws() {
        let body = function_body(&src("graphics/view.rs"), "register_click_listener");
        assert!(
            body.contains("lvgl_button::register_click_listener("),
            "register_click_listener must consult the backend's answer"
        );
        assert!(
            body.contains("java_lang_IllegalStateException"),
            "a click-listener table that cannot hold the registration must \
             throw, not log and carry on (QA 2026-09-13)"
        );
    }

    // J-fix b7ec1abd: a View native whose receiver was released by
    // `removeView` is refused before any native call. The bench boards cast
    // `lv_obj_t*` to a 32-bit handle with no table to say the widget was
    // freed, so the stale pointer went straight back to LVGL.
    #[test]
    fn the_graphics_router_refuses_a_released_receiver() {
        let path = src("native_handler/graphics/mod.rs");
        let body = function_body(&path, "dispatch_with");
        let check = body
            .find("released_receiver(ctx)")
            .expect("dispatch_with must check for a released receiver (QA 2026-09-13)");
        let first_arm = body
            .find("let class_hit")
            .expect("dispatch_with must dispatch class-specific arms");
        assert!(
            check < first_arm,
            "the released-receiver check must come before any dispatch, or a \
             freed widget pointer reaches LVGL"
        );
        // And the check is the handle the Java side zeroes, not a guess.
        let text = read_stripped(&path);
        assert!(
            text.contains("NATIVE_HANDLE") && text.contains("Value::Int(0)"),
            "released_receiver must read the view's native handle field"
        );
    }
}
