# Coverage (`scripts/coverage.sh`)

Measured line, region and function coverage of the Rust workspace. Before
2026-10-14, every coverage statement in this repo was inferred from which
modules compile under test (`code-health-2026-09.md` backlog item 10).

Audience: developers and AI agents. Every command below is copy-pasteable.

## Layers

| Layer | What runs | Where it runs |
|---|---|---|
| **unit** | The host unit tests, exactly as `scripts/test.sh` runs them: the whole workspace, in both shrink modes | CI on every push (the `Coverage` job), and locally |
| **sim** | The `hil-tests.conf` rows and the `sim-run.sh` lanes (launcher, settings, pdb, enviro boards, …) on an instrumented simulator. This reaches the FreeRTOS-POSIX backing, boot, the lifecycle and the native handlers, which no unit test does | Locally, or on a schedule (see [Nightly](#nightly)) |
| **Java SDK** | Not built yet; see [designs/java-sdk-coverage-2026-10.md](designs/java-sdk-coverage-2026-10.md) | — |

Device-only code (`platforms/rp/src/hal/rp/**`, `hal/freertos_tcp`,
`boot_tasks`, the device `pdb`) cannot run on the host. It shows up in the
*Not in report* column, not as 0 %, and the HIL matrix stays the evidence
for it.

## Setup

Run this once per machine. CI installs the same versions.

```bash
rustup component add llvm-tools-preview --toolchain 1.98.1   # CI's RUST_TOOLCHAIN
cargo install cargo-llvm-cov --version 0.9.1 --locked
```

## Running

```bash
./scripts/coverage.sh                               # unit layer (~3 min)
./scripts/coverage.sh --sim --apps qa_lang,qa_life  # unit + a few sim rows
./scripts/coverage.sh --sim                         # unit + every sim row and lane
./scripts/coverage.sh --out /tmp/cov                # somewhere other than build/coverage/
```

Output goes to `build/coverage/`. With `--sim`, the merged unit + sim run gets
the same set again under `build/coverage/sim/`, and the unit-only numbers
stay in place, so each layer's contribution is visible.

| File | What |
|---|---|
| `summary.md` | Per-crate table: instrumented lines, line / region / function %, files not in the report. CI puts it in the job summary |
| `html/index.html` | Browsable per-file, per-line report |
| `lcov.info` | For editors (Coverage Gutters and similar) and any external tool |
| `summary.json` | `llvm-cov export --summary-only`, the input to the table and the ratchet |
| `uncompiled.txt` | `lines path` for every source file the report never names |

The instrumented builds live in `target/llvm-cov-target/`, so a coverage run
never invalidates `target/` for `sim.sh`, `test.sh` or the pre-commit lanes.
Each `coverage.sh` run starts with `cargo llvm-cov clean --workspace`. That
deletes stale profiles and instrumented workspace builds, but keeps
dependencies.

## Reading the numbers

- **Denominator.** Instrumented lines in non-test files. Files under
  `tests/`, `*_tests.rs`, `tests.rs`, `third_party/`, `benches/` and
  `examples/` are excluded. Inline `#[cfg(test)] mod tests` blocks inside a
  source file cannot be told apart on stable Rust, so they count and lift
  the percentage a little.
- **Not in report.** These are source files under a crate's `src/` that
  llvm-cov never names. Either the run never built them, or they hold no
  executable code (lists of `mod`s, type definitions, consts).
  - Device-only code: `hal/rp/**`, `freertos_tcp`, `boot_tasks`.
  - Opt-in diagnostics: `mem_diag.rs`, `sched_diag.rs`, `parity.rs`.
  - Sim networking: `hal/sim/net.rs`, `net/*`. The test build has no
    `has_network` board.

  A file that should be testable but sits here has a gate to remove.
- **`warning: N functions have mismatched data`.** llvm-cov prints this for
  about 126 of the workspace's functions, even on a single test pass. A
  function only counts towards the report when its profile hash matches its
  binary's, so the warning means a few functions are under-counted, never
  over-counted.
  - The shrink pass rebuilds some test binaries in place. This adds one
    `counter mismatch` warning at merge time, from about 5 functions whose
    code differs between the two maps.

### First measurement (unit layer, 35dc8d71, 2026-10-14)

Workspace: **62.4 % of 65,079 lines.** The extremes are below. The full
table is what `coverage.sh` prints.

| | Line % |
|---|--:|
| `compat`, `papk-format`, `heap4`, `pdb-protocol`, `http-head`, `pd-lvgl-sys` | 95–100 |
| `class-link`, `pd-json`, `pd-install` | 90–94 |
| `jvm` | 83.2 |
| `picodroid-core` | 44.3 (and 47 files / 8.5k lines not in the report) |
| `build_support`, `papk-info` | 28–32 |

Some inferred claims turn out wrong in both directions:
- `jvm::object_heap::map_store` and `iter_store` were listed as untested
  (code-health-audit-2026-07 item 13). They are 85 % and 100 % covered,
  indirectly.
- `lifecycle/activity_stack.rs`, `lifecycle/mod.rs` and
  `native_handler/mod.rs` are 0 % under the unit tests.
  - Three sim rows alone (helloworld, callbacktest, ellipsizedemo) bring
    `lifecycle/mod.rs` to 341 of 532 lines and `sim_boot.rs` to 94 of 121.
  - `activity_stack.rs` stays near 0 % even then.

## The sim layer

`scripts/sim-run.sh --coverage` is what `coverage.sh --sim` drives. It:

- **Builds instrumented.** It builds the sim (and `pdb`, and the host tools
  the APK build runs) with `cargo llvm-cov show-env`'s wrapper into
  `target/llvm-cov-target`.
  - It uses the **dev** profile, because `cargo llvm-cov report` reads a
    single profile directory and the unit binaries are in `debug/`.
- **Runs no-shrink only, and only the rows and lanes.** Shrinking renames
  Java names, not Rust paths. It skips heap-pressure, the mem-diag and
  sched-diag soaks, the GC-stress pass and the size ratchet.
- **Scales timeouts.** Each conf row's timeout is multiplied by
  `PICODROID_SIM_TIMEOUT_SCALE` (default 2).
- **Uses LLVM continuous mode** (`LLVM_PROFILE_FILE=…/sim-%p-%m%c.profraw`,
  built with `-Cllvm-args=-runtime-counter-relocation`). The counters live
  in the mmap'd profile file itself, so a sim stopped by SIGTERM still
  leaves its coverage. That covers most rows, which sim-run stops once
  their patterns match, plus the `timeout`-ended loop rows and an abort.
  - Without counter relocation the runtime refuses on Linux ("Neither
    `__llvm_profile_counter_bias` nor `__llvm_profile_bitmap_bias` is
    defined") and leaves an empty file. `coverage.sh` deletes empty
    profiles before it reports.
  - `%m` matters for the pdb lane. Its reboot re-`exec`s the sim under the
    same pid. With `%p-%c` alone, the new image truncated the file and wiped
    the counters of the image it replaced. Measured on that lane,
    `app_region::init` counted 1 run without `%m` and 3 with it (one cold
    boot, two warm boots).

The flag goes in through `CARGO_TARGET_<host>_RUSTFLAGS`, which cargo
appends to `.cargo/config.toml`'s host rustflags (`aes_force_soft`,
`polyval_force_soft`). A plain `RUSTFLAGS=` would silently drop them.

## Ratchet (not yet gating)

`scripts/coverage-baseline.conf` holds the per-crate line % of the unit
layer:

```bash
./scripts/coverage.sh --check    # FAIL on a crate more than 0.5 pp off its baseline, either way
./scripts/coverage.sh --accept   # rewrite the baseline; commit with a `coverage:` trailer
```

- `--check` fails on a rise as well as a drop, the way
  `check-source-guards.sh --unsafe` does. That keeps the floor at what the
  tree actually achieves.
- `PICODROID_COVERAGE_TOLERANCE` overrides the 0.5 pp tolerance. Two
  back-to-back runs of the same tree gave identical per-crate tables. Their
  totals differed by one line in 65,079, a thread-timing path. On the
  smallest crate, one line is about 0.3 pp.
- The CI job runs report-only. To gate, change its step to
  `bash scripts/coverage.sh --check`, in a commit of its own, once the
  nightly history shows the numbers are stable.

## Nightly

`coverage.sh` takes `--history FILE` and appends one row per crate and layer
(`utc,commit,layer,crate,lines,covered,line_pct`). Keep that file under
`build/`, which is untracked: the nightly must not dirty the shared checkout.

A full `--sim` run is the no-shrink half of the sim matrix without its soaks,
on a slower instrumented binary. It hasn't been timed yet; expect tens of
minutes. The 03:00 sim nightly already runs until about 04:00, and the 04:00
HIL fleet run builds firmware until about 08:30, so the suggested slot is
after both:

```cron
30 9 * * * cd /home/shiv/projects/picodroid-rs && mkdir -p build/coverage && ./scripts/coverage.sh --sim --out build/coverage/nightly --history build/coverage/history.csv >> build/coverage/cron.log 2>&1
```
