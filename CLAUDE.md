# Picodroid Development Guidelines

## Two-crate layout

Family-neutral framework code lives in `crates/picodroid-core/` (JVM natives, lifecycle, graphics, networking, sim HAL); family-specific code lives in `platforms/rp/`. Never create a file at the same relative path in both `src` trees — the pre-commit shadow-twin guard rejects it. When moving code between them, move it; don't copy.

## Project Goal

Picodroid brings Android-like Java app development to embedded systems. The Java API exposed to developers should stay as close to its Android counterpart as possible — class names, method signatures, semantics, and idioms should match `android.*` so that code and developer intuition transfer directly. When a design choice is forced by embedded constraints prefer the option that preserves the Android-facing API surface, even if the internal implementation diverges.

## Apps import `picodroid.*`, never `android.*`

Apps must import the SDK classes directly as `picodroid.*` (e.g. `import picodroid.view.View;`). Importing `android.*` is **not supported** — there is no `android.*` stub jar or alias layer, so `import android.view.View;` will neither compile nor load. Do not add one back, and do not write apps or examples that import `android.*`.

This does not contradict the Project Goal: the goal means the picodroid API is *named* to mirror `android.*` (so `picodroid.view.View` matches `android.view.View` method-for-method and intuition transfers) — it does **not** mean apps import the `android` namespace.

## After Every Code Change

Two checks, both cheap. CI and the nightlies are the regression gates, not your machine.

### 1. Sim smoke test

After a change under `crates/`, `platforms/`, `sdk/` or `system-apps/`:

```bash
./scripts/sim.sh --app helloworld
```

Confirm `[HelloWorld] Hello, World!` appears. Docs, example-app and script-only edits need no smoke. Every other app (`benchmark`, `blinky`, the `qa_*` suites, …) runs in the 3 AM sim nightly, and GitHub CI runs a 17-app sim smoke on every push.

### 2. Pre-commit

```bash
./scripts/pre-commit          # after every change; what the git hook runs
```

Must end with `==> All checks passed.` It takes seconds and builds nothing: the shadow-twin and cfg-hygiene guards, `apply_jvm_env`, and whichever of `cargo fmt`, Java/Kotlin formatting and markdown lint the changed files implicate. A `scripts/` change adds the `hil-tests.conf` drift check and the device-lock test.

Then push. Do not wait for anything longer locally. GitHub CI (~55 min) runs clippy for every board, both boards in debug and release, the tests in both shrink modes, every example APK, the same source guards and the sim smoke; the 3 AM `sim-run.sh` runs the whole `hil-tests.conf` matrix in both shrink modes (the `qa_*` apps, the diagnostics soaks, the binary-size ratchet) and the 4 AM `hil-fleet.sh` runs it on hardware. After a push, `gh run list --limit 3` shows CI; nightly results arrive by email and under `build/sim/results/` and `build/hil/results/`.

```bash
./scripts/pre-commit --full   # before cutting a release
```

`--full` is the release-cut gate and covers only the legs nothing else runs: the staged `handle-table-32` clippy and build, the opt-in `mem-diag` / `sched-diag` firmware builds, `pico_enviro_mon_w` clippy, the shrunk-image name check and the size ratchet on both boards. A few minutes.

`--list` prints the stages a run would execute; `--serial` runs the lanes one at
a time and streams to stdout, which is what to use when a parallel run fails and
you want readable output. Per-run logs are kept under `build/pre-commit/`.

WiFi-enabled device builds (`testbench_rp2350w`, `pico_enviro_mon_w`, `pico_touch_kit`) take `PICODROID_WIFI_SSID` / `PICODROID_WIFI_PASS` at build time; local credentials live in the gitignored `.wifi-creds.env` at the repo root. `hil-run.sh` reads that file itself for the `net` rows of `hil-tests.conf` and SKIPs them when it is missing.

## Shared bench: one lease per board

Several boards on the bench, each with its own debug probe, and several parallel sessions. `flash.sh`, `power-cycle.sh`, `pdb.sh`, `parity-bench.sh --hil` and `hil-run.sh` take a lease on **one** board through `scripts/device-lock.sh` (`lib.sh::require_device_lock`) before touching it. Say which board with `--board NAME` (or `--slot NAME`); with neither, the script uses the board this session already holds, or the only one on the bench, or stops and lists the slots. If the board is free the script acquires it for **this session** (owner `claude:<session id>`, alive as long as the session is) and keeps it until you release, so a flash followed by pdb calls needs no ceremony. If another session holds that board the script exits 75 with the holder and a hint; the other boards stay free. Waiters queue FIFO.

The bench is described by `~/.config/picodroid/fleet.conf` (format in `scripts/fleet.conf.example`; `./scripts/fleet.sh discover` prints what is plugged in, `fleet.sh check` validates the file). Without that file every script assumes a single board, as before.

```bash
./scripts/device-lock.sh status                     # every slot: holder, since when, queue
./scripts/device-lock.sh acquire --board X --wait   # queue; run it with run_in_background and you are notified on acquisition
./scripts/device-lock.sh release                    # everything you hold; also kills your lingering probe-rs
./scripts/fleet.sh discover                         # probes and boards on USB, with their positions
```

Never `pkill -f probe-rs` (it kills any shell whose command line mentions it); `release` kills only the probe-rs on your board's probe. Overnight soaks launched with `setsid nohup` need a lease that outlives the session: `PICODROID_DEVICE_OWNER=soak ./scripts/device-lock.sh acquire --board X --pin` before the flash, `release` at teardown. The 4 AM `hil-fleet.sh` runs every board at once; each runner waits up to an hour for its board, then records a SKIP. `PICODROID_DEVICE_LOCK=0` bypasses the check (emergencies only).

> **When debugging:** Skip these checks during intermediate debugging steps. Only run them once you are confident the bug is fixed.
>
> **When debugging memory (heap growth, churn, OOM, corruption):** opt-in monitors and offensive checks exist — see `docs/memory-diagnostics.md` (`./scripts/sim.sh --app <app> --mem-diag`).
>
> **When debugging scheduling (a task hogging a core, sleep-polling, starving a peer, a spin that runs long):** the opt-in scheduling monitor — see `docs/scheduling-diagnostics.md` (`./scripts/sim.sh --app <app> --sched-diag`).
