// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

/// Runnables handed to `Thread.start`, rooted from the moment of spawn until
/// their child task exits. The task closure captures the obj_ref as a raw
/// u16, and idiomatic Java (`new Thread(new Work()).start();`) drops every
/// Java-side reference immediately — so between spawn and the child's first
/// frame (and on any GC that races the child's startup) the Runnable had no
/// root at all and was swept while the parent churned (bugbash B1: all three
/// threadstress workers died on their first getfield). Same hazard class as
/// the executor queues (F2), one queue further out. Guarded by an
/// atomic-section like the heap's own compound ops: start() runs on the
/// spawning task, the release on the child.
const MAX_SPAWNED: usize = 8;
struct SpawnedCell(
    core::cell::Cell<[u16; MAX_SPAWNED]>,
    core::cell::Cell<usize>,
);
unsafe impl Sync for SpawnedCell {}
static SPAWNED_RUNNABLES: SpawnedCell = SpawnedCell(
    core::cell::Cell::new([0; MAX_SPAWNED]),
    core::cell::Cell::new(0),
);

fn spawned_push(r: u16) -> bool {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let mut arr = SPAWNED_RUNNABLES.0.get();
    let len = SPAWNED_RUNNABLES.1.get();
    if len >= MAX_SPAWNED {
        return false;
    }
    arr[len] = r;
    SPAWNED_RUNNABLES.0.set(arr);
    SPAWNED_RUNNABLES.1.set(len + 1);
    true
}

fn spawned_remove(r: u16) {
    let _atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let mut arr = SPAWNED_RUNNABLES.0.get();
    let len = SPAWNED_RUNNABLES.1.get();
    if let Some(pos) = arr[..len].iter().position(|&x| x == r) {
        arr.copy_within(pos + 1..len, pos);
        arr[len - 1] = 0;
        SPAWNED_RUNNABLES.0.set(arr);
        SPAWNED_RUNNABLES.1.set(len - 1);
    }
}

/// Drops the root when the child exits, on every path (error included).
struct SpawnedRootGuard(u16);
impl Drop for SpawnedRootGuard {
    fn drop(&mut self) {
        spawned_remove(self.0);
    }
}

/// GC roots: every Runnable whose child task has been spawned and has not
/// exited. See [`SPAWNED_RUNNABLES`].
pub fn visit_spawned_runnable_roots(visit: &mut dyn FnMut(u16)) {
    let arr = SPAWNED_RUNNABLES.0.get();
    let len = SPAWNED_RUNNABLES.1.get();
    for &r in &arr[..len] {
        visit(r);
    }
}

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    match (class_name, method_name) {
        ("picodroid/os/SystemClock", "sleep") => Some(crate::os::system_clock::sleep(ctx.args)),
        ("picodroid/os/SystemClock", "elapsedRealtimeNanos") => {
            Some(crate::os::system_clock::elapsed_realtime_nanos())
        }
        ("picodroid/os/SystemClock", "setCurrentTimeMillis") => {
            Some(crate::os::system_clock::set_current_time_millis(ctx.args))
        }
        // Elapsed-since-boot until SystemClock.setCurrentTimeMillis anchors
        // the epoch (offset stays 0 before that, preserving the historical
        // behaviour for apps that never sync).
        ("java/lang/System", "currentTimeMillis") => {
            let nanos = crate::hal::system_clock::elapsed_realtime_nanos();
            let millis = nanos / 1_000_000 + crate::os::system_clock::wall_offset_ms();
            Some(Ok(Some(Value::Long(millis))))
        }
        ("picodroid/content/pm/PackageManager", "hasSystemFeature") => {
            // args[0] = this, args[1] = feature name String
            let supported = match ctx.args.get(1) {
                Some(Value::Reference(idx)) => match ctx.strings.resolve(*idx) {
                    // FEATURE_WIFI: board has a wireless driver compiled in.
                    Some("picodroid.hardware.wifi") => cfg!(has_network),
                    _ => false,
                },
                _ => false,
            };
            Some(Ok(Some(Value::Int(supported as i32))))
        }
        ("picodroid/concurrent/Thread", "start") => {
            if let Some(Value::ObjectRef(thread_idx)) = ctx.args.first() {
                if let Some(Value::ObjectRef(runnable_obj_idx)) =
                    ctx.objects.get_field(*thread_idx, 0)
                {
                    let class_name: &'static str = ctx
                        .objects
                        .class_name(runnable_obj_idx)
                        .ok_or(JvmError::InvalidReference)
                        .ok()?;

                    // `Thread.setPriority` is advisory: every task that
                    // interprets Java runs at the one JVM tier, because the
                    // shared heap's safety rests on "a running JVM task
                    // keeps the core until it blocks" — see `task_priority`.
                    let spec = crate::rtos::TaskSpec {
                        // The class name rides along as the task name so the
                        // platform can identify the Runnable — the simulator
                        // prints it when declining, and it shows up in the
                        // debug bridge's task list on device.
                        name: class_name,
                        kind: crate::rtos::TaskKind::JvmChild,
                        priority: crate::task_priority::PRIORITY_JVM_NORM,
                        stack_bytes: None, // platform's JvmChild default
                    };

                    // Root the Runnable BEFORE the task exists: the closure
                    // holds it only as a raw u16 (see SPAWNED_RUNNABLES).
                    if !spawned_push(runnable_obj_idx) {
                        crate::pd_warn!(
                            "Thread.start: more than {} live threads, {} not started",
                            MAX_SPAWNED,
                            class_name
                        );
                        return Some(Ok(None));
                    }
                    // One call for every target. Core-0 pinning and the debug
                    // bridge's child-task bookkeeping live in the platform's
                    // Rtos::spawn. The simulator runs it as a real task on
                    // its hosted kernel; only the `cargo test` backing
                    // declines the kind (no scheduler runs there) and
                    // reports it.
                    let spawned = crate::rtos::spawn(
                        &spec,
                        alloc::boxed::Box::new(move || {
                            // Shared class set (boot::SHARED_JVM): children
                            // read the set `run_app` published instead of
                            // building a private `Jvm` + re-running
                            // `load_classes` — that duplicate cost ≈14 KB of
                            // parsed metadata per thread, permanently
                            // (docs/mem-session-2026-08.md). Don't panic on
                            // absence: a `bkpt`-halt here would freeze USB
                            // CDC and lock pdb out. Log and bail instead so
                            // jvm_task and PDB stay alive.
                            let Some(jvm) = crate::boot::shared_jvm() else {
                                crate::pd_error!(
                                    "Thread.start: no shared class set for {}",
                                    class_name
                                );
                                return;
                            };
                            let _spawn_root = SpawnedRootGuard(runnable_obj_idx);
                            let heap = crate::boot::shared_heap();
                            let mut handler = super::PicodroidNativeHandler::new();
                            // Cross-executor GC root visibility; drops with
                            // the child on every exit path, error included.
                            let _handler_roots = super::HandlerRootGuard::new(&handler);
                            if let Err(e) = jvm.invoke_instance(
                                class_name,
                                "run",
                                runnable_obj_idx,
                                heap,
                                &mut handler,
                            ) {
                                crate::pd_error!(
                                    "Thread.start: child-task {}.run() failed: {}",
                                    class_name,
                                    defmt::Display2Format(&e)
                                );
                            }
                            // A `run()` that left through a non-Java error (a
                            // debugger stop, an internal fault) skipped the
                            // `monitorexit` handlers javac emits for every
                            // `synchronized` block; anything this task still
                            // holds would block every other thread forever.
                            let leaked = crate::monitor_store::release_all_held_by_current();
                            if leaked != 0 {
                                crate::pd_error!(
                                    "Thread.start: {} exited holding {} monitor(s); released",
                                    class_name,
                                    leaked
                                );
                            }
                        }),
                    );
                    // A declined spawn means the Java thread will simply never
                    // run — which reads as starvation from the app's side, so
                    // it must never be silent.
                    if !spawned {
                        crate::pd_error!("Thread.start: task spawn failed for {}", class_name);
                        // The closure was dropped unrun; release the root.
                        spawned_remove(runnable_obj_idx);
                    }
                }
            }
            Some(Ok(None))
        }
        _ => None,
    }
}
