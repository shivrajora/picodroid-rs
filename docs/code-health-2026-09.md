# Code-health round, 2026-09-20

Follow-up to [`code-health-audit-2026-07.md`](code-health-audit-2026-07.md).
Three goals: make self-contained modules reusable by another project, raise
unit-test coverage where it is cheap and valuable, and break up the largest
files. Every change here is a move plus re-exports unless it says otherwise;
no behaviour was meant to change.

## What landed

### New crates

picodroid-core re-exports each one under its old module path, so no call
site changed.

| Crate | From | What had to be cut |
|---|---|---|
| `picodroid-build-support` | `crates/build_support/` (`#[path]`-included into two build scripts) | nothing; `freertos-cargo-build` went behind a `freertos-device` feature |
| `picodroid-test-support` | `crates/test_support/` (`#[path]`-included at nine sites) | the duplicated directory walker in `gc_root_scan` |
| `http-head` | `net/http_head.rs` | nothing |
| `heap4` | `hal/sim/heap4.rs` | nothing |
| `pd-json` | `json/` | the global pool and its `AtomicSection` guard stay in core; the crate's `Pool` is caller-owned |
| `pd-drivers` | `drivers/` | three identical `I2cBus` traits became one; per-driver `cfg` gates dropped (the drivers are generic, a board links what it instantiates) |
| `pd-install` | `install/` | new `PackageDirectory` trait with `const CAN_COMPACT`; framework-map version passed in; `Plan` / `PlanError` / `run_sectors` moved with it |
| `pd-lvgl-sys` | `lvgl_ffi.rs` + the LVGL C build | own `build.rs` resolving the board from forwarded `board-*` features; `links = "lvgl"` |
| `pd-rtos` | `rtos/mod.rs` + `jvm_run_lock.rs` | nothing: the two reference only each other. The run lock moved *with* the wrappers rather than becoming a registered hook, because a hook nobody installed is a lock that is silently absent |

`crates/jvm/build.rs` still `#[path]`-includes `jvm_defaults.rs` and
`names.rs`: pico-jvm takes no path build-dependency, which is what keeps it
liftable.

### File splits

| File | Was | Now |
|---|---:|---|
| `jvm/src/native/tests.rs` | 5,837 | `native/tests/` — `mod.rs` (harness) + 13 topic files |
| `picodroid-core/src/lifecycle.rs` | 2,562 | `lifecycle/{mod, activity_stack, widget_events, input, alarm_events}.rs` |
| `jvm/src/object_heap/mod.rs` | 2,174 | 1,156, plus `tests`, `growth_tests`, `num_fmt`, `boxed_cache` |
| `jvm/src/gc/tests.rs` | 2,052 | `gc/tests/` — 7 topic files |
| `picodroid-core/src/packages.rs` | 1,920 | 790, plus `packages/{plan, tests, test_support}.rs` |
| `jvm/src/interpreter/ops_invoke.rs` | 1,725 | 962, plus `ops_indy.rs`, `coll_fastpath.rs` |
| `picodroid-core/src/lvgl_ffi.rs` | 1,708 | moved whole into `pd-lvgl-sys` (not split further: it is one flat declaration list) |
| `papk-format/src/lib.rs` | 1,431 | 830, plus `tests.rs` |
| `graphics/lvgl/events.rs` | 1,363 | `events/{mod, groups, keypad, touch, swipe, focus, tests}.rs` |
| `class-shrink/src/shrink.rs` | 1,284 | 713, plus `shrink/tests.rs` |
| `jvm/src/array_heap.rs` | 1,180 | 738, plus `array_heap/tests.rs` |
| `scripts/lib.sh` | 849 | 108, plus `net-lib.sh`, `board-lib.sh`, `lock-lib.sh`, `build-lib.sh` (all sourced by `lib.sh`; `declare -f` identical across 60 functions) |

Convention, from `crates/jvm/src/interpreter/`: `X.rs` becomes `X/mod.rs`;
children do `use super::*`; items that now cross a file become `pub(super)`;
`mod.rs` re-exports whatever the rest of the crate names, flat.

### Coverage

- **`graphics/lvgl/*` compiles under `cargo test`.** Its `cfg(not(test))`
  gates existed because the bindings' `extern "C"` block is itself
  `cfg(not(test))`. As the `pd-lvgl-sys` crate the bindings are an ordinary
  dependency of core's test build, so ~10k lines can carry unit tests for
  the first time. Tests there must call only pure functions — LVGL is
  linked, not initialised.
- `events.rs`'s 7 tests had never run (gated out, no shim). 3 run now; the
  other 4 are `cfg(has_buttons)` and still do not, because no test build
  selects a board with buttons. **Open.**
- New tests: `build_support` config + names (16 → 34), `papk-pack` value
  grammar (16 → 24), `pdb` ping decode (23 → 29), animation easing and unit
  conversion, touch-calibration fit (14).
- Bugs found writing them: `build_support::config::strip_quotes` panicked
  on a value that is a single quote character.
- Workspace total across both shrink modes: 3,148 at the start; 3,116 after
  removing 32 duplicate runs of the `flash_layout` tests (see corrections);
  3,214 at the end (+98: 49 new or newly running tests, in each mode).

### Guards touched

`spin_guard` scan roots (pd-drivers, pd-install); `porting.rs` seam files
and macro scan (pd-install, pd-rtos); `rtos::seam_guard` second root and a
must-see entry that `graphics/lvgl/lifecycle.rs` had been satisfying by
accident; `check-source-guards.sh` cfg-gate check widened to every crate;
`alloc_scan` skipping `tests/` directories; `qa_shape_guards` and the
GC-root registration for the `lifecycle/` layout; the tick-source guard's
`lv_conf.h` path; the builtin-method contract test reading three interpreter
files. Each was sabotage-checked where the change could have made it pass
vacuously.

## Flash (testbench_rp2040, release — what the ratchet gates)

| Commit | Flash | RAM |
|---|---:|---:|
| e9aaf811, before this round | 857,728 | 245,812 |
| Phase 0–1 (support crates, http-head, heap4, pd-json, pd-drivers) | 857,724 | 245,812 |
| pd-install | 857,756 | 245,812 |
| pd-lvgl-sys | 857,752 | 245,812 |
| pd-rtos | 857,392 | 245,812 |
| lifecycle split | 857,400 | 245,812 |
| events split + lvgl ungating | 857,576 | 245,820 |
| packages + object_heap splits | 857,620 | 245,820 |
| ops_invoke split | 857,716 | 245,820 |

Net for the round: **−12 B flash, +8 B RAM.** The crate extractions are
size-neutral or better; what costs flash is splitting a *production* file:
both profiles use one codegen unit, so no inlining moves, but each new file
with a panic site adds its source path to `.rodata` (~50–60 B each).

**The size ratchet was already failing before this round**: `ratchet.toml`
records 857,240 and the tree stood at 857,728 (+488) after the T3.1-F
trampoline commit. It has not been advanced here; doing so is the explicit
act of consenting to the spend and is left to the maintainer.

Tried and rejected on measurement: marking `pd-rtos`'s 28 thin wrappers
`#[inline]` cost +2,908 B on RP2040 and +3,152 B on RP2350 — they were never
inlined, and inlining copies the run-lock release/retake into every call site.

## Corrections to claims made during the round

- Commit 5ea00f9f says build_support's 16 `flash_layout` tests "had never
  executed". They had, through a `#[path]` shim in `platforms/rp/src/main.rs`.
  Corrected in 6130aae8, which removes the now-duplicate shim.
- The survey's "one outbound reference" for `jvm/src/class_file/` undersold
  it; see below.

## Deliberately not done

- **`class-file` crate.** `ClassFile` is a JVM runtime structure: `'static`
  flash-backed data, lazy parse behind a `OnceCell`, `pub(crate)`
  constructors the class table uses, `line-numbers` / `mem-diag` features,
  a shrink-map-dependent spelling of `java/lang/Object`, and measured
  `#[inline(never)]` placements. It is already reachable as
  `pico_jvm::class_file`, and pico-jvm depends only on `libm`.
- **Generic `SlotTable` for the handle table.** Its state is four separate
  statics so the 1 KB pointer array lands in `.bss`; one struct would move
  it into `.data` and cost ~1 KB of RP2040 flash. One consumer.
- **`hal/sim/allocator.rs` parameterisation.** It stays in core (a process
  global allocator, env-driven, `mem-diag`-aware), so turning its board
  constant into a parameter buys nothing.
- **`native_handler/method_tables.rs`.** Test-only `#[path]` wiring keeps
  ~310 string triples out of `.rodata`, and child modules of a
  `#[path]`-included file hit the nesting trap.
- **`tools/papk-pack/src/res.rs` test extraction.** Its tests embed
  raw-string XML fixtures a dedent would corrupt.

## Backlog

Ranked by value over cost.

1. **`pd-hal`**, then the `graphics/lvgl/` engine above `pd-lvgl-sys`
   (~10.8k lines, the largest reusable asset left). The engine calls the HAL
   facade at ~45 sites and has 143 `pub(in crate::graphics)` functions; on
   the LTO-less RP2040 those stop inlining across a crate boundary. The HAL
   facade's only non-HAL references are `board_cfg` (3), `rtos` (3) and
   `task_priority` (1), all now satisfiable from `pd-rtos`.
2. **Pure back-stack transitions out of `lifecycle/activity_stack.rs`.**
   Still the largest untested state machine. `native_handler::state` (24
   tests) already supplies the data types; what is missing is separating
   "what the stack becomes" from the JVM and LVGL calls that carry it out.
3. **Ungate more of core under test.** `graphics/{assets, display, fields,
   view, view_group, widgets}` fail only on `native_handler`, which is
   `cfg(not(test))` for its own reasons. If that gate can go the way
   `graphics/lvgl`'s did, most of the 14 `#[path]` shims in `lib.rs` become
   unnecessary.
4. More in-place tests now that `graphics/lvgl` compiles under test:
   `time_picker` 12/24 h wrap and minute stepping, `number_picker`
   clamp/wrap/step, `hw_scroll::is_valid`.
5. `net/http_connection.rs::parse_response_head` and the chunk stepping →
   `http-head`; `hardware/sensors/sampler.rs::Sampler::service` over a
   timeline with the fake I2C bus in `pd-drivers`; `service_lifecycle.rs`
   connection table; `tools/class-shrink/src/main.rs` `cmd_*`.
6. A test configuration with `has_buttons`, so the 4 gated `events` tests run.
7. `executors/` extraction: enters `pico_jvm` atomic sections and roots
   obj_refs for the GC; needs four injections and has no second consumer.
8. Splits not done: `sched_diag.rs` (1,116), `tools/papk-pack/src/main.rs`
   (1,269), `scripts/sim-run.sh` (1,059), and a shared runner library for
   `hil-run.sh` / `sim-run.sh` (the latter is a behaviour change on the
   nightly path).
9. Kotlin `buildSrc/classfile/*` has no tests and is a third class-file
   parser; the `pdb list` text schema is still hand-mirrored
   (`docs/designs/pdb-schema-as-code.md`).
10. A coverage tool. None is configured; every coverage statement in this
    document is inferred from which modules compile under test.

## Needs hardware

Nothing in this round was run on a board. `pd-rtos` carries the run lock,
which is what keeps the shared heap sound on SMP parts — `threadstress` and
`qa_thr` pass in the simulator, but the nightly HIL run is the first real
evidence. Also wanted: `pdb install` fresh / upgrade / compact / uninstall
on RP2040 and RP2350 for `pd-install`, and one board per panel driver plus
the `hw_vscroll` board for `pd-lvgl-sys` and `pd-drivers`.
