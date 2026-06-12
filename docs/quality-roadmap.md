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

### HIL board/app coverage *(deferred)*

`scripts/hil-run.sh` hardcodes `BOARD="testbench_rp2350"`. Add a `--board` flag; add picoenvmon
rows once its heap budget lands; longer-term, pdb-injected key events so `keydemo`-class tests
stop being skipped on hardware. **Tradeoff:** more nightly HIL wall-time (already ~1 h —
per-board runs may need alternating nights); key injection adds a debug-only code path to
maintain.

## Test coverage

### Method-level native registry cross-check (stage 2)

The landed check is class-level. Stage 2: have each dispatch handler export its
`(method, descriptor)` list as const data and diff exactly against the SDK's `ACC_NATIVE`
methods, both directions. Kills the remaining silent-NoSuchMethod surface (~294 native methods).
**Tradeoff:** wide mechanical refactor of the handler modules — methods are currently matched
inside opaque `match` arms; do it behind the existing test suite.

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

### Thread support in sim

`Thread.start()` is a documented no-op in sim
(`platforms/rp/src/system/native_handler/os.rs`), so thread-spawning code paths are
hardware-only — threaddemo's workers never run under sim, and its conf row can only assert the
weak "Starting threads" pattern. Spawning a std::thread with a child JVM (mirroring the
FreeRTOS task path) would let sim exercise threading and allow stronger test assertions.
**Tradeoff:** host threads are truly concurrent while the device is single-core cooperative —
sim could surface races that can't happen on hardware (or mask ones that can); scope it to
logic coverage, not concurrency fidelity.

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

## Long-term stability

### GC root registration that can't be forgotten

Replace "remember to edit `gc_visit_roots` when adding a native listener map" with a central
root-provider registry: each native-side map/singleton holding JVM refs registers a visitor at
construction; `gc_visit_roots` iterates the registry. GC-rooting misses are the most frequent
serious bug class in the history (a59dc53 Display singleton, d3e052d VIEW_KEY_MAP, b9194cb
touch/swipe/click/dialog maps). **Tradeoff:** fixed-capacity registry boilerplate in no_std, a
small GC-walk overhead, and the registry itself is new unsafe-adjacent machinery — pair with
the GC-stress nightly mode as the detection net while it lands.

### Extend the LVGL header-parse drift guard

Copy `lv_event_constants_match_vendored_header` (`picodroid-core/src/lvgl_ffi.rs`) — which
parses the vendored header and asserts Rust constants match C ordinals — to the other
hand-written constant families: `LV_OBJ_FLAG_*`, `LV_ALIGN_*`, `LV_STATE_*`, color formats. The
event-code guard exists because LVGL 9.5.0 actually shifted enum values and caused infinite
render loops; the other families have identical exposure and no guard. **Tradeoff:**
header-parse tests are brittle to upstream formatting (the anchor self-tests mitigate); still
far cheaper than bringing bindgen/clang into the two-toolchain build.

### Document concurrency divergences as checked invariants

An ARCHITECTURE.md section listing what sim deliberately cannot catch — dual-core visibility
(cyw43/pdb on core 1), single-core safety assumptions around `ACTIVE_APK`, no-op `delay_ns`,
no ISR preemption — plus cheap hardware-side `debug_assert!` core-affinity checks where the
assumptions are load-bearing, naming HIL as the owning test layer per item. **Tradeoff:**
documentation is not detection; this consciously accepts the class as HIL-only until JVM
threading expands.
