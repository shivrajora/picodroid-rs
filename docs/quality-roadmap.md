# Quality Roadmap

Improvement ideas from the 2026-06 stability/testing/automation audit, deferred for later.
Each entry: what, why, and the tradeoff to weigh before starting. Ordered by value-per-effort
within each theme. Already landed (for context): the hil-tests.conf drift guard
(`scripts/check-hil-conf.sh`), new-vs-known failure diffing in the nightly emails, CI caching +
all-examples compile + per-push sim smoke, runtime APK loading in sim, and the native-class
registry cross-check (`every_native_class_is_registered`).

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
directly shrinks the JVM heap budget that is already ~30 KB short for picoenvmon on RP2350.
**Tradeoff:** legitimate feature growth trips thresholds — keep it report-only to avoid
baseline-update fatigue.

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

## Test coverage

### Method-level native registry cross-check (stage 2) — **LANDED 2026-07-26**

The landed check was class-level; stage 2 extends it to methods. Each dispatch handler's
`(class, method, descriptor)` triples are declared as const data in
`platforms/rp/src/system/native_handler/method_tables.rs` (plus `BUILTIN_SDK_HANDLED` in
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

(a) `platforms/rp/src/lifecycle.rs` is a 10-commit churn hotspot with two High-severity
historical fixes and no direct tests — extract the push/pop/dialog-stack state machine behind a
small trait (no LVGL) and unit-test its invariants. (b) Direct tests for
`jvm/src/native/{hashmap,hashset,string_builder}.rs` and the `object_heap` list/map stores
(resize, collisions, slot reuse) — currently tested only behaviorally. **Tradeoff:** (a)
refactors the very file being protected; land the sim scenario tests first as a net.

### Grow langsuite-style conformance suites (not host JUnit)

Extend `examples/langsuite` / `examples/bytecodecoverage` per SDK area (collections edge cases,
String.format grammar, boxing, exceptions). Host-JVM JUnit would test OpenJDK's stdlib, not
pico-jvm's reimplementation — the only JVM whose semantics matter runs these suites already.
**Tradeoff:** log-token asserts are coarser than JUnit; each suite adds nightly wall time.

## Host-dev velocity

### FreeRTOS-native mailbox for the sensor sampler

`sensors/mailbox.rs` hands sampler readings to the JVM task through a hand-rolled seqlock
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

`platforms/rp/src/system/picodroid/graphics/lvgl/events.rs` holds ~46 unsafe blocks of raw
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

### GC root registration that can't be forgotten

Replace "remember to edit `gc_visit_roots` when adding a native listener map" with a central
root-provider registry: each native-side map/singleton holding JVM refs registers a visitor at
construction; `gc_visit_roots` iterates the registry. GC-rooting misses are the most frequent
serious bug class in the history (a59dc53 Display singleton, d3e052d VIEW_KEY_MAP, b9194cb
touch/swipe/click/dialog maps). **Tradeoff:** fixed-capacity registry boilerplate in no_std, a
small GC-walk overhead, and the registry itself is new unsafe-adjacent machinery — pair with
the GC-stress nightly mode as the detection net while it lands.

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

### Document concurrency divergences as checked invariants

An ARCHITECTURE.md section listing what sim deliberately cannot catch — dual-core visibility
(cyw43/pdb on core 1), single-core safety assumptions around `ACTIVE_APK`, no-op `delay_ns`,
no ISR preemption — plus cheap hardware-side `debug_assert!` core-affinity checks where the
assumptions are load-bearing, naming HIL as the owning test layer per item. **Tradeoff:**
documentation is not detection; this consciously accepts the class as HIL-only until JVM
threading expands.
