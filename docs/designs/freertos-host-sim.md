# Design: Real FreeRTOS under the host simulator (parity M7 / THR-01)

> Produced 2026-07-28. Explored against source at `a2fb34d`; the amendments
> section at the bottom OVERRIDES the design body where they conflict.
> Execute from this doc; update it if reality diverges.
>
> **Status: implemented and shipped as the default, same day.** `sim` now
> *means* the real FreeRTOS kernel — there is no `sim-freertos` feature and no
> `--rtos` flag; the body below still describes both, and is superseded on
> every point the amendments touch. The host-thread backing survives only as
> the `cargo test` backing. Read the amendments first.

## 0. Motivation and current state (verified in source)

The simulator does not run FreeRTOS. It runs a *model* of it:

- `picodroid-core/src/hal/sim/rtos.rs` — std threads, condvars and mutexes
  implementing the [`Rtos`] seam: hand-rolled recursive-mutex ownership
  tracking (`:49-54`, `:180-239`), a hand-paced tick thread (`:298-334`),
  and a `Thread.start` that **refuses to run** (`:70-100`) because the
  object heap's safety rests on a single-core scheduling guarantee host
  threads do not provide.
- `picodroid-core/src/hal/sim/heap4.rs` — a Rust re-implementation of
  `heap_4.c` with forced 32-bit block arithmetic (`:14-21`), because the
  real C compiled on a 64-bit host doubles `BlockLink_t` to 16 bytes and
  the arithmetic goes device-wrong.
- `platforms/rp/src/boot_budget.rs` — the device's boot-time task stacks,
  TCBs and queues *pre-charged* as synthetic arena allocations
  (`BOOT_TASKS` `:74-125`, `precharge_boot_budget()` `:143-168`),
  calibrated against a measured RP2350 (89,472 B, ±2 KB HIL assertion).

The parity audit names the costs of the model
(`docs/parity-audit.md`): THR-01/THR-02 (S1 — `Thread.start` is a no-op,
`threaddemo` unvalidatable in sim), the §6 honest limit #2 (preemption
granularity), and M7 ("real sim threads", deferred, ask-first). This design
is M7, taken at the kernel rather than with a GIL: **run the actual FreeRTOS
scheduler in the sim process**, using the POSIX port that already ships in
the vendored kernel:

- `third_party/FreeRTOS-Kernel/portable/ThirdParty/GCC/Posix/` — `port.c`,
  `portmacro.h`, `utils/wait_for_event.{c,h}`. Tasks are pthreads; exactly
  one runs at a time (suspend/resume via per-thread event semaphores,
  `port.c:263`, `:347`, `:551-597`); the tick is a SIGALRM timer
  (`port.c:278`); `vPortEndScheduler` exists and unblocks
  `xPortStartScheduler` for a clean process exit (`port.c:294`, `:325`).

Why this is tractable now — three seams already exist and are narrow:

1. **The `Rtos` trait** (`picodroid-core/src/rtos.rs:82-126`): 15 methods,
   registered via `set_rtos!`. The device impl in `platforms/rp/src/glue.rs`
   (`:504-548` spawn, `:629-659` software-timer tick) is the template — the
   host backend becomes a near-copy of it over the same `freertos-rust` API.
2. **The kernel C build is TOML-driven** (`build_support/freertos.rs:35-92`):
   the port is a string key. A hosted leg is the same builder pointed at
   `ThirdParty/GCC/Posix` with a hosted `FreeRTOSConfig.h`.
3. **`freertos-rust-pd` is our own fork** (crates.io 0.2.3, repo
   `shivrajora/FreeRTOS-rust`) — missing wrappers (e.g. `end_scheduler`)
   are ours to add, not upstream negotiations.

## 1. Architectural principles

### 1.1 Scheduler real, kernel memory modeled

The kernel's *behavior* becomes real; its *allocations* stay modeled at
device sizes. We do **not** compile any `heap_N.c` for the host. Instead the
hosted build exports Rust shims for `pvPortMalloc` / `vPortFree` (+
`xPortGetFreeHeapSize`) that route to the host System allocator under
`allocator::bypass()` — uncounted, exactly like thread internals today
(`hal/sim/rtos.rs:104`).

Rationale: on a 64-bit host every kernel object is host-sized —
`StackType_t` is `unsigned long` (`Posix/portmacro.h:58`, 8 B vs the
device's 4), TCBs carry 64-bit pointers, and real `heap_4.c` headers double
(`heap4.rs:14-21`). Routing those into the arena would wreck the calibrated
device-byte accounting (MEM-04/M4, the ±2 KB HIL boot assertion). So:

- The **arena, cap, bypass, canaries and `heap4.rs` stay exactly as they
  are.** `CappedAllocator` remains the `#[global_allocator]` backing.
- Device bytes for kernel objects keep entering through the **model**: the
  boot budget (stage 3 moves it from pre-charge to charge-at-real-creation)
  and `charge_thread_spawn()` (`boot_budget.rs:174-182`, stage 4 pairs it
  with a release on task exit).

Parked, explicitly: "compile real `heap_4.c` as a C oracle" only makes
sense on a 32-bit host (see the reverted arm32 lane, `c45e84a`); on 64-bit
it is a divergence, not an oracle.

### 1.2 Device topology on host

The POSIX port's one-active-task invariant only holds for code that blocks
through the kernel. A task blocking on a *std* primitive (futex) can be
woken by a holder that is not the active task, and briefly two pthreads run
user code — precisely the overlap the JVM heap must never see. Therefore in
FreeRTOS mode, cross-task coordination must go through the `Rtos` seam or
kernel objects, and the sim should **light up the device-side code paths
that already do this** instead of its std shortcuts:

- Filesystem: the device's pinned worker task + queue
  (`platforms/rp/src/fs/worker.rs:63-68`) replaces the sim's
  `OnceLock<Mutex>` synchronous path (`fs/mod.rs:160-175`).
- Sensors: `sampler.rs` already spawns `TaskKind::Sensor` through the seam
  — it becomes a real kernel task with no code change.
- Background pool: the device boot path (`boot_tasks.rs`) creates 4
  `jvm-bg` workers; sim gets the same instead of the fall-back-to-main-queue.

Host-service threads with no device analog stay **outside** the kernel:
the control-channel reader (`display.rs:641-664`) and, in stage 5, the
window pump. They already communicate through atomics and injected queues,
not std locks shared with tasks.

### 1.3 One seam, two backends, feature-selected

New cargo feature `sim-freertos` (implies `sim`). `register_sim_platform!`
grows a variant (or a sibling macro arm) whose `Rtos` methods delegate to a
new `hal/sim/rtos_freertos.rs` backed by `freertos-rust` — structurally a
copy of the device impl in `glue.rs:504-659`, minus core-affinity, plus the
model charges. The std backend and `test_platform.rs` are untouched:
**`cargo test` stays on the std backend forever** (a scheduler that owns
the process cannot host 800 independent tests); FreeRTOS mode is a property
of the sim *binary* only.

## 2. Component design

### 2.1 Hosted kernel build (`build_support/freertos.rs`)

Add `build_hosted(out, repo_root)` alongside `build()`:

- port `ThirdParty/GCC/Posix`, plus `utils/wait_for_event.c`;
- config dir `platforms/rp/mcus/host/` with a new `FreeRTOSConfig.h`:
  `configNUMBER_OF_CORES 1` (the POSIX port is single-core; see §6
  non-goals), `configTICK_RATE_HZ 1000`, `configMAX_PRIORITIES 32`,
  `configUSE_RECURSIVE_MUTEXES 1`, `configUSE_TIMERS 1` — mirroring
  `mcus/rp/FreeRTOSConfig.h` wherever meaningful. No
  `configTOTAL_HEAP_SIZE` `#error` clause: no heap file is compiled
  (`b.heap()` is skipped; the shims of §2.2 satisfy the linker);
- no `pico_shim`, no vector aliases, no linker fragments.

Called from `platforms/rp/build.rs` when `CARGO_FEATURE_SIM_FREERTOS` is
set and `!is_embedded` — the first hosted consumer of the `freertos::`
module, which today runs only inside the `is_embedded` branch
(`build.rs:63-96`).

Dependency wiring: `freertos-rust-pd` moves into an *additional* optional
declaration under `[target.'cfg(not(target_os = "none"))'.dependencies]`
in `platforms/rp/Cargo.toml`, activated by `sim-freertos`; the existing
unconditional declaration under `cfg(target_os = "none")` (`:59-66`) is
unchanged. Its build.rs compiles `shim.c` against the hosted kernel via the
`links = "freertos"` metadata — same mechanism as the device build.

### 2.2 Kernel allocation shims (new, `hal/sim/freertos_heap_shim.rs`)

```rust
#[no_mangle] extern "C" fn pvPortMalloc(size: usize) -> *mut c_void
#[no_mangle] extern "C" fn vPortFree(p: *mut c_void)
#[no_mangle] extern "C" fn xPortGetFreeHeapSize() -> usize
```

Route to `std::alloc` under `allocator::bypass()`, with a size header (or a
`HashMap` under bypass) so `vPortFree` can reconstruct the layout.
`xPortGetFreeHeapSize` reports the *modeled* arena's free bytes
(`allocator::heap4_stats()`), so anything kernel-side that asks sees device
truth. Compiled only under `sim-freertos`.

### 2.3 `main()` restructure (`platforms/rp/src/main.rs:135-172`)

Today sim `main` runs the JVM inline. Under `sim-freertos`:

```
arm() → fs/display init that must precede tasks →
create "jvm" task (JVM_STACK_WORDS charged to the model; host pthread
  stack is the port's own) running the existing run_jvm body →
start_tasks() equivalent (§2.5) →
FreeRtosUtils::start_scheduler()   // blocks in xPortStartScheduler
```

Clean exit: the jvm task's tail calls a new `end_scheduler()` wrapper
(fork addition; `vPortEndScheduler` at `port.c:325` unblocks the main
thread), after which `main` prints the final heap stats exactly as today.
One app per process — same contract as `sim-run.sh`. The POSIX port's
restart quirks (`port.c:309-314` resets `pthread_once` state only on
macOS) make in-process scheduler restart a non-goal.

Panic policy: every task trampoline wraps its body in `catch_unwind` and
aborts on panic — unwinding across the port's `extern "C"` trampoline is
UB, and abort-on-panic is what the device does (panic-probe).

### 2.4 The FreeRTOS `Rtos` backend (`hal/sim/rtos_freertos.rs`)

Method-for-method mirror of `glue.rs`'s device impl:

| Seam method | Backing | Notes |
|---|---|---|
| `spawn` | `Task::new().name().stack_size().priority().start()` | No core affinity (1 core). `JvmChild`: **no refusal** — charge `charge_thread_spawn()`, release the charge in the exit trampoline (device reclaims via `vTaskDelete(NULL)`, `glue.rs:526-531`); register/deregister with pdb pending is device-only and stays out. |
| `queue_*` | `Queue::<u32>` | Deletes nothing host-side; handles remain leaked boxes (`RawQueue = usize`). |
| `mutex_recursive_*` | kernel recursive mutex | Deletes the owner/depth bookkeeping (`rtos.rs:49-54`) — the kernel's is the device's. |
| `sem_*` | binary semaphore | |
| `tick_timer_*` | FreeRTOS software timer | Same shape as device (`glue.rs:629-659`); deletes the hand-paced tick thread in this mode. `stop` deletes the timer (no thread to join). |
| `delay_ms` | `vTaskDelay` | Tick-quantized like the device, replacing `std::thread::sleep`. |

The `PICODROID_PARITY_STRICT` refusal remains in the **std** backend only;
under `sim-freertos` it is satisfied, not bypassed — `Thread.start` runs.

### 2.5 Boot topology and the budget (stage 3)

`precharge_boot_budget()` is replaced in this mode by *real creation in the
same order*: a hosted `start_tasks()` that walks the same spawn sites the
device does (fs worker, sensor task on sensor boards, background pool
workers, jvm task; pdb/cyw43 have no sim endpoint — their `BOOT_TASKS`
entries stay as synthetic charges so the arena figure still matches the
device). Each real spawn charges the model (`stack_words × 4 + TCB_EST`)
and bypasses its host-side cost. The banner and the ±2 KB HIL assertion
figures must come out unchanged — that is the stage gate.

`Tmr Svc`/`IDLE` are created by the kernel itself; their entries also stay
synthetic charges (their host allocations ride the §2.2 bypass shims).

### 2.6 Windowed mode (stage 5)

`xPortStartScheduler` owns the main thread, and macOS requires UI on the
main thread — so the window moves *out* of the dispatcher: `main` (before
`start_scheduler` on Linux, or restructured with the scheduler on a
secondary thread where the port allows) — simplest portable shape: the
window pump becomes a non-kernel thread looping `update_with_buffer` on
the existing `static FRAMEBUF` + input sampling, communicating through the
already-atomic touch-override/GPIO-inject paths (`display.rs:72-76`,
`gpio.rs:96`). Until stage 5, `sim-freertos` forces headless the way
`sim-run.sh` already runs.

## 3. What this deletes or resolves (at default-flip time)

- THR-01/THR-02 (S1): `Thread.start` real; `threaddemo` un-SKIPped.
- M7 closed without inventing a GIL; honest-limit #2 narrows from "no
  preemption model" to "FreeRTOS-tick granularity, single-core".
- `rtos.rs` std backend: recursive-mutex bookkeeping (`:49-54, :172-239`),
  tick thread (`:298-357`), parity-strict refusal (`:80-100`) — all
  superseded in the FreeRTOS lane; deleted entirely if/when the std lane
  itself is retired (not proposed here).
- `precharge_boot_budget()` becomes real creation + charges (§2.5).
- `charge_thread_spawn`'s deliberate leak (`boot_budget.rs:174-182`) gains
  its missing release-on-exit.

## 4. Feature and flag surface

- Cargo: `picodroid-core/sim-freertos` (implies `sim`),
  `platforms/rp/sim-freertos` forwarding it — same staging pattern as
  `handle-table-32` (`platforms/rp/Cargo.toml:39-44`).
- Scripts: `sim.sh --rtos <std|freertos>` (default `std`) appending the
  feature; `sim-run.sh --rtos` lane with tags `app[mode/frtos]` (opaque to
  `hil-email.py`, like the mode tags).
- `pre-commit`: one added clippy leg
  (`--features sim,board-testbench-rp2350,sim-freertos`) so the staged
  backend cannot rot — the `handle-table-32` precedent
  (`scripts/pre-commit:146-174`). No test-matrix growth until stage 6.
- `cargo test` / `test.sh`: untouched, std backend (§1.3).

## 5. Stages

| # | Stage | Contents | Gate |
|---|---|---|---|
| 0 | This doc | — | — |
| 1 | Hosted kernel | `build_hosted()`, `mcus/host/FreeRTOSConfig.h`, §2.2 shims, dep wiring, `main()` restructure, `end_scheduler` fork wrapper | `--rtos freertos` boots helloworld + benchmark headless; `--rtos std` (and feature-off builds) byte-identical to today; firmware untouched |
| 2 | Backend swap | `rtos_freertos.rs` full seam (§2.4), software-timer tick | smoke subset (helloworld, gcstress, langsuite, callbacktest, executordemo) green under `--rtos freertos`; `parity:` counters equal between std and frtos lanes |
| 3 | Device topology | fs worker task, sensor task, bg pool, real-creation boot budget (§2.5) | boot-budget banner within ±2 KB of device figure; picoenvmon-enviro smoke green |
| 4 | Thread.start (M7) | JvmChild real, charge+release, catch_unwind trampolines | threaddemo runs and passes in frtos lane; syncdemo monitor semantics validated; THR-01/THR-02 amendments in parity-audit |
| 5 | Windowed | window pump outside the kernel (§2.6); sim-remote works | interactive picoenvmon under `--rtos freertos` via sim-remote |
| 6 | Lane + flip decision | sim-run `--rtos` lane, soak, docs; then decide std-lane default | one week of green nightly frtos lane before any default change |

Estimate: stages 1-2 ≈ 4-5 focused days; 3-4 ≈ 3-4 days; 5 ≈ 2 days.
Fork (`shivrajora/FreeRTOS-rust`) changes ride alongside stage 1-2 and need
a crates.io point release (or a git dependency during development).

## 6. Risks and non-goals

Risks, with mitigations:

- **EINTR under a 1 kHz SIGALRM tick**: every blocking host syscall in a
  task can be interrupted. Rust std retries EINTR internally for I/O and
  locks; audit remaining raw calls (`libc::write` in the allocator's abort
  path is write-and-die, fine). Stage-1 gate includes a langsuite run
  specifically because it is syscall-noisy (Gradle-produced output prints).
- **std sync across task boundaries** (§1.2): transient two-threads-running
  breaks the heap invariant. Mitigation is topology (device paths), plus a
  stage-3 audit list: `fs/mod.rs:77-107` (replaced), `sampler.rs` condvar
  (replaced by its device shape), `display.rs` statics (jvm-task-only by
  contract, unchanged), OnceLock initializers (init-before-scheduler).
- **pthread stack sizing**: device word counts (e.g. 128-word idle) are
  far below host minimums; the port sizes pthread stacks itself — verify in
  stage 1 that `Task::stack_size` words only feed the *model*, and host
  stacks come from the port's policy.
- **Scheduler teardown**: one app per process; in-process restart is
  explicitly unsupported (POSIX port re-init caveats, `port.c:309-314`).
- **Tick-quantized `delay_ms`**: sleeps become ≥1 ms ticks — closer to
  device, but any sim test that depended on sub-tick sleep precision will
  shift; watch input-injection gesture tests (`input_inject.rs:31-51`) in
  the stage-2 gate.

Non-goals:

- **SMP / dual-core**: the device kernel runs `configNUMBER_OF_CORES 2`
  (`mcus/rp/FreeRTOSConfig.h`); the POSIX port is single-core. Cross-core
  interleavings (THR-04, X1) remain HIL-only. Single-core host is the
  *conservative* side for JVM heap safety.
- **Timing fidelity**: unchanged honest limit #1 — counters and behavior,
  never wall-clock.
- **arm32 revival**: orthogonal, reverted at `a2fb34d`; nothing here
  depends on pointer width.
- **Real `heap_4.c` on the host**: a 64-bit divergence, not an oracle
  (§1.1).
- **pdb/networking sim endpoints**: unchanged scope.

## Amendments

*(amendments override the body above where they conflict)*

### 2026-07-28 — stages 1-4 landed behind a staged `sim-freertos` feature

*(superseded the same day by the retirement amendment below, which removed the
feature and the flag. Kept because the corrections to the design body — A1
through A8 — still apply.)*

Implemented and verified on Linux/x86-64. What the body got right is not
repeated here; what it got wrong or under-specified is.

**A1. The hosted kernel is built by `picodroid-core/build.rs`, not
`platforms/rp/build.rs`, and by a plain `cc::Build`.**

Two forced corrections to §2.1:

- `freertos_cargo_build::Builder` cannot be told to omit the heap. Its
  `verify_paths` *requires* `portable/MemMang/<heap>.c` to exist and `compile`
  adds it unconditionally, so "`b.heap()` is skipped" is not reachable through
  that API. Compiling `heap_4.c` anyway and letting the linker prefer our Rust
  shims would work only by accident — one reference to a symbol only
  `heap_4.o` defines would pull it in and duplicate the rest. The hosted leg is
  therefore a direct `cc::Build` in the new `build_support/freertos_host.rs`:
  kernel top-level `.c`, the POSIX port, `utils/wait_for_event.c`, and the
  `freertos-rust` `shim.c`. No `heap_N.c` at all.
- The builder needs `DEP_FREERTOS_SHIM`, which Cargo exports only to the
  build script of a crate that *directly* depends on the `links = "freertos"`
  package. `picodroid-core` needs `freertos-rust` anyway for
  `rtos_freertos.rs`, so it owns both the dependency and the C build — the same
  arrangement LVGL already has, and for the same reason: the crate that calls
  into the library compiles it. `platforms/rp` gained its own optional
  host-side declaration too (Cargo accepts the same package in an
  `arm` and a `not(arm)` target table), because `sim_boot.rs` and the fs worker
  create tasks directly.

Consequently the hosted `FreeRTOSConfig.h` lives at
`picodroid-core/freertos-host/FreeRTOSConfig.h`, not `platforms/rp/mcus/host/`.
It is not MCU config: the simulator is family-neutral and lives in that crate.

**A2. Backend selection happens in `hal/sim/mod.rs`, not in the macro.**

§1.3 proposed a `register_sim_platform!` variant. A `#[cfg(feature = ...)]`
written inside a macro body is evaluated against the *expanding* crate's
features, which would have put the choice in `platforms/rp` where
`picodroid-core` cannot check it. Instead `hal/sim/mod.rs` compiles either
`rtos.rs` or `rtos_freertos.rs` and re-exports the winner as `hal::sim::rtos`;
both expose the same free functions, so the macro, `test_platform.rs` and every
caller name `rtos::` and never the choice.

**A3. The charge hook grew a return value and a partner; there is no separate
`JvmChild`-only hook.**

`register_sim_platform!`'s `charge_jvm_child_spawn = fn()` became
`charge_task_spawn = fn(&TaskSpec) -> u32` plus
`release_task_spawn = fn(&TaskSpec)`. The return value is the device-modeled
stack size in bytes, and it does double duty: it is what gets charged to the
arena *and* what sizes the real kernel task, so the two cannot drift. Stack
sizing therefore stays platform policy exactly as `rtos.rs` requires —
`boot_budget::default_stack_bytes` is now the single source for it, and the
device `Rtos` impl in `glue.rs` calls the same function.

**A4. `precharge_boot_budget` was not replaced wholesale — `BOOT_TASKS` gained
a `sim_real` flag.**

§2.5's "real creation in the same order" is right for four of the entries and
wrong for the rest, so the table says which is which. `sim_real: true` (fs,
jvm, 4× jvm-bg) are created for real and charge at creation; `sim_real: false`
(pdb, cyw43, `Tmr Svc`, `IDLE0`, `IDLE1`) stay synthetic pre-charges. The std
lane ignores the flag and pre-charges everything, unchanged.

**The sensor task is `sim_real: false`, contradicting §1.2.** The body claimed
`sampler.rs` "becomes a real kernel task with no code change". It does not: the
device sampler exists to drive real I²C parts and there are none on the host,
so the simulator keeps its own backing, which fabricates snapshots on a host
thread and publishes them through an all-atomic seqlock mailbox — one of the
host-service threads §1.2 already leaves outside the kernel. Making it a real
task would have produced a real task reading nothing.

New: `boot_budget::report_boot_budget()` asserts, at the end of boot, that the
charges sum to `modeled_boot_bytes()`. It caught a missing fs charge during
bring-up, which is the class of drift the ±2 KB HIL assertion would otherwise
have found a nightly later. Verified equal on both boards — 70,320 B
(testbench_rp2350) and 74,536 B (pico_enviro_mon).

**A5. The JVM task waits for Java threads before ending the scheduler.**

Not in the body, and stage 4 does not work without it. `run_jvm` returns as
soon as `onCreate` does; ending the scheduler there tears down children that
have not executed an instruction — the same visible outcome as the std
backing's deliberate no-op, reached by accident. `sim_boot`'s JVM task now
polls a `LIVE_JVM_CHILDREN` counter (incremented before the spawn, decremented
in the exit trampoline) exactly where the device's supervisor loop waits on
`ACTIVE_JVM_THREADS`. A 10 ms poll rather than the device's notifications: the
device must wake promptly because a flash erase is queued behind it, and here
the only thing waiting is process exit.

**A5b. A task that *returns* aborts the process, so finished tasks park.**

Not in the body, and the single sharpest thing found while implementing. The
POSIX port ends a task with `pthread_exit()` (`port.c:592-595`), which performs
a **forced unwind**. That unwind has to pass back through `freertos-rust`'s
spawn trampoline — a Rust `extern "C"` function, therefore `nounwind` — so the
unwinder hits a frame it may not cross and calls `abort()`. Deleting the task
from another task is no escape: `vPortCancelThread` uses `pthread_cancel`,
which unwinds the same frames.

The failure mode is nasty because it is invisible in the obvious test:
`threaddemo` as shipped loops forever, and every other task in the topology
(fs worker, bg workers) loops forever too, so nothing ever exercised a task
exit. The first Java `run()` that returned killed the simulator.

`park_finished_task` therefore suspends the task forever instead of letting it
end. The **model** charge is released first, so the arena — which is what
parity measures — is exactly as correct as if the task had ended. What leaks is
host-side and uncounted: one suspended pthread and TCB per finished Java
thread. Fine for hundreds of threads over a run; an app churning tens of
thousands would exhaust host threads. The real fix is a `freertos-rust-pd`
point release declaring `thread_start` as `extern "C-unwind"` (no Rust
destructors are live in that frame by then, so the unwind would be clean) —
not done here because it needs a published fork release.

**A5c. The arena lock does *not* need a scheduler-suspend guard — §1.1's "the
arena, cap, bypass, canaries and `heap4.rs` stay exactly as they are" holds
after all.**

Recorded because the wrong answer was written first and is worth not
repeating. The reasoning that led there: `CappedAllocator::arena` is a
`std::sync::Mutex` whose comment calls it "the port's stand-in for
`vTaskSuspendAll`", and under a real scheduler a task holding it can be
suspended while a *higher-priority* task blocks on it in a futex the kernel
cannot see — the kernel still believes the waiter is runnable, re-selects it
every tick, and neither moves. `Thread.setPriority` above `NORM_PRIORITY` maps
above the JVM task's own priority, so Java code can reach that shape.

Both halves of that turned out to be wrong.

*It does not happen.* The tick is a SIGALRM, and a signal interrupts
`futex_wait`. The handler runs on the waiter's own thread, calls
`xTaskIncrementTick`, and switches to the holder through `prvSwitchThread` —
which resumes it. The holder finishes, releases, and the waiter proceeds.
Preemption-by-signal is exactly what rescues the case. Measured: an app
starting three `MAX_PRIORITY` allocation-heavy Java threads against a
simultaneously-allocating JVM task completes cleanly 5 runs out of 5, all four
finishing.

*And the "fix" broke something real.* Wrapping every arena acquisition in
`vTaskSuspendAll`/`xTaskResumeAll` meant **non-kernel threads took a kernel
lock**: the control-channel reader, the sensor backing and minifb's own threads
all allocate, and `uxSchedulerSuspended` is a plain global the kernel expects
only tasks to touch. Racing it from outside tripped
`xTaskResumeAll: Assertion 'uxSchedulerSuspended != 0U' failed` and killed
every windowed run — which is how it was caught, since no headless test touches
those threads. It is reverted; `allocator.rs` is byte-identical to before this
work.

Residual risk, stated rather than mitigated: the inversion is unreachable only
because the tick keeps firing. Code that masks the tick (a FreeRTOS critical
section) *and* allocates would reintroduce it. The kernel never allocates
inside a critical section, and neither does anything here — but it is the
invariant to check if a hang ever shows up in this lane.

**A6. `end_scheduler` needed no fork change.**

§2.3 proposed adding a wrapper to `freertos-rust-pd`. Not required, and better
avoided: `vTaskStartScheduler` / `vTaskEndScheduler` are ordinary kernel
symbols, declared directly in `rtos_freertos.rs`. That also sidesteps a real
hazard — the fork types `FreeRtosUtils::start_scheduler` as `-> !`, which is
false for the POSIX port, where it returns.

**A7. `bg_worker` is no longer device-only.** Its `defmt::error!` calls became
`pd_error!` (defmt does not link in a sim binary), and its cfg gate now
includes `sim-freertos`.

**A8. Stage 5 (windowed) turns out to be free on Linux.**

§2.6 assumed the window had to move out of the dispatcher because
`xPortStartScheduler` owns the main thread. On Linux/X11 it does not: the JVM
task is an ordinary pthread and minifb is happy there. Verified on Xvfb —
`displaydemo` under `--rtos freertos` opens its window and renders the full UI
at 56 FPS, and the screenshot matches the std lane's (12,473 vs 12,502 bytes of
PNG for the same frame). No code was needed.

So stage 5 reduces to **macOS**, where the constraint §2.6 describes is real:
Cocoa requires the window on the main thread, and the scheduler has taken it.
Untested here — this session is Linux. Until someone runs it on a Mac, treat
windowed `--rtos freertos` as Linux-only. `sim-remote` should work on Linux by
the same argument but is unverified.

#### Verified

- Every sim-compatible row of `scripts/hil-tests.conf` — 17 tests — passes in
  both lanes (release, headless, handle sanitizer and parity-strict on).
  `threaddemo`, skipped under the std backing since THR-01, **passes** under
  `--rtos freertos` with T1 ticking at 500 ms and T2 at 1000 ms.
- `picoenvmon` on `board-pico-enviro-mon` boots to `Home.onCreate` in both
  lanes.
- App-visible output is byte-identical between the lanes for every test.
  `gcstress` differs only in wall-clock microseconds — its GC collection and
  freed-object counts are equal — which is honest limit #1, not a divergence.
- The std lane is unchanged: same output, and heap figures within the
  ±24-40 B run-to-run spread the *pre-change* binary already exhibits (checked
  10 runs each side; the spread is pre-existing, not introduced here).
- The `Thread.start` charge is really released. Five threads whose `run()`
  returns: the std lane leaks 5 × 16,504 B (they never ran, so the device never
  reclaimed) and ends 82 KB above baseline; the freertos lane peaks 124 KB
  higher — the five stacks are genuinely charged — and returns to within 216 B
  of baseline. Twenty at once correctly OOMs the 416 KB arena, which is what
  hardware would do.
- Windowed `displaydemo` renders identically to the std lane on Linux/Xvfb
  (A8), and three `MAX_PRIORITY` allocation-heavy Java threads plus an
  allocating JVM task complete 5 runs of 5 with no lock pathology (A5c).
- `clippy --deny=warnings` clean on `sim-freertos` for both boards, wired into
  `scripts/pre-commit` as a staged leg beside `handle-table-32`.

#### Why the std backing cannot simply be deleted

Measured, not assumed: `cargo test -p picodroid-core` against the
`sim-freertos` backing **segfaults** — 198 tests start, then SIGSEGV. The test
harness runs cases on parallel threads with no scheduler ever started, so the
kernel's task APIs dereference a current-TCB that does not exist. The same
command on `sim` does not crash. This is §1.3's claim, now with evidence: the
thing that serves `cargo test` has to be a non-kernel backing, which is exactly
what `hal/sim/rtos.rs` is. Deleting it needs a third answer for the test build,
not just a default flip.

The other two blockers to deletion are macOS windowing (A8) and simple
maturity — this lane is a day old, and two genuine defects (A5b, A5c) were
found in it by pushing on paths the suite does not cover.

### 2026-07-28 (later) — the host-thread backing is retired as a runtime

Stage 6's default-flip, taken further than a flip: `sim` now *means* the real
kernel, and `--rtos` is gone rather than defaulted. This overrides §1.3's "one
seam, two backends, feature-selected" and the whole of §4's flag surface.

What made it safe to do now rather than after a soak was learning that the two
things the design expected to block it do not:

- **Windowed works** (A8) — verified on Linux/Xvfb, which is the only platform
  this project simulates on.
- **The theoretical lock hazard is not real** (A5c) — and the "fix" for it was
  the actual bug.

What did *not* go away is the `cargo test` constraint of §1.3, now measured
rather than predicted: `cargo test` against the kernel backing segfaults after
198 tests, because the harness runs cases on threads it owns with no scheduler
started, and the kernel's task APIs dereference a current task that does not
exist. So `hal/sim/rtos.rs` survives — not as a simulator, but as the test
harness's backing, selected by `#[cfg(test)]` in `hal/sim/mod.rs` and renamed
`rtos_std` there to say so. `cfg(test)` is true only when picodroid-core is
itself the test target; a platform crate's tests compile core normally, get the
kernel backing, and are fine because they never spawn a task.

Removed: the `sim-freertos` feature (both crates), `sim.sh --rtos`,
`sim-run.sh --rtos` and its `/frtos` result tag, the staged pre-commit clippy
leg, and `sim-run.sh`'s THR-01 skip of `threaddemo`. `precharge_boot_budget`
and `charge_task_spawn` lost their two-lane branches — real creation and
charge-with-release are simply what happens now.

`PICODROID_PARITY_STRICT` is inert in the simulator as a result: the no-op it
existed to turn into a hard failure no longer exists. It still guards the
`cargo test` backing, where a spawn is still refused, so it is left in place.

#### Not done

All six stages are done, but three things are owed and one is a standing
caveat.

Owed:

1. **The `extern "C-unwind"` fork release** (A5b). Finished tasks park instead
   of ending, leaking a suspended pthread each. Needs a `freertos-rust-pd`
   point release.
2. **A thread-churn soak.** Nothing in the suite starts and finishes threads in
   a loop, which is the shape that would surface both the parked-pthread
   accumulation and any charge/release imbalance over time. This is the one
   gap that would most plausibly hide a defect today.
3. **`sim-remote`** under the kernel is unverified, though windowed
   `displaydemo` works (A8), so it is very likely fine.
4. **`BootLeaves::extra_boot_tasks`** — the one temporary hook. `sim_boot`
   moved to `picodroid-core` (residue B11) and takes family policy as three
   `fn` pointers; the third exists only to spawn the LittleFS worker and is
   deleted by residue Stage 5, which moves `fs` into core.

Standing caveat: **macOS**. The simulator now takes the main thread for the
scheduler, and Cocoa wants the window there. Untested — this project simulates
on Linux, which is why retiring the host-thread runtime was acceptable. Anyone
bringing the simulator up on a Mac should expect to do §2.6's window-pump
restructure first.
