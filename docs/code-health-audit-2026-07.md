# Code-Health Audit — 2026-07-24

Full-repo audit on four axes: **test coverage**, **modularization**, **reusability**, and
**API contracts**. Read-only — no fixes applied; the prioritized backlog at the end is the
input for follow-up sessions.

- **Baseline:** commit `7234d0b`, plus in-flight uncommitted work in the LVGL key-input path
  (`events.rs`, `key_debounce.rs`, `gpio.rs`) which was not assessed (since landed as
  `0ef2909`).
- **Method:** five parallel deep-dives (jvm, platforms/rp, shared-architecture crates, Java
  SDK + build, tools + test infra), each grounded in file:line evidence, plus independent
  verification of every declared architecture invariant (see Appendix).
- **Prior art:** `docs/quality-roadmap.md` (2026-06 stability/testing audit). This audit
  confirms several of its items are still open and does not restate its per-item tradeoffs —
  cross-references appear as *(roadmap)*.
- **Decision incorporated:** ESP32-S3 support will be **removed for now** (owner decision,
  2026-07-24 — "too complicated"). Section 7 is the removal plan; reusability findings are
  framed accordingly.

## 1. Executive summary

The codebase has a **strong spine and soft edges**. The execution core (`jvm`) is a
well-layered, heavily tested `no_std` interpreter — roughly half that crate is behavioral
tests. The repo's signature quality move — turning conventions into **executable contract
tests** (native-class registry, dispatch-site/shrink cross-checks, HAL compile-time
contract, LVGL event-code drift guard) — is genuinely excellent and should be extended, not
replaced. Risk concentrates in three places:

1. **`platforms/rp`'s UI/lifecycle layer** — the biggest subsystem (34K lines, 61% in
   `system/`), the least tested (~a third of the crate has zero unit tests), and the home of
   nearly all `unsafe` (871 of 905 occurrences repo-wide).
2. **Duplicated knowledge** — the PAPK binary format is independently re-declared in five
   places with no shared crate; four stale dead twins of `picodroid-core` modules linger in
   `platforms/rp/src/` (two silently divergent); the HAL contract was copy-pasted per
   platform instead of centralized.
3. **Gate gaps at the top of the value curve** — the shipping `pico_enviro_mon` board is
   never cross-compiled to ARM by any gate, `board-testbench-rp2350w` is compiled by nothing
   at all, three host tools get no clippy (one already ships a latent bug), and per-push CI
   runs only 5 of ~24 runnable Java behavioral suites.

### Scorecard

| Area | Modularization | Reusability | API contracts | Test coverage |
|---|---|---|---|---|
| `jvm` (24.7K ln) | B+ | B− | C+ | **A−** |
| `platforms/rp` (34.2K ln) | C+ | C | B− | **D+** |
| `picodroid-core` (2.5K ln) | C+ (grab-bag) | n/a | B (guarded tables) | C |
| Java SDK (6.2K ln) | A− (API design) | n/a | A− | C+ |
| `tools/*` (4.9K ln) | B− | C (bin-only) | C (format ×5) | C |
| Scripts / CI / test infra | B+ | n/a | n/a | C+ (completeness) |

### Top strengths (preserve these)

- `jvm` layering and `no_std` hygiene: deps are only `libm` + `compat`; zero
  `cortex-m`/`rp2*`/`std` references; only 6 `unsafe` in 24.7K lines.
- The **contract-test pattern**: `every_native_class_is_registered`
  ([class_registry.rs:227](../platforms/rp/src/system/native_handler/class_registry.rs)),
  `every_site_resolves_under_active_shrink_map`
  ([dispatch_sites.rs:146](../picodroid-core/src/dispatch_sites.rs)),
  HAL CONTRACT v1 compile-asserts ([contract.rs](../platforms/rp/src/hal/contract.rs)),
  the LVGL event-code header-parse guard
  ([lvgl_ffi.rs:812](../picodroid-core/src/lvgl_ffi.rs)), and the
  `check-hil-conf.sh` token drift guard.
- The sim/HIL harness engineering: asserted outputs + crash-marker scanning at every layer,
  dual shrink-mode runs everywhere, handle-sanitizer + parity-strict defaults in CI,
  `flock`/power-cycle/probe-recovery in the HIL runner.
- Build system: auto-discovering Gradle plugin, typed incremental tasks, Error Prone at
  default-ERROR with a single justified suppression.
- Dependency hygiene: the crate graph is a clean acyclic DAG (`compat` < `jvm` < `core` <
  platforms; tools isolated).

## 2. Repo metrics

| Crate / area | Files | Lines | Tests (live) | `unsafe` |
|---|---|---|---|---|
| `jvm` (pico-jvm) | 70 | 24,686 | 544 | 6 |
| `platforms/rp` | 203 | 34,216 | 183 (2 more in dead files) | 871 |
| `picodroid-core` | 15 | 2,548 | 19 | 14 |
| `compat` | 1 | 199 | 13 | 0 |
| `platforms/esp` (to be removed) | 40 | 1,180 | few | 14 |
| `tools/class-shrink` | 9 | 1,744 | 26 | 0 |
| `tools/pdb` | 8 | 1,915 | 35 | 0 |
| `tools/papk-pack` | 2 | 803 | 4 | 0 |
| `tools/papk-info` | 1 | 457 | **0** | 0 |
| Java SDK (`sdk/java`) | 111 | 6,160 | 0 JUnit (by design; see §3.3) | — |
| `scripts/` | 25 | ~3,480 bash + 646 py | n/a | — |

Churn hotspots (last 6 months) align with the risk map: `main.rs`, `app.rs`,
`lifecycle.rs`, `native_handler`, `jvm/interpreter`, `lvgl_ffi.rs` — i.e. the most-edited
files are in the least-tested layer (except the interpreter, which is well covered).
Repo-wide `TODO/FIXME/HACK` count: **zero** (issues are tracked in docs, not code — good
discipline).

## 3. Axis: test coverage

### 3.1 The pyramid as actually implemented

- **L0 — `cargo test` via `scripts/test.sh`**: workspace tests ×2 shrink modes (+ esp,
  until removal). Runs in pre-commit and CI. This is where the 824 live Rust tests run.
- **L1 — `scripts/pre-commit`**: formatting, markdownlint, conf drift guard, compileJava,
  clippy ×6 configs, rp2040 debug+release builds (896K flash gate), rp2350+mem-diag build,
  L0, JVM-env test.
- **L2 — per-push CI** (7 jobs): builds (rp2040/rp2350 × debug/release × ubuntu/macOS), L0,
  clippy, **all examples compiled ×2 shrink modes**, `sim-smoke` running **5 apps**
  (`helloworld gcstress langsuite callbacktest picoenvmon`) with sanitizer + parity-strict.
- **L3 — nightly sim cron**: full 24-row `hil-tests.conf` matrix ×2 shrink modes with
  asserted log tokens, enviro-board host smoke, heap-pressure soak, mem-diag strict soak.
- **L4 — nightly HIL cron**: full matrix on real `testbench_rp2350` hardware via RTT,
  install/reject/stress paths included (~1 h).

Outputs are asserted (`check_patterns` + `check_no_crash` in `scripts/lib.sh`), not
eyeballed, at every layer.

### 3.2 Rust coverage: bimodal by design, with the risk on the wrong side

Tests cluster in **pure-logic islands** deliberately refactored to be HAL-free: wire
protocols (`hal/rp/{i2c,spi,pdb_usb}/protocol.rs`, 46 tests), the activity state machine
(`native_handler/state.rs`, 18), PIO managers (32), input filters (25), and the entire
`jvm` crate (GC roots, interning, unwinding, lazy class-loading all covered — the best
subsystem in the repo, `gc/tests.rs` alone is 1,669 lines / 49 tests).

Zero-unit-test subsystems in `platforms/rp` (~11K lines, ≈32% of the crate):

| Subsystem | Lines | Notes |
|---|---|---|
| LVGL widget rendering (`graphics/lvgl/widgets/` + `graphics/widgets/` + `lvgl_backend.rs` + `view.rs`) | ~7,800 | Only reachable via sim smoke |
| Lifecycle orchestration (`lifecycle.rs` + `service_lifecycle.rs` + `app.rs`) | ~3,000 | 10-commit churn hotspot, 2 historical High-severity fixes *(roadmap)* |
| `handle_table.rs` | 190 | Highest-risk file in the repo (see §6.2); zero tests |
| `fs/` storage worker | 491 | Interface by convention |
| `pdb/` device side (input injection, IRQ park-signal, sysmon) | 800 | HIL-only |
| GC-root interop (`native_handler/mod.rs:196-235`) | — | The load-bearing anti-UAF logic; most frequent serious bug class in repo history *(roadmap)* |

`jvm` gaps worth closing even at A−: the `lib.rs` facade (`load_class`, 5× `invoke_*`,
`reset`, `prereserve`) has **zero direct tests**; monitor semantics untested in-crate;
`object_heap/{map_store,iter_store}` and `parity.rs` have no direct tests.

### 3.3 Java coverage: no JUnit — deliberately — but the gate is thin per-push

There are zero Java unit tests and no test framework. This is a **documented decision**
(*roadmap*: host-JVM JUnit would test OpenJDK's stdlib, not pico-jvm's reimplementation).
What exists instead is genuinely engineered:

- **Self-checking suites** that fail loudly: `langsuite` (14 sub-demos, isolated
  try/catch), `bytecodecoverage`, `callbacktest` (asserts a real state invariant),
  `prefs_demo`, `gcstress`/`heapstress`, `clinitdemo` — all gate on computed values and
  emit `=== ALL PASSED ===`-style tokens only when correct.
- **Token assertion + crash scan** in the harness, with `check-hil-conf.sh` failing the
  build if a conf token loses its matching Java string literal.

Honest weaknesses: per-push CI runs **5 of ~24** runnable apps (the rest wait for
nightly); **pre-commit never runs the Java behavioral suite** (compileJava only, so a
Java semantics regression isn't caught locally); several conf rows assert only that a line
printed, not its value; `threaddemo` is unverifiable in sim (`Thread.start()` is a sim
no-op *(roadmap)*); 15 of 24 rows are `hw`/`pdb`/`skip` categories that never run in sim CI.

### 3.4 Gate gaps (compile/lint lanes)

- **`pico_enviro_mon` — the shipping product board — is never cross-compiled to ARM
  firmware by any gate** (pre-commit builds rp2040/rp2350/tdeck; CI builds rp2040/rp2350;
  HIL hardcodes `testbench_rp2350` at `hil-run.sh:28`). Its `sensor-bme688`/`sensor-ltr559`
  hardware paths get no compile coverage anywhere. Known *(roadmap)* but still open.
  **RESOLVED 2026-07-25:** ARM clippy lanes added to the pre-commit board loop and CI
  linting for both this board and `testbench_rp2350w`.
- **`board-testbench-rp2350w` (cyw43 network) is compiled by nothing automated** — feature
  exists in `platforms/rp/Cargo.toml:25`, referenced by zero scripts/CI. It can rot
  silently; decide to gate it or delete it. **RESOLVED 2026-07-25: gated** (kept — Pico 2 W
  is a published board); enabling the gate required adding the missing `# Safety` docs the
  rot had already accumulated in `drivers/cyw43.rs`.
- **`papk-pack`, `papk-info`, `class-shrink` get clippy nowhere** (pre-commit clippies
  boards/sim/pdb; CI clippies `-p picodroid` only). Concrete cost already incurred: see
  §6.4.
- **The parity harness has never produced a hardware data point.** `parity-bench.sh` is
  invoked by no automation; `bench/parity/history.csv` is 7 sim-only rows from one commit.
  Its purpose (assert sim-vs-HIL counter equality) has never executed end-to-end. Wire it
  into the nightly HIL run or descope it.
- Nightly crons run the **dirty working tree** *(roadmap)*.
- Documented sim-invisible classes (`docs/parity-audit.md`): 32-bit handle dangles
  (HAL-05, S1), XIP placement/restore (APK-01), threading/cross-core visibility
  (THR-01/03). Honestly catalogued; only partially netted.

## 4. Axis: modularization

### 4.1 God-modules (three, one per layer)

- [`platforms/rp/src/lifecycle.rs`](../platforms/rp/src/lifecycle.rs) — **1,894 lines, 47
  fns, three concerns**: Application lifecycle, Activity stack, and 17+ per-widget
  `dispatch_*` functions (`:930-1872`). Every new event-emitting widget edits this file.
  The extraction seam already exists: the `DISPATCH_SITES` table it indexes. *(roadmap
  proposes the state-machine extraction; land sim scenario tests first as the net.)*
- [`jvm/src/object_heap/mod.rs`](../jvm/src/object_heap/mod.rs) — **1,195 lines, 42 pub
  fns, four unrelated jobs**: object slots/field arena, StringBuilder scratch
  (`:511-560`), exception side-tables (`:173-244`), and number→string formatting
  (`:747-795`). The last two belong beside `native/string*`, not in the heap.
- [`tools/papk-pack/src/main.rs`](../tools/papk-pack/src/main.rs) — 803-line bin with all
  serialization private; its 4 tests cover only the auxiliary class inspector, not the
  writer.

### 4.2 Dead and stale files (six)

`platforms/rp/src/main.rs` re-exports four modules from `picodroid-core`
(`main.rs:12,15,34,46`) — the local same-named files are **git-tracked but never
compiled**:

| File | State |
|---|---|
| `platforms/rp/src/dispatch_sites.rs` (144 ln) | **Divergent** — 8 sites behind core's twin (missing `SERVICE_ON_REBIND`, `ACTIVITY_ON_ACTIVITY_RESULT`, `VIEW_LONG_CLICK`, `ALERT_DIALOG_ITEM`); its 1 `#[test]` never runs. A maintainer editing it sees no effect — an active trap (bitten once already per memory). |
| `platforms/rp/src/task_priority.rs` (63 ln) | **Divergent** — missing core's `PRIORITY_SENSOR`; 1 dead `#[test]` |
| `platforms/rp/src/framework_classes.rs`, `shrink_names.rs` | Byte-identical dead copies |
| `platforms/rp/src/hal/rp/timer_alarm.rs` (82 ln) | **Orphaned real code — RESOLVED 2026-07-25: deleted.** Archaeology showed `f1c0b0d` (PDB task moved to core 0, 2026-04-12) deliberately retired the whole cross-core park design — it removed the `mod` declaration (and the `signal_park_from_isr` symbol the orphan calls) but forgot the file. Deleted along with the equally-dead `park_for_flash()` and `CORE0_RELEASE`; the six stale doc/comment sites describing the old design were rewritten in the same commit. |
| `examples/androidport/` | Orphaned — zero tracked files, only gitignored build output |

Also: headline test count for `platforms/rp` is 185 but **183 run** (2 live in dead files).

### 4.3 Layering

- **Clean:** HAL is a leaf (verified: no `use crate::{system,app}` in `hal/`); chip
  selection is one `#[cfg]` swap; `jvm`'s interpreter → heaps → classfile spine is acyclic;
  the 12 `ops_*.rs` opcode groups are a tidy split.
- **Cycle:** `native_handler` ↔ `graphics` (GC-root visiting reaches into
  `graphics::lvgl::{events,widgets,animations}` at `native_handler/mod.rs:220-235`, while
  graphics modules call back into `native_handler`). The *(roadmap)* central root-provider
  registry would break this cycle as a side effect — GC roots register themselves instead
  of being enumerated by the dispatcher.
- **Confusing twin trees:** `graphics/widgets/` (26 files, native-method impls) vs
  `graphics/lvgl/widgets/` (32 files, LVGL rendering) — both live, distinguished only by
  path. A naming/README fix, not a rewrite.
- **JVM/GC coupling:** `jvm/src/gc/mod.rs` hardcodes `"java/util/ArrayList"` etc.
  (`:250,260,329,336`) and reaches into collection backing stores — the collector can't be
  reasoned about without knowing native collection layout.
- **Feature matrix: disciplined.** Board capability cfgs (`has_display`, `has_buttons`,
  `sensor_*`…) are derived from each `board.toml` by `build.rs:160-186`, collapsing the
  combinatorial space to one cfg-set per board. The real drift axis is `feature="sim"`
  (218 uses) × `target_pointer_width` (33 uses) — nearly every hardware module has a sim
  fork that only parity discipline keeps honest.

## 5. Axis: reusability

Reality has three tiers (and ARCHITECTURE.md's framing — reference implementation vs
reusable crates — is the right one; the doc itself is just stale, see §8):

**Tier 1 — reusable today:** `compat` (exemplary zero-dep `no_std` leaf, single source of
truth used device- and host-side), `class-shrink` (the only tool with a `[lib]`, tested
across all 7 modules), and `jvm` *minus two leaks*: it ships the Picodroid PAPK parser
(`jvm/src/apk.rs`, 921 lines, including an LVGL-asset section) and a `compat` dependency
that exists only for PAPK — a generic embedded JVM has no business bundling an
app-container format. Extracting PAPK (see §6.1) makes `jvm` a genuinely neutral crate.

**Tier 2 — lib-shaped but locked in bins:** `pdb`'s `protocol.rs`/`papk_meta.rs` are
clean, tested pub-fn modules that `papk-info` cannot reuse because pdb is bin-only.

**Tier 3 — trapped:** the ~20.7K-line `system/` framework layer lives entirely in
`platforms/rp`. The encouraging measurement: **~90% of `system/` is already
silicon-free** — only 6 files touch rp-silicon crates, all cleanly cfg-gated; the real
coupling is `freertos_rust` used directly (no concurrency abstraction). With ESP removed,
extracting this layer loses its near-term driver — record it as the known cost of a future
second family, not a current work item.

**`picodroid-core` verdict:** a thin grab-bag (bytecode blob, name tables, priority
consts, 844-line LVGL FFI, embedded-hal drivers) whose modules share only "needed by more
than one platform build." With ESP gone it has a single consumer. **Recommendation: keep
it** — it costs nothing, hosts the well-guarded `dispatch_sites` and the LVGL drift-guard
test, and remains the landing zone if a second family returns — but clean it: drop the
`family-esp` feature, delete the never-used optional `freertos-rust` dep
(`Cargo.toml:16,34` — declared, never referenced), and fix its Cargo.toml porting comment
that documents a `src/hal/contract.rs` centralization that was never done.

**Third-party forks — inconsistent tracking:** `FreeRTOS-Kernel` is a proper pinned
submodule of vanilla upstream with SPDX manifests; the `littlefs-rust-core` fork's only
provenance is a root-Cargo.toml comment — no FORK.md, no pinned upstream ref, no
diffable record. Document the fork properly.

## 6. Axis: API contracts

The repo's pattern of **enforcing contracts with exhaustive tests** is its best idea. The
findings below are all "surface X exists but isn't under the pattern yet."

### 6.1 PAPK format: five independent declarations, zero shared code

The on-disk binary layout (24-byte header, 16-byte section headers, offset table, TLVs) is
re-derived by hand in:

1. `jvm/src/apk.rs` — runtime parser (treated as authoritative, well-tested)
2. `tools/papk-pack/src/main.rs:20-31` — writer (untested)
3. `tools/papk-info/src/main.rs:7-12` — reader (**zero tests**, silently `break`s on
   truncation)
4. `tools/pdb/src/papk_meta.rs:8-9` — reader (tested; self-describes as "Mirrors the PAPK
   format documented in `jvm/src/apk.rs`")
5. `build_support/papk.rs` — build-time emitter

`schema/PicodroidManifest.xsd` covers only the input XML and says itself it is "NOT wired
into the build." A header change today means editing 4-5 files with no compiler linkage.
**Fix shape:** extract a `no_std` `papk-format` crate (parser + writer + the format doc),
consume it from all five sites, and add pack→parse round-trip tests. This one change fixes
the tools' worst duplication, `papk-info`'s zero-test risk, *and* `jvm`'s reusability leak.

### 6.2 Handle table: the S1 contract-by-comment

[`handle_table.rs`](../platforms/rp/src/system/picodroid/graphics/lvgl/handle_table.rs):
the 32-bit hardware path is a raw `ptr as u32 as i32` bit-cast with **no invalidation** —
delete-hook, `DELETED` sentinel, and the opt-in sanitizer exist only on the 64-bit sim path.
The comment at `:52-58` admits a deleted handle "dangles into freed LVGL memory" on
hardware; this class already caused a hardware-only animation-engine hang and is
sim-invisible by construction (parity-audit HAL-05, severity S1). The 64-bit path also
never reclaims slots and hard-panics at 4,096 cumulative widgets. Zero tests. This is the
single highest-value hardening target in the repo: give the 32-bit path real invalidation
(e.g. generation-tagged indices) and unit-test the table on both widths.

### 6.3 Native dispatch: class-level guarded, method-level not

Class registration is stringly-typed but backstopped by `every_native_class_is_registered`
— as well-guarded as the pattern can be. **Method-level** dispatch is `match (class,
method)` string tuples with `_ => None` (e.g. `native_handler/io.rs:36-48`): a typo in any
of ~294 native methods silently becomes a runtime `NoSuchMethod` (softened only by curated
`API_HINTS`). The *(roadmap)* stage-2 method-level cross-check closes this; it is the
highest-leverage contract-test extension available. Relatedly, the Java-side bridge is
broad and un-cataloged: ~73 SDK files declare `native` methods directly; there is no
single catalog of the native surface.

### 6.4 Other unguarded surfaces

- **LVGL constants:** the event-code drift guard covers 9 `LV_EVENT_*` ordinals; the
  hand-maintained `LV_KEY_*`, `LV_STATE_*`, `LV_PART_*`, `LV_OBJ_FLAG_*`,
  `LV_COLOR_FORMAT_*` families (`lvgl_ffi.rs:116-349`) have identical exposure and **no
  guard** *(roadmap)*. The 2026-06 infinite-render-loop incident came from exactly this
  class.
- **Shrink maps:** injectivity of every committed map is unit-tested
  (`mapping.rs:300-319`), but the **append-only superset invariant (vN+1 ⊇ vN)** — the
  property that keeps old PAPKs runnable on new firmware — is enforced only at generation
  time, never re-validated in CI. Cheap insurance: add the cross-version diff test.
- **`papk-info` latent bug (proof of the gap):** `fmt_size`
  (`tools/papk-info/src/main.rs:272-280`) has two identical branches — any ≥1 MiB value is
  mislabeled as KiB (2 MiB → "2048.0 KiB"); the MiB branch is dead. This is textbook
  `clippy::if_same_then_else`, but papk-info is in no clippy lane and has no tests.
- **`jvm` embedder API accretion:** a genuinely designed core contract exists
  (`NativeMethodHandler` + `NativeContext` + `SharedJvmHeap`, with `gc_visit_roots` as a
  thoughtful native-root hook) — but `NativeContext` hands raw `&mut` to every heap and
  `interpreter::execute` is `pub` with 11 params, which is why `platforms/rp` reaches past
  the facade at **185 touch-points** (importing `ObjectHeap`/`ArrayHeap`/`StringTable`
  directly from HAL and net code). The invoke surface has also accreted to 5 near-duplicate
  `invoke_*` variants. Long-term: narrow `NativeContext`; short-term: document the
  de-facto contract and stop the touch-point count growing.
- **`lib.sh` parses `board.toml`/`mcu.toml` with grep/sed** (`:108-138`) — fragile to
  reformatting; mitigated but not eliminated by `test-jvm-env.sh`.
- **`pre-commit:42`** greps for *exactly 4* occurrences of `not(feature="family-rp")` — a
  magic count that breaks on any legitimate fifth use (and is ESP-related; see §7).

## 7. ESP removal plan (decision 2026-07-24)

**Executed 2026-07-25** — all checklist items below landed (plus stragglers the checklist
missed: the `resolve_board` esp branch in `scripts/lib.sh`, a `monitor_store.rs` doc
comment, `docs/parity-audit.md` scope lines, and the `index.mdx` platform card).

Footprint is small and contained — no git submodule, separate Cargo workspace, nothing in
per-push CI beyond what `test.sh` runs. Checklist for the removal session:

1. **Delete `platforms/esp/`** (own workspace + own `Cargo.lock`; nothing else path-deps
   into it — verified).
2. **`scripts/test.sh`** — remove the `platforms/esp` block (lines ~41-48).
3. **`scripts/pre-commit`** — remove the `tdeck_plus` clippy lane (lines ~122-132); revisit
   the `not(feature="family-rp")` count-4 grep at `:37-42` (the "ESP/no-family path"
   comment) — simplify or re-derive the expected count.
4. **`picodroid-core/Cargo.toml`** — drop `family-esp = []`; also drop the unused
   `freertos-rust` optional dep and fix the stale porting comment (§5).
5. **Docs** — README.md and ARCHITECTURE.md ESP/multi-family sections (fold into the §8
   doc refresh: either delete or mark "dormant — see git history"); website pages
   `get-started/esp32s3.md`, `reference/esp32s3-toolchain.md` (delete + remove sidebar
   entries in `astro.config.mjs`), plus ESP mentions in `build.md`, `index.mdx`,
   `limits.md`, `cargo-aliases.md`, `porting-guide.md`, `architecture.md`,
   `release-notes.md` (edit). Run `npm run build` in `website/` — the links validator will
   catch stragglers.
6. **Leave alone:** `third_party/FreeRTOS-Kernel` Xtensa ports (vanilla upstream
   submodule content); `platforms/rp/build.rs:44`'s `"xtensa"` arm in the target-arch
   match (harmless, generic).
7. **Bonus simplifications unlocked:** `test.sh` drops a whole cargo invocation; the
   duplicated `[patch.crates-io]` littlefs patch in the esp workspace goes away; the
   HAL-contract copy-paste and 17 drifting sim-stub twins (§4/§5 findings) dissolve.

Findings this decision retires: HAL-contract centralization, sim-stub drift, the
concurrency-HAL abstraction (record as future-family cost), and the esp/rp code-sharing
grade. `picodroid-core` stays (rationale in §5).

## 8. Documentation health

- **ARCHITECTURE.md is the right doc with the wrong paths**: 13 broken links (verified) —
  the whole module map points at a top-level `src/` that moved to `platforms/rp/src/`;
  `docs/porting-guide.md` moved to the website; `PICODROID_NATIVE_CLASSES` is cited in
  `native_handler/mod.rs` but lives in `native_handler/class_registry.rs`; it cites the
  orphaned `timer_alarm.rs` as live. Refresh alongside the ESP edit (§7.5).
- **README coverage is inconsistent with the reusability story:** exactly the three
  Tier-1 crates have READMEs (`jvm`, `compat`, `class-shrink`) — good signal — but
  `picodroid-core`, `platforms/rp`, and `pdb` have none.
- `docs/` itself is lean and current (`quality-roadmap.md`, `parity-audit.md`,
  `memory-diagnostics.md` are all recent and honest about blind spots). Website API docs
  exist for the SDK. Class-level javadoc sits at ~54% of public SDK types, but core
  classes are documented method-for-method against their `android.*` equivalents.

## 9. Prioritized backlog for fix sessions

**P0 — correctness/safety now, all cheap:**

1. Delete the 4 stale core-twins in `platforms/rp/src/` + `examples/androidport`;
   investigate-then-resolve `hal/rp/timer_alarm.rs` (§4.2 — replaced or lost?).
2. Fix `papk-info` `fmt_size`; add clippy lanes for `papk-pack`/`papk-info`/`class-shrink`.
3. Execute the ESP removal checklist (§7).
4. Add a `pico_enviro_mon` ARM firmware-compile gate (pre-commit or CI `building`); decide
   gate-or-delete for `board-testbench-rp2350w`.

**P1 — contract hardening (extend the existing guard pattern):**

5. Extract the `papk-format` crate + round-trip tests (§6.1) — also un-leaks `jvm`.
6. Method-level native-registry cross-check (§6.3, *roadmap* stage 2).
7. Extend the LVGL drift guard to the other constant families (§6.4).
8. Shrink-map superset (vN+1 ⊇ vN) CI test (§6.4).
9. Handle-table invalidation on the 32-bit path + tests on both widths (§6.2).
10. Per-push Java gate: add the remaining self-checking suites (`bytecodecoverage`,
    `prefs_demo`, `clinitdemo`, …) to CI `sim-smoke`, and a fast `sim-run --app langsuite`
    (or similar) lane to pre-commit.

**P2 — structure and polish:**

11. Split `lifecycle.rs` (widget dispatch out via `DISPATCH_SITES`; state machine behind a
    trait) — after the *(roadmap)* sim scenario tests exist as a net.
12. Split `object_heap/mod.rs` (StringBuilder scratch, formatting, exception side-tables
    out of the heap).
13. `jvm` facade tests (`lib.rs`); consolidate the 5 `invoke_*` variants; in-crate monitor
    tests; `map_store`/`iter_store` direct tests.
14. Make `pdb`'s protocol/papk modules a `[lib]`; wire `parity-bench.sh` into nightly HIL
    or descope it; document the littlefs fork (FORK.md + upstream ref).
15. ARCHITECTURE.md refresh + READMEs for `picodroid-core`/`platforms/rp`/`pdb` (§8).
16. `set -euo pipefail` in `build.sh`/`build-apk.sh`/`flash.sh`; replace `lib.sh`'s
    sed-TOML parsing; consider rewriting `hil-run.sh` (671 lines) as a real program.
17. GC-root provider registry *(roadmap)* — also breaks the `native_handler`↔`graphics`
    cycle (§4.3).

## Appendix: invariants verified directly during this audit

| Declared rule | Check | Result |
|---|---|---|
| `jvm` must not depend on `cortex_m`/`embassy`/`rp2*` | `rg` over `jvm/src` | **PASS** (empty) |
| `jvm` must not contain `picodroid/*` class names | `rg 'picodroid/' jvm/src` | **PASS** (empty) |
| `hal/` must not import `system`/`app` | `rg 'use crate::(system\|app)' platforms/rp/src/hal/` | **PASS** (empty) |
| ARCHITECTURE.md links resolve | link walk | **FAIL** — 13 broken (§8) |
| `timer_alarm.rs` reachable from mod tree | `rg timer_alarm` + `hal/rp/mod.rs` | **FAIL** — orphaned (§4.2) |
| ESP footprint outside `platforms/esp` | repo-wide `rg` | 1 core feature, 2 script blocks, docs only (§7) |
