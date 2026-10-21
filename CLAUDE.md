# Picodroid: guide for agents and contributors

Picodroid brings Android-style Java app development to microcontrollers: a JVM, an Android-shaped SDK (`picodroid.*`) and LVGL graphics on FreeRTOS, in Rust, for the RP2040 and RP2350 boards, with a host simulator that runs the same kernel. Read this file first, then the orientation table. `AGENTS.md` is the same file.

## 1. Orientation (5 minutes)

| Read | Why |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | How the runtime fits together, by reusability |
| [`docs/README.md`](docs/README.md) | The engineering record, routed by subsystem ("working on networking: read these, in this order"); status lines and amendments |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Setup, tests, the pre-commit tiers, adding an app or a native method, cutting a release |
| [`docs/gaps.md`](docs/gaps.md) | The one ranked work list. Pick from here |
| [`docs/scorecard.md`](docs/scorecard.md) | Generated, measured numbers: Android parity, flash, panics, unsafe, coverage, the test matrix |
| [`docs/devlog.md`](docs/devlog.md) | What the last sessions landed and left open; where the next one picks up |
| [`docs/decisions.md`](docs/decisions.md) | Dated rationale for the non-obvious choices, so they are not re-litigated |
| [`.claude/rules/`](.claude/rules/) | The conventions by area: Rust, the Java SDK, testing, the bench. Loaded automatically |
| Website `reference/pdb-commands.md`, `guides/debugging.md` | Driving an app on a device (`pdb input`) and in the simulator (`sim-ctrl.sh`, `sim-shot.sh`, `test.ctrl`) |

Nested `CLAUDE.md` files in `crates/jvm/`, `crates/picodroid-core/`, `platforms/rp/`, `sdk/`, `scripts/`, `system-apps/` and `crates/fuzz-targets/` carry that area's conventions and its exact test command.

## 2. Workspace map

Layers from `scripts/layers.conf`, enforced by `check-source-guards.sh --layers`: a crate depends only on lower layers.

```text
L0  class-link compat heap4 http-head pd-drivers pd-json pd-lvgl-sys pd-rtos pd-tls pdb-protocol
    leaf parsers, protocols, the RTOS seam, LVGL bindings, drivers
L1  papk-format (the app package)   jvm (pico-jvm: the interpreter, heap, GC, threads)
L2  pd-codec (system records)   pd-install (the app region)   pd-protobuf
L3  pd-pdb (the debug bridge's command loop)
L4  picodroid-core   family-neutral framework: natives, lifecycle, graphics, networking, the sim HAL
L5  platforms/rp     the RP2040 / RP2350 family: boards, HAL, main
host  crates/build_support  tools/{class-shrink,papk-info,papk-pack,pdb}
test  crates/test_support  crates/fuzz-targets
sdk/            the Java SDK (picodroid.*, java.* stubs), its name tables and shrink maps
system-apps/    launcher, settings      examples/   the apps, the qa_* suites, test.ctrl drivers
scripts/        every workflow          bench/      the size ratchet      fuzz/   libFuzzer + regress
website/        the published manual    docs/       the engineering record
```

## 3. Golden rules

1. **The API mirrors `android.*`.** Class names, method signatures, semantics and idioms match Android so code and intuition transfer. When an embedded constraint forces a choice, keep the Android-facing surface and let the implementation diverge. Apps import `picodroid.*`; there is no `android.*` alias layer and none is coming back (`import android.view.View;` neither compiles nor loads).
2. **Two source trees, never twins.** Family-neutral code lives in `crates/picodroid-core/`, family-specific code in `platforms/rp/`. The same relative path may not exist under both `src` trees (the shadow-twin guard). Move code; never copy it.
3. **Firmware never panics.** A panic on a board is a reset with the app's state gone. Return the crate's error type and propagate with `?`; `unwrap_or*` only a fallback that is truly correct; check input-derived indices and sizes; cap allocations by input. The leaf crates deny `clippy::unwrap_used` and friends; the rest are on a ratchet (`scripts/panic-baseline.conf`), which only goes down. A provably infallible site keeps a comment saying why.
4. **Every crash fix ships its reproducer.** A parser bug's minimised input goes to `fuzz/regress/` (replayed by `test.sh`); anything else gets a unit test or a `qa_*` row that failed before the fix.
5. **Verify UI changes with a capture.** `./scripts/sim-shot.sh` boots the app, sends control lines and saves a PNG plus the log; look at the PNG and read the `[layout]` numbers. A guard scripts can run beats a note that says "looks right".
6. **Numbers are dated and tied to a commit.** A measurement names the tree it came from; re-measure before relying on one from a different tree. Generated files (`docs/scorecard.md`, `sdk/api-contract.tsv`, `sdk/*-names.tsv`) are never hand-edited: change the source and regenerate.
7. **Architecture rules become guards.** A fixed architecture-class bug pairs with a text-scan test with a pinned count, a config assertion where a number is load-bearing, or an opt-in monitor with a strict mode the soaks run (`spin_guard`, `seam_guard`, the ratchets).

## 4. After every code change

Two checks, both cheap. CI and the nightlies are the regression gates, not your machine.

### 4.1 Sim smoke test

After a change under `crates/`, `platforms/`, `sdk/` or `system-apps/`:

```bash
./scripts/sim.sh --app helloworld
```

Confirm `[HelloWorld] Hello, World!` appears. Docs, example-app and script-only edits need no smoke. Every other app (`benchmark`, `blinky`, the `qa_*` suites, …) runs in the sim nightly, and GitHub CI runs a 15-app sim smoke, in four parallel shards, on every push.

### 4.2 Pre-commit

```bash
./scripts/pre-commit          # after every change; what the git hook runs
```

Must end with `==> All checks passed.` It runs the source guards (shadow twins, cfg hygiene, the unsafe and panic ratchets, crate layering, `apply_jvm_env`) and whichever of `cargo fmt`, Java/Kotlin formatting and markdown lint the changed files implicate. A Rust change adds CI's clippy legs on CI's pinned toolchain (`RUST_TOOLCHAIN` in `ci_checks.yml`; ~30 s warm), the leaf-crate leg that enforces the never-panic denies, and compiles `test.sh`'s test binaries without running them (clippy never sees `#[cfg(test)]` code). A `scripts/` change adds the `hil-tests.conf` drift check and the device-lock test. An `sdk/` or Rust change adds the three name-table currency tests (what `gen-api-contract.sh` regenerates: run it after the *last* Java edit, since javac numbers its lambdas by position) and the embedded-class guards; a change under `sdk/`, `scripts/*.conf` or `bench/parity/` adds `scorecard.py --check`; a change that adds an SDK class also runs the `--shrink` image check CI's Building job runs.

Do not wait for anything longer locally; commit and push only when asked. GitHub CI (~20 min) runs clippy for every board, both boards in debug and release, the tests in both shrink modes, every example APK, the same source guards and the sim smoke; the 3 AM `scripts/nightly.sh` cron runs three stages back to back: `fuzz.sh --nightly` fuzzes every untrusted-input parser (docs/fuzzing.md), then `sim-run.sh` runs the whole `hil-tests.conf` matrix in both shrink modes (the `qa_*` apps, the diagnostics soaks, a GC-stress pass, the binary-size ratchet), then `hil-fleet.sh` runs it on hardware (roughly 03:00–04:00, 04:00–05:35, 05:35–09:20). After a push, `gh run list --limit 3` shows CI; nightly results arrive by email and under `build/sim/results/`, `build/hil/results/` and `build/fuzz/results/`.

```bash
./scripts/pre-commit --full   # before cutting a release
```

`--full` is the release-cut gate and covers only the legs nothing else runs: the opt-in `mem-diag` / `sched-diag` firmware builds, the shrunk-image name check and the size ratchet on both boards. A few minutes. `--list` prints the stages a run would execute; `--serial` runs the lanes one at a time and streams to stdout, which is what to use when a parallel run fails. Per-run logs are kept under `build/pre-commit/`.

> **When debugging:** skip both checks during intermediate steps; run them once the fix is in. Memory (heap growth, churn, OOM, corruption): `docs/memory-diagnostics.md` (`./scripts/sim.sh --app <app> --mem-diag`). Scheduling (a task hogging a core, sleep-polling, starvation, a long spin): `docs/scheduling-diagnostics.md` (`--sched-diag`).

WiFi-enabled device builds (`testbench_rp2350w`, `pico_enviro_mon_w`, `pico_display2_w`, `pico_touch_kit`) take `PICODROID_WIFI_SSID` / `PICODROID_WIFI_PASS` at build time; local credentials live in the gitignored `.wifi-creds.env` at the repo root. `hil-run.sh` reads that file itself for the `net` rows of `hil-tests.conf` and SKIPs them when it is missing. A device can also be provisioned from Settings → Wi-Fi (saved at `/system/wifi`, rejoined at boot); build-time credentials override that (docs/designs/wifi-provisioning-2026-09.md).

## 5. Picking work

1. **`docs/gaps.md`** ranks every known shortfall with an estimate and a **Human** marker. Take the highest-ranked entry nobody is working on. When you close part of one, update its evidence and estimate in the same commit.
2. **`docs/scorecard.md`** has the measured numbers: a `missing` parity row, a crate with panic sites left, a board near its flash ceiling, a crate under the coverage floor. Prefer a number to an estimate.
3. **Last night's failures** under `build/{sim,hil,fuzz}/results/` (newest file), and the email. A row that failed without a matching code change: read its log, then check for leaked prefs and live listeners before debugging the app.
4. **`docs/devlog.md`**, the "still open" lines of the latest entries.

## 6. Before you finish

A change is done when every line below that applies to it holds; the first five are gated.

- [ ] `./scripts/pre-commit` ends with `==> All checks passed.`, and the sim smoke printed its line.
- [ ] A new Java API mirrors its Android counterpart (name, signature, semantics); native registry and method-table rows added; `./scripts/gen-api-contract.sh` run after the last Java edit.
- [ ] A test exists: a unit test, a `fuzz/regress/` input, or a `hil-tests.conf` row driven by a `qa_*` or example app (with a `test.ctrl` when UI is involved). `scripts/check-hil-conf.sh` keeps the row honest.
- [ ] Flash moved: the ratchet accepted in the same commit (`size:` trailer; `scripts/bench-report.py --ratchet --accept`). Unsafe or panic count moved: `check-source-guards.sh --unsafe-accept` / `--panics-accept`.
- [ ] `./scripts/scorecard.py --write` when `sdk/`, a baseline or `hil-tests.conf` changed; `sdk/android-parity.tsv` gets a row for a new SDK class.
- [ ] UI verified with a `sim-shot.sh` capture; a device check when the change touches a driver, the HAL or timing.
- [ ] The website page that documents the surface updated (`website/src/content/docs/api/*.md`, `reference/compatibility-matrix.md`); a design doc's status line and `docs/README.md` row updated.
- [ ] A terse entry at the top of `docs/devlog.md`: what landed (commit), the numbers, what is still open. Sessions end abruptly; the devlog plus a green tree is how the next one picks up. A non-obvious choice goes to `docs/decisions.md`.

Commit messages carry no attribution footer (no `Co-Authored-By`, no "Generated with"); a local hook strips one anyway.

## 7. Parallel agents

- **One task, one worktree.** `git worktree add -b <branch> .claude/worktrees/<name> main`, then `git submodule update --init --recursive` in it (a new worktree's `third_party/` is empty, `pd-lvgl-sys` compiles an empty archive from it, and the host tests fail to link `lv_*` symbols; after populating, `cargo clean -p pd-lvgl-sys`), copy the main checkout's `Cargo.lock` and `scripts/lint-md/node_modules` in (both gitignored; a fresh resolve shows phantom flash growth and pre-commit's markdown lint needs the install), and strip the thumb `rustflags` array from `.cargo/config.toml` for any firmware link (cargo merges it with the parent checkout's copy and the link sees `-Tlink.x` twice); restore it before committing. Several sessions share the main checkout and `git checkout` there hijacks whoever is mid-edit. Run `git status --porcelain && git branch --show-current` before any branch operation. Prefix every Bash command with `cd <worktree> &&`: the shell's cwd can reset to the shared checkout.
- **Your own build directories.** `CARGO_TARGET_DIR` and `SIM_TARGET_DIR` per worktree; `target/` is shared otherwise and another session's build replaces your ELF. Copy a binary out of `target/` before measuring it. Never two APK builds at once (`sim.sh` and `flash.sh` race on `build/apks/<app>.papk`); one `hil-run.sh` per slot.
- **Edit only the files you own.** Shared hotspots get small, surgical edits after a re-read: the native registry and method tables, `sdk/class-names.tsv` / `member-names.tsv`, `hil-tests.conf`, `docs/README.md`, the baselines. Never `git add -A` (it stages a peer's work in progress); never `cargo fmt --all` over a peer's unformatted files; if a peer's edit breaks the build, wait, do not fix their files.
- **The hook runs `main`'s scripts.** In a worktree a guard fixed on the branch is not enforced until it is merged; run the script by hand.
- **Kill by exact name or pid**, never `pkill -f <pattern>` from a command line that mentions the pattern (it shoots your own shell, exit 144). Sims: `pkill -x picodroid` scoped to your tree; probes: `device-lock.sh release`.
- **Detached work** (`setsid nohup … &`) outlives the session; a `run_in_background` task does not, and the 600 s tool timeout applies to it.

## 8. Shared bench: one lease per board

Several boards on the bench, each with its own debug probe, and several parallel sessions. `flash.sh`, `power-cycle.sh`, `pdb.sh`, `parity-bench.sh --hil` and `hil-run.sh` take a lease on **one** board through `scripts/device-lock.sh` (`lock-lib.sh::require_device_lock`) before touching it. Say which board with `--board NAME` (or `--slot NAME`); with neither, the script uses the board this session already holds, or the only one on the bench, or stops and lists the slots. If the board is free the script acquires it for **this session** (owner `claude:<session id>`, alive as long as the session is) and keeps it until you release, so a flash followed by pdb calls needs no ceremony. If another session holds that board the script exits 75 with the holder and a hint; the other boards stay free. Waiters queue FIFO.

The bench is described by `~/.config/picodroid/fleet.conf` (format in `scripts/fleet.conf.example`; `./scripts/fleet.sh discover` prints what is plugged in, `fleet.sh check` validates the file). Without that file every script assumes a single board, as before.

```bash
./scripts/device-lock.sh status                     # every slot: holder, since when, queue
./scripts/device-lock.sh acquire --board X --wait   # queue; run it with run_in_background and you are notified on acquisition
./scripts/device-lock.sh release                    # everything you hold; also kills your lingering probe-rs
./scripts/fleet.sh discover                         # probes and boards on USB, with their positions
```

Never `pkill -f probe-rs` (it kills any shell whose command line mentions it); `release` kills only the probe-rs on your board's probe. Overnight soaks launched with `setsid nohup` need a lease that outlives the session: `PICODROID_DEVICE_OWNER=soak ./scripts/device-lock.sh acquire --board X --pin` before the flash, `release` at teardown; one owner name per detached script, since `release` drops every slot that owner holds. The nightly `hil-fleet.sh` (from about 05:35) runs every board at once; each runner waits up to an hour for its board, then records a SKIP. `PICODROID_DEVICE_LOCK=0` bypasses the check (emergencies only).

## 9. Where things are tracked

- `docs/gaps.md`: the ranked work list. `docs/scorecard.md`: the numbers. `docs/devlog.md`: the sessions. `docs/decisions.md`: the why.
- `docs/README.md`: every design, audit and bug record with its status; closed items move to `docs/completed/`.
- `bench/parity/ratchet.toml`, `scripts/{unsafe,panic,coverage}-baseline.conf`, `scripts/layers.conf`: the gates.
- `sdk/android-parity.tsv`: the Android surface and its status. `sdk/api-contract.tsv`: the served `java/**` surface (generated).
- `website/src/content/docs/`: the published manual; `project/release-notes.md` and `reference/shrinker.md` change at every release cut, which happens on `main` only, when asked.
