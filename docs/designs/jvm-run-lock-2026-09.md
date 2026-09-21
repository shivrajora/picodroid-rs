# The JVM run lock — 2026-09-15

**Status: landed 2026-09-15** (`d1a09765`).

One interpreting task at a time, enforced by a kernel mutex instead of by scheduler
configuration. Module: `crates/pd-rtos/src/run_lock.rs` (until 2026-09-20 `crates/picodroid-core/src/jvm_run_lock.rs`; still reachable as `picodroid_core::jvm_run_lock`).

## The contract, and how it was kept

The shared JVM heap has no locks of its own. Every task that runs Java — the UI task, each
`Thread.start` child, the background-pool workers — shares one heap, one class set and one
string table, and the runtime's rule is that these tasks change places **only at blocking
points**: a sleep, a monitor wait, a queue the task drains, a socket, a `Thread.yield`. A task
that is mid-bytecode, or mid-native with a fresh object index in a Rust local that no frame
roots yet, must not lose the core to another Java task, because that task may collect.

Until now the rule was a property of the configuration:

- every Java-running task sits at one priority (`PRIORITY_JVM_NORM`) with
  `configUSE_TIME_SLICING = 0`, so the tick never rotates them
  (`docs/picoenvmon-qa.md`, 2026-08-17);
- the SMP kernel's equal-priority wake yield (`prvYieldForTask` uses `>=`) is covered by
  `pico_jvm::atomic_section` around every compound heap mutation and around the collector,
  so the one preemption the configuration still allows cannot land mid-resize (`0c1326d`).

## What broke it

FreeRTOS does not resume the task it interrupted. When a **higher-priority** task wakes,
runs and blocks again, `vTaskSwitchContext` selects the *next* entry of the interrupted tier's
ready list (`listGET_OWNER_OF_NEXT_ENTRY`), so two equal-priority Java tasks rotate every
time anything above them wakes — at whatever instruction the interrupted one was on. On a
device that happens on the tick timer's software-timer task, the sensor sampler and the USB
bridge; rarely enough that `threadstress` passes on hardware. The simulator gained a real
debug-bridge task on 2026-09-14 (`09d0b1bd`, `hal/sim/pdb.rs`) at `PRIORITY_RT_1`, polling
its socket every 10 ms, and the 3 AM sim run of 2026-09-15 failed `threadstress` in both
shrink modes: `InvalidReference` in `onCreate`, `OutOfMemoryError` with 230 KB free, workers
reporting corrupted rounds. Slowing the poll to once a second only delayed the failure; the
bisect landed on that commit and the mem-diag traps stayed silent — nothing overlapped, a
task simply ran Java it had no right to run yet.

## The fix

A recursive kernel mutex, created before the first task by both boots
(`rtos::freertos::install_heap_atomic_hooks`, next to the atomic-section hooks):

- **`Held::acquire()`** at the top of every Java life: `boot::run_app` (the UI task, held until
  the app is gone), the `Thread.start` child body (`native_handler/threads.rs`), and each
  background-pool work item (`bg_worker.rs`). Nested holds are free.
- **`unlocked()` / `unlocked_for(timeout)`** around every blocking wait, inside the `rtos`
  seam wrappers (`delay_ms`, `queue_recv`/`queue_send` and their `_ptr` forms, `sem_take`,
  `task_wait_notification`, `mutex_recursive_lock`) so no caller has to remember; a
  `Timeout::None` attempt keeps the lock, since giving it up would turn a poll into a switch
  point. The network facade (`hal/facade.rs`, `tcp_*`/`udp_*`/`dns_resolve`) and the blocking
  calls that bypass the seam (`SystemClock.sleep` on both targets, the embedded-hal delay on
  the device) release it by hand.
- A task the kernel rotates in while another holds the lock blocks on the mutex. It is then
  not in the ready list, so the holder is what the kernel picks next, and the rotation is
  harmless. A `Thread.yield` releases, yields and re-takes, so the waiter it hands the core to
  really runs.
- A `Forever` take that returns false — the debug bridge's app stop aborts every child's
  wait, a mutex wait included — is retried: the task still has to unwind through the heap.

What a Java thread observes is unchanged: it always ran until it blocked. The atomic-section
guards stay; they are the second line, for the wake preemption, and cost a counter.

## Rules for new code

- Never take the run lock inside an `AtomicSection` — the kernel is suspended there.
- A task that holds the lock must not block except through a wrapper that releases it.
  A new blocking primitive reached from Java (a driver wait, a new socket call) gets a
  `let _run = jvm_run_lock::unlocked();` around the wait.
- Tasks that never interpret Java never touch the lock; `unlocked()` is a no-op for them.
- With no kernel (`cargo test`) every operation is a no-op.
- **A short wait that a native owns is not always one to release around.** The rule above is
  about waits a Java thread is *meant* to sit in. A wait inside a native the UI task is already
  in the middle of — a driver transfer, say — holds Rust-side state no frame roots yet, so
  giving up the lock there hands a sibling the heap at exactly the wrong moment. The answer is
  to keep such a wait short, not to release around it. Where it went wrong is on record: an SPI
  completion wait with a 5,000 ms cap, taken directly on a `freertos_rust::Semaphore` rather
  than through the seam, froze every Java thread for five seconds at a time
  (`docs/qa-2026-09-13-followups.md` item 1, fixed in `f74c108e`). The lock did not cause that —
  before it, the same wait froze the UI loop alone — but it is what turned one task's stall into
  everyone's, so a long blocking wait inside a native is now a correctness problem as well as a
  latency one.

## Verification

Sim: `threadstress` 45 s window clean in both shrink modes (was failing at 41 ms), plus
`helloworld`, `threaddemo`, `qa_thr`, `jucdemo`, `executorstress`, `quotademo`,
`servicedemo`. Device: `filesdemo` on `testbench_rp2040`, `qa_thr` on `pico_enviro_mon_w`
(debug builds, RTT). The nightlies are the regression gate.
