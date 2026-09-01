# Quality Roadmap

Improvement ideas from the 2026-06 stability/testing/automation audit, deferred for later.
Each entry: what, why, and the tradeoff to weigh before starting. Ordered by value-per-effort
within each theme. Already landed (for context): the hil-tests.conf drift guard
(`scripts/check-hil-conf.sh`), new-vs-known failure diffing in the nightly emails, CI caching +
all-examples compile + per-push sim smoke, runtime APK loading in sim, and the native-class
registry cross-check (`every_native_class_is_registered`).

**2026-08-17 — post GC-race follow-up backlog:** the picoenvmon corruption family is fixed
(`0c1326d`, see `picoenvmon-qa.md`); the open items it left behind — sim thread-parallelism
audit, `sb_buf` cross-thread aliasing, memmon's child-GC blind spot, serve-loop latency,
per-child class-metadata sharing, GC-pacing measurements, and two sensor-hardware tickets —
are recorded in detail in **`followups-2026-08.md`**. The re-run of the long soak plus the
PEM-3 prereserve retune has its own runbook: `picoenvmon-soak-handover-2026-08.md`.

## Regression automation

### Handle sanitizer + GC-stress variant in the nightly sim run

Enable `PICODROID_HANDLE_SANITIZER=1` in `scripts/sim-run.sh` (the per-push CI smoke job
already sets it), and add a variant pass with `gc_alloc_threshold` forced low (~16) for the
UI-heavy rows. Use-after-free via stale handles and GC-rooting sweeps are the two most frequent
serious bug classes in this repo's history (4 GC-rooting fixes, 3 UAF fixes); both are
probabilistic, and the sanitizer + forced-frequent GC make them near-deterministic.
**Tradeoff:** GC-stress rows run slower (subset only), and the sanitizer may surface latent
stale lookups that have to be triaged before the run can gate.

### Pin nightly runs to a clean worktree

The 3 AM / 4 AM cron runs execute whatever is in the working tree — uncommitted edits included;
the SHA in the report is only a label. Run them from a dedicated `git worktree` pinned to
`origin/main`, or at minimum stamp the email with a `DIRTY` flag. **Tradeoff:** a worktree needs
its own `build/` and first-build time; the flag-only variant is free but only labels the
ambiguity instead of removing it.

### Binary-size regression report

Capture `print_memory_usage` (`scripts/lib.sh`) per board into a checked-in baseline during CI
or the nightly run; warn (don't fail) on >2–3% flash/static-RAM growth. Every KB of bloat
directly shrinks the JVM heap budget for picoenvmon on RP2350. (The old "~30 KB short" figure
predates the 2026-08-16 measurements — the measured headroom is 20,484 B static on the W board,
and the 2026-08-17 session recovered ~14 KB of class-metadata duplication plus ~8.5 KB of
byte[] payload on top; see docs/perf-memory-handover-2026-08.md §5 and docs/mem-session-2026-08.md.)
**Tradeoff:** legitimate feature growth trips thresholds — keep it report-only to avoid
baseline-update fatigue.

*2026-08-11 (`2eb9b19`):* premise partly overtaken — the build report now measures flash
against `LENGTH(FLASH)` of the program region (99% real usage was previously reported as
43% of chip total), so the number warns before a link failure. The checked-in baseline /
trend tracking remains open.

Stage 2 — **hard budget gate**: once the report has burned in, make `print_memory_usage`
fail the build (non-zero exit) when flash or static RAM exceeds a per-board percentage of
the region (mcu toml `flash_kb` / the linker script's program region). The RP2040 896 K
ceiling becomes a CI wall instead of a printed number — today an overflow only surfaces as
a link error with no margin trend (the rp2350+mem-diag pre-commit build already prints its
cost; this would enforce it). **Tradeoff:** the same baseline-fatigue risk as above but
sharper — gate on region percentage, not on deltas, so only genuine ceiling risk fails.

### Nightly strict-sentinel memory soak

`scripts/test-memdiag.sh` runs 30 s soaks per push; add a nightly 300 s+ variant (churny
rows: `picoenvmon` Live, `animdemo`, `term`-category apps) under
`PICODROID_MEMDIAG_STRICT=1` so slow leaks — under ~1 KB/min stays below the 4 KB/8-window
trip at the default cadence — go red overnight instead of surfacing as a field OOM.
Complements the GC-stress nightly variant above (that one forces collection frequency;
this one watches the post-GC floor). **Tradeoff:** nightly wall-time; sentinel
false-positives on apps with legitimate slow warm-up need per-row window/threshold tuning
via `PICODROID_MEMDIAG_WINDOW_MS`.

### HIL board/app coverage *(deferred)*

`scripts/hil-run.sh` hardcodes `BOARD="testbench_rp2350"`. Add a `--board` flag; add picoenvmon
rows once its heap budget lands; longer-term, pdb-injected key events so `keydemo`-class tests
stop being skipped on hardware. **Tradeoff:** more nightly HIL wall-time (already ~1 h —
per-board runs may need alternating nights); key injection adds a debug-only code path to
maintain.

*2026-08-11 (`2eb9b19`):* the script is now chip-agnostic — `resolve_board` sets
`PROBE_CHIP` from the board's MCU, so it can drive `testbench_rp2040` — but
`BOARD=` is still hardcoded; the `--board` flag remains open. Also wanted here:
networking rows (netdemo, ideally http_get) — that half is NET-7 in
`docs/networking-followups-2026-08.md`.

*2026-08-28 (`40411ec`):* **the `--board` flag landed** (`hil-run.sh:47`, help at
`:63`) and is used in anger — the 2026-08-30 bug bash ran
`hil-run.sh --board pico_enviro_mon_w`. This also closes the board-parameterisation
third of NET-7, which `networking-followups-2026-08.md` still lists as blocking.

*Still open, and now the sharper problem:* **no rp2040 row has ever run.** All 5,296
`hil` rows in `bench/parity/history.csv` are `testbench_rp2350` — there is no rp2040
on the probe. That is the board sitting at 97 % flash, so it is simultaneously the
most likely to break and the only one never tested on hardware. It is also the last
open item on the Kotlin roadmap (AMENDMENT 13).

*2026-08-31 (concurrency parity, `a34a639`):* `threadparity` and `jucdemo` are now
`term` rows in `hil-tests.conf` and passed 2/2 on the RP2350 board in both shrink
modes — but that run predates the merge of main into the branch, so a post-merge
re-run is still owed (low risk: the merge added a Kotlin app and docs, no framework
code). The rp2040 gap above bites harder now: `testbench_rp2040` excludes the whole
`j.u.c.` class set via `framework_class_excludes` to stay under its flash gate, so
`jucdemo` cannot run there **by construction** — the exclusion itself is what wants
testing (an app that touches an excluded class must fail to load cleanly, not
mysteriously), and no rp2040 hardware row exists to test it.

## Test coverage

### Method-level native registry cross-check (stage 2) — **LANDED 2026-07-26**

The landed check was class-level; stage 2 extends it to methods. Each dispatch handler's
`(class, method, descriptor)` triples are declared as const data in
`picodroid-core/src/native_handler/method_tables.rs` (plus `BUILTIN_SDK_HANDLED` in
`jvm/src/native/mod.rs`) and diffed against the SDK's 308 `ACC_NATIVE` methods in both
directions, closing the silent-NoSuchMethod surface. It found one live instance on the
first run (`NotificationManager.notify`/`cancel`).

The tables are declared *parallel* to the `match` arms rather than generated from them, so
the mechanical refactor the tradeoff warned about was avoided; the duplicate-row and
both-direction assertions are what keep the two in step. Generating the arms from the same
list (the X-macro phase in `docs/designs/method-level-native-registry.md`) remains open and
would make drift structurally impossible rather than test-enforced.

### Scripted UI scenario tests via the control FIFO

A scenario runner feeding `PICODROID_SIM_CTRL_FIFO` button sequences synchronized on log tokens,
encoding the lifecycle invariants from past bugs: "open dialog → push Activity → dialog gone →
BACK dismisses the *new* Activity" (bcb22ba, f15d280); "BACK mid-animation → no hang" (09808a3);
"rapid double-tap → one startActivity" (cf23713). Essentially automates the
`docs/picoenvmon-qa.md` walkthrough. **Tradeoff:** the highest false-positive risk on this page —
sync on log tokens, never sleeps, and keep it to ~5 invariant scenarios, not coverage.

### Lifecycle state-machine and store unit tests

(a) `picodroid-core/src/lifecycle.rs` is a 10-commit churn hotspot with two High-severity
historical fixes and no direct tests — extract the push/pop/dialog-stack state machine behind a
small trait (no LVGL) and unit-test its invariants. (b) Direct tests for
`jvm/src/native/{hashmap,hashset,string_builder}.rs` and the `object_heap` list/map stores
(resize, collisions, slot reuse) — currently tested only behaviorally. **Tradeoff:** (a)
refactors the very file being protected; land the sim scenario tests first as a net.

### Kotlin concurrency conformance under contention

Opened 2026-08-31 alongside the Thread-parity merge. `examples/langsuite_kt`'s `SyncDemo.kt`
covers `synchronized(lock) {}`, `@Synchronized` and `@Volatile` thoroughly but **entirely on one
thread**: nesting, early return, exception unwind, lambdas, loops. Nothing there would have
noticed that `ACC_SYNCHRONIZED` methods took no monitor at all until WP2 — the Java side caught
it only because `threadparity` increments a shared counter from two threads and checks the total.

Wanted: a Kotlin counterpart that spawns `picodroid.concurrent.Thread` from Kotlin, contends a
`@Synchronized` method and a `synchronized(this)` block hard enough that a missing monitor loses
increments, and exercises `wait`/`notify` through Kotlin's `(this as Object).wait()` idiom (Kotlin
has no `Object` supertype syntax, which is itself worth pinning as conformance). Note that
`@Volatile` currently compiles to a field the interpreter treats as ordinary — see *Cross-thread
field visibility has no `volatile` and no fences* under Long-term stability; a Kotlin test must
not be written in a way that silently depends on volatile semantics. **Tradeoff:** contention
tests are the flaky kind — bound the iteration counts so a slow board does not time out, and
assert on totals rather than interleavings.

### Grow langsuite-style conformance suites (not host JUnit)

Extend `examples/langsuite` / `examples/bytecodecoverage` per SDK area (collections edge cases,
String.format grammar, boxing, exceptions). Host-JVM JUnit would test OpenJDK's stdlib, not
pico-jvm's reimplementation — the only JVM whose semantics matter runs these suites already.
**Tradeoff:** log-token asserts are coarser than JUnit; each suite adds nightly wall time.

## Host-dev velocity

### FreeRTOS-native mailbox for the sensor sampler

`picodroid-core/src/hardware/sensors/mailbox.rs` hands sampler readings to the JVM task through a hand-rolled seqlock
(atomic load/store only — shared verbatim by device, sim, and host tests). FreeRTOS's
purpose-built mailbox — a length-1 queue used via `xQueueOverwrite`/`xQueuePeek` — would
replace the fence reasoning with a kernel primitive, but `freertos-rust-pd` 0.2.3 wraps
neither call (its `Queue<T>` has only send/receive/len) and its `shim.c` FFI layer needs
patching too, i.e. forking the crates.io dependency. The `read_env()`/`publish_env()` API
boundary already isolates the swap; nothing else moves. **Tradeoff:** fork maintenance plus a
`std` sim backing split (two mailbox implementations) vs. removing hand-rolled memory-ordering
code; revisit if the fork gets vendored for other reasons.

### Thread support in sim — DONE 2026-07-28

Resolved, and not by the std::thread route sketched here: the simulator now compiles the real
FreeRTOS kernel (POSIX port) and runs `Thread.start()` as a real task
(`docs/designs/freertos-host-sim.md`, parity-audit M7/THR-01). The tradeoff this entry worried
about — host threads being truly concurrent where the device is cooperative — is answered by
construction, since the scheduler *is* the device's and runs one task at a time. threaddemo's
conf row now asserts its workers' output rather than just "Starting threads".

What remains hardware-only is core count: the POSIX port is single-core where the chip is
dual-core, so genuinely parallel races still need a board.

### Framebuffer screenshot dump

A control-FIFO command (`screenshot /path.png`) or `PICODROID_SIM_SCREENSHOT_DIR` env dumping
the minifb buffer; optionally 2–3 coarse checks in sim-run ("not blank after boot", probe
pixels). A blank-screen regression passes every log-token test today. **Tradeoff:** skip
golden-image diffing — every LVGL/theme/font bump would invalidate all baselines; coarse checks
only.

### Scriptable sensor/peripheral injection in sim

Extend the control FIFO (or a timestamped-CSV env var) to inject ADC readings and I2C register
values over time, replacing the constant-only BME688 / 1.65 V stubs in `hal/sim/`. picoenvmon's
threshold/event logic is untestable in sim today, and the GC-starvation OOM class (74a7b24)
needs sustained sensor-event streams to reproduce. **Tradeoff:** keep the format dumb
(timestamped value list); injected values arrive with sim timing, useless for driver timing.

## Readability / maintenance

### Module docs for load-bearing invariants

`//!` docs on each `jvm/src/interpreter/ops_*.rs` (opcode range and role), the
BUILTIN_DISPATCH ↔ BUILTIN_CLASS_NAMES two-table invariant in `jvm/src/native/mod.rs` (naming
the test that enforces it), `picodroid-core/src/lib.rs`'s multi-family role, and an
ARCHITECTURE.md note on `picodroid-core/src/drivers/` vs `platforms/rp` drivers. **Tradeoff:**
doc rot — document test-enforced invariants and name the test, not narrative.

### Encapsulate the LVGL event-registry statics

`picodroid-core/src/graphics/lvgl/events.rs` holds ~46 unsafe blocks of raw
`static mut` arrays; the phantom-BACK boot bug (de5fd11, uninitialized `KEY_PRESSED_MASK`)
lived exactly in this pattern. Wrap behind one checked-index accessor with a single documented
unsafe core. **Tradeoff:** churn in a regression-critical file — land integration coverage
first; mind ISR-context accesses.

### Encode review checklists as checks

For the churn hotspots (native_handler/mod.rs, lvgl_ffi.rs, lvgl_backend.rs, lifecycle.rs,
object_heap/mod.rs), turn recurring review questions into tests/lints (the registry cross-check
and conf drift guard are the pattern); keep only what can't be automated as a short
CONTRIBUTING checklist.

## Memory-diagnostics follow-ups

(The `mem-diag` feature — monitor, growth sentinel, offensive checks, histogram — landed
2026-07; docs/memory-diagnostics.md. These extend it.)

### StackOverflow-as-OOM per-window counter

Count `Err(JvmError::StackOverflow)` returns (the JVM's catchable OOM stand-in, pervasive
in native helpers) per `[memmon]` window. An OOM-retry storm — allocation failing, GC
relieving, failing again — is a churn symptom the live-bytes floor cannot see (the floor
stays flat while the allocator thrashes at the ceiling). One plain counter field bumped at
the error-construction sites, drained per window like `alloc_total`. **Tradeoff:** the
error is used for genuine stack-depth failures too; either split the variants (wide
mechanical rename) or accept the conflation and document it.

### Per-task stack-watermark trending in memmon

`CMD_SYSMON` already reads per-task stack high-water via `uxTaskGetSystemState`
(`pdb/sysmon.rs`); fold the minimum watermark into the periodic device `memmon:` line so a
slowly-deepening stack is caught alongside heap growth (FreeRTOS overflow method 2 only
fires after the fact). **Tradeoff:** `uxTaskGetSystemState` suspends the scheduler
briefly every window — keep it to every Nth window or device-idle windows.

### Device per-class allocation histogram

The sim-only histogram answers "who churns" with class names; on device it would need a
`mem-diag-histo` sub-feature (RP2350-only — RP2040 has no flash headroom) dumping
class-table indices + counts over an extended CMD_SYSMON, with the host `pdb` tool
resolving names from the papk. **Tradeoff:** protocol surface + a per-alloc branch and
`4 B × class_count` RAM on device; the sim histogram covers most hunts since the JVM is
execution-identical (parity P1) — only sensor/HW-driven allocation patterns differ.

## Long-term stability

### GC root registration that can't be forgotten — DONE 2026-07-26

Replace "remember to edit `gc_visit_roots` when adding a native listener map" with a central
root-provider registry: each native-side map/singleton holding JVM refs registers a visitor at
construction; `gc_visit_roots` iterates the registry. GC-rooting misses are the most frequent
serious bug class in the history (a59dc53 Display singleton, d3e052d VIEW_KEY_MAP, b9194cb
touch/swipe/click/dialog maps). **Tradeoff:** fixed-capacity registry boilerplate in no_std, a
small GC-walk overhead, and the registry itself is new unsafe-adjacent machinery — pair with
the GC-stress nightly mode as the detection net while it lands.

Delivered as audit P2-17 (`23fa075`), pulled forward as a shared-core-extraction enabler:
native maps/singletons register root providers, `gc_visit_roots` iterates the registry, and
both crates carry a source-scanning completeness guard so an unregistered JVM-ref-holding
module fails the tests rather than silently losing roots.

### Extend the LVGL header-parse drift guard — DONE 2026-07-25

Landed (audit P1-7): guards now cover `LV_KEY_*`, `LV_STATE_*`, `LV_PART_*`,
`LV_OBJ_FLAG_*`, `LV_COLOR_FORMAT_*`, `LV_DIR_*`, `LV_FLEX_*`,
`LV_IMAGE_ALIGN_*` (implicit-ordinal, underscore-member aware),
`LV_BUTTONMATRIX_*`, and the `#define` constants (`LV_IMAGE_HEADER_MAGIC`,
`LV_RADIUS_CIRCLE`, `LV_BUTTONMATRIX_BUTTON_NONE`), plus the previously
unguarded `LV_EVENT_FOCUSED/DEFOCUSED/DELETE` rows and a mirrored RGB565
guard in papk-pack (which bakes that byte into every image asset).
Deliberate exemptions (alias/composite values and trivially-stable one-off
families) are documented in the tests-module comment in
`picodroid-core/src/lvgl_ffi.rs`. Note: the original list here named
`LV_ALIGN_*`, but no such Rust constants exist — nothing to guard.

### Scheduling fairness: a compute-bound thread holds its core until it blocks

Deferred out of the concurrency-parity work as WP3c (2026-08-31); by design, not by oversight.
`configUSE_TIME_SLICING 0` plus one JVM priority tier is exactly what makes the lock-free shared
heap safe — a running JVM task keeps the core until it blocks — so equal-priority threads do not
round-robin. A thread that loops without allocating, sleeping or doing I/O starves its siblings
until it exits. `Thread.yield()` is the escape hatch and works today; `setPriority` cannot help
because it is advisory (parity-audit THR-06).

The designed-but-unbuilt fix: a `NativeMethodHandler::safepoint()` (default no-op) that the
production handler implements as `rtos::task_yield()`, called from the interpreter every ~4096
instructions from a counter beside `insn_count`. It is safe precisely because an opcode boundary
outside any `AtomicSection` is the "blocking yield point" the heap model already assumes.

**Measure before keeping it.** Gate on the perf harness against `benchmark` (±4 % noise floor per
`docs/perf-memory-handover-2026-08.md`) and keep only if TOTAL moves under 2 %. **Tradeoff:** this
buys fairness nothing in-tree currently needs — no shipped app has a compute-bound background
thread — at a tax on the hottest loop in the system. The honest trigger is an app that actually
starves, not the tidiness of having it.

### Cross-thread field visibility has no `volatile` and no fences

Opened 2026-08-31 by the concurrency-parity work (merged `a34a639`). Java threads are real
FreeRTOS tasks sharing one heap, but the interpreter has **no `volatile` semantics and emits no
barriers**: `getfield`/`putfield` are plain loads and stores whatever the field's ACC flags say,
and `jvm/src/` has no fence of any kind. Today that is *correct* rather than lucky, and only
because of two properties that are themselves load-bearing elsewhere:

- every JVM-interpreting task is pinned to core 0, so there is one cache and one store buffer;
- `configUSE_TIME_SLICING 0` plus a single JVM priority tier (`PRIORITY_JVM_NORM`, WP3) means a
  task holds the core until it blocks, so a switch is always a full context save.

Both fall the moment a second core interprets — which is exactly what parity-audit **THR-04 / X1**
asks about, and RP2350 already runs real SMP with `configRUN_MULTIPLE_PRIORITIES=1`. So this
entry is a dependent of X1, not an independent one: if X1 concludes the pinning invariant is
unenforced, `volatile` fields stop being a documentation gap and become a correctness bug.

Cheapest honest step, in order: (1) a source-scanning guard that a JVM-adjacent task cannot be
spawned unpinned, so the invariant is checked rather than remembered; (2) record in
ARCHITECTURE.md that `volatile` parses and is ignored; (3) only then consider `dmb` on
`volatile` accesses — thumbv6m has the instruction but no atomic RMW, so anything past plain
load/store still needs `AtomicSection`. **Tradeoff:** step 3 taxes the hot `getfield`/`putfield`
path for a guarantee nothing in-tree currently needs; do not pay it before X1 reports.

**2026-08-31 — X1 reported (parity-audit THR-04).** Pinning is now enforced, not remembered:
`task_affinity::spawn` (`platforms/rp/src/task_affinity.rs`) is the only way the RP family creates a
task, names the core, and suspends the scheduler around create+pin — closing a real ~1 µs window in
which every `Thread.start` child began on core 1 before freertos-rust's post-hoc `vTaskCoreAffinitySet`
evicted it — and a source scan under `scripts/test.sh` rejects any other spawn. (Pinning by kernel
default instead was tried and reverted on HIL: it pins the idle-task reaper and `threadparity` OOMs.)
Step (1) is done and this entry is a documentation gap; steps (2) and (3) stand as written.

### `IO_IRQ_BANK0` runs on both cores and services the button queue from core 1

Found 2026-08-31 by the THR-04 / X1 trace — the one genuine cross-core race it turned up, and it is
outside the JVM. On `pico_enviro_mon_w` the vector is unmasked twice: on core 0 for the buttons
(`hal/rp/gpio.rs` `init_gpio_irq`, PROC0 routing) and on core 1 for the cyw43 host-wake line
(`gpio::hostwake::init`, PROC1 routing, called from the cyw43 task). Both cores share one RAM vector
table, and the handler body is not core-aware: after the host-wake block it unconditionally reads
`proc0_ints`, calls `enqueue_gpio_event` and clears `INTR`. So a host-wake interrupt on core 1 — one
per received frame — also services core 0's button path, and `enqueue_gpio_event`'s read-modify-write
of `GPIO_QUEUE` / `GPIO_QUEUE_HEAD` / `GPIO_DROPPED` (plain `static mut`s) races core 0's own ISR, the
UI task's `drain_gpio_event` and the PDB task's `inject`. Symmetrically, core 0's handler executes the
host-wake block and RMWs `proc1_inte`, racing `picodroid_cyw43_hostwake_rearm` on core 1 — a lost
re-arm degrades cyw43 RX to the 1000 ms poll fallback.

Fix: branch on `sio_hw->cpuid` at the top of the handler — core 1 runs only the host-wake block,
core 0 only the button loop (each core's `procN_ints` is already the right register for it). A few
lines, but HIL-only to validate, so it was not folded into X1's change: it needs a `pico_enviro_mon_w`
on the probe with button presses during traffic. **Tradeoff:** until then the race is a duplicated or
lost button event coincident with a received frame, and a rare host-wake re-arm loss; neither reaches
the JVM heap.

Two smaller residues from the same trace, both narrow, both recorded rather than fixed: `RESETS.RESET`
is RMW'd non-atomically from core 1 (cyw43 init, `pio_spi.rs`) and core 0 (`ensure_io_unreset` on any
Java `Gpio` call, `gpio.rs` / `dma.rs`) — RP2350's atomic-alias addresses would close it; and the
FreeRTOS+TCP `IP-task` (2 KB stack + TCB on network boards) is absent from `boot_budget::BOOT_TASKS`,
so the simulator's boot charge is short by that much on those boards (parity-audit M4).

### Simulator leaks a pthread and TCB per finished Java thread

Opened 2026-08-31; the mechanism is written up in full in
`docs/designs/freertos-host-sim.md` § A5b. The POSIX port ends a task with `pthread_exit()`, a
forced unwind that has to cross `freertos-rust`'s `extern "C"` (therefore `nounwind`) spawn
trampoline and calls `abort()` when it does. `park_finished_task` sidesteps that by suspending
finished tasks forever: the *model* charge is released first, so heap-parity measurements stay
exact, but one suspended pthread and TCB accumulate per finished Java thread.

This was invisible until the parity work — every task in the old topology looped forever, so no
Java `run()` ever returned. It is now reachable from ordinary app code: `threadparity` alone
churns 40 start/join cycles per run, and thread-churn soaks are the obvious next sim test.
Hundreds of threads per run are fine; tens of thousands would exhaust host threads.

The real fix is a `freertos-rust-pd` point release declaring `thread_start` as
`extern "C-unwind"` — no Rust destructors are live in that frame by then, so the unwind is
clean — which needs a published fork release, hence the deferral. **Tradeoff:** until then a
long-running sim soak that spawns threads in a loop will die host-side for a reason that has
nothing to do with the firmware under test; budget a bounded thread count in soak apps, and
suspect this first when a sim soak dies at a suspiciously round thread count.

### Document concurrency divergences as checked invariants

An ARCHITECTURE.md section listing what sim deliberately cannot catch — dual-core visibility
(cyw43 on core 1 as of `ac4bd74`; pdb runs on core 0), single-core safety assumptions around `ACTIVE_APK`, no-op `delay_ns`,
no ISR preemption — plus cheap hardware-side `debug_assert!` core-affinity checks where the
assumptions are load-bearing, naming HIL as the owning test layer per item. **Tradeoff:**
documentation is not detection; this consciously accepts the class as HIL-only until JVM
threading expands.

## Networking follow-ups

Carried over from `docs/networking-followups-2026-08.md` after the 2026-08-15
implementation session closed most of that backlog. These two remain open by
choice — each is an experiment with a real cost and an unproven payoff, not a
known bug with a known fix. The NET-* IDs there stay canonical.

### Silence the two boot-time iovar rejections (NET-1)

At every WiFi bring-up the firmware rejects two of `cyw43_ll_bus_init`'s
config commands (`apsta`, `ampdu_rx_factor`) with BCME -5 (NOTDOWN), logging
two warning lines over RTT. Purely cosmetic today: `apsta` only matters for
concurrent AP+STA mode and `ampdu_rx_factor` is a throughput tunable; joins,
DHCP, TCP, and HTTP are unaffected. The experiment is to reorder the boot
sequence in the vendored fork — issue an explicit `WLC_DOWN` before the two
iovars (BCME -5 means the WL core claims to be up when they arrive), or move
them ahead of the 150 ms post-boot settle — and check whether the -5 lines
disappear without regressing join latency. Comparing against a pico-sdk boot
on the same firmware blob would show whether stock Pico W setups silently hit
this too (upstream discards firmware error statuses; only our fork logs
them). **Tradeoff:** each attempt is a fork edit + rebuild + reflash + RTT
soak for a log-cosmetic win, and a botched reorder can break the regulatory/
country setup in ways that only show up as NONET join failures — validate
join → lease on hardware after every variant.

### Measure, then widen, the 256-byte socket I/O chunking (NET-9 leftover)

Socket send/recv and the HTTP streams copy data between Java arrays and the
network stack through fixed 256-byte stack buffers (`BUF_SIZE` /
`IO_CHUNK` in `picodroid-core/src/net/`), so a 4 KB read costs 16
native-call round-trips. Correctness is fine; this is the last known
throughput bottleneck now that the gSPI bus runs at 37.5 MHz (NET-4). The
buffers live on the JVM task's stack, and stack budgets on-device are tight
and deliberately accounted (`boot_budget.rs`) — so the work is: benchmark
real socket/HTTP throughput on hardware first (a netdemo variant moving a
few hundred KB would do), then decide whether 512 B or 1 KB chunks pay for
their stack (or whether the buffers should move off-stack instead).
**Tradeoff:** raising the buffers without the measurement risks a
hardware-only JVM-task stack overflow to speed up a path nobody has shown to
be slow; the measurement itself needs a HW session with a listener host.

## Framework direction

### Compose-like declarative UI layer *(deferred)*

A declarative layer over the existing retained LVGL `View` tree: `State<T>`/`MutableState<T>`
with a plain invalidation list (no MVCC snapshots), tree descriptions built by lambdas, a
positional + `key()` reconciler that re-runs affected subtree builders on state change
(virtual-DOM style — no compiler plugin, so no automatic skipping), and an applier that calls
`View` setters only for changed properties. Java-first: it does not need Kotlin; Kotlin only adds
DSL sugar (trailing lambdas, receivers). Backend gaps: `ViewGroup.addView` is append-only with no
reorder (needs `lv_obj_move_to_index` in `lvgl_ffi.rs`, or subtree rebuilds), and a post-callback
`Recomposer.flush()` dispatch site in `picodroid-core/src/lifecycle.rs`. Estimated 20–30 classes /
1.5–2.5k LOC, 35–60 KB flash (ship inside the PAPK, or E1-gate it — RP2040 has ~40 KB of program
flash left), ~9 KB retained + ~8 KB transient per rebuild for a picoenvmon-sized UI (33 views),
~10 ms per full recompose on device. Jetpack Compose proper is ruled out — see
`docs/designs/android-parity-roadmap-2026-08.md` § Not doing. Deferred until
`docs/designs/kotlin-roadmap-2026-08.md` closes, so the DSL is designed once, against the language
apps will actually use. **Tradeoff:** every screen written imperatively in the meantime is a screen
to port later; but designing the DSL before Kotlin lands would either freeze a Java-shaped API or
block on the Kotlin toolchain.
