# Picodroid Development Guidelines

## Two-crate layout

Family-neutral framework code lives in `picodroid-core/` (JVM natives, lifecycle, graphics, networking, sim HAL); family-specific code lives in `platforms/rp/`. Never create a file at the same relative path in both `src` trees — the pre-commit shadow-twin guard rejects it. When moving code between them, move it; don't copy.

## Project Goal

Picodroid brings Android-like Java app development to embedded systems. The Java API exposed to developers should stay as close to its Android counterpart as possible — class names, method signatures, semantics, and idioms should match `android.*` so that code and developer intuition transfer directly. When a design choice is forced by embedded constraints prefer the option that preserves the Android-facing API surface, even if the internal implementation diverges.

## Apps import `picodroid.*`, never `android.*`

Apps must import the SDK classes directly as `picodroid.*` (e.g. `import picodroid.view.View;`). Importing `android.*` is **not supported** — there is no `android.*` stub jar or alias layer, so `import android.view.View;` will neither compile nor load. Do not add one back, and do not write apps or examples that import `android.*`.

This does not contradict the Project Goal: the goal means the picodroid API is *named* to mirror `android.*` (so `picodroid.view.View` matches `android.view.View` method-for-method and intuition transfers) — it does **not** mean apps import the `android` namespace.

## After Every Code Change

Run these two checks without exception:

### 1. Sim smoke test

```bash
./scripts/sim.sh --app helloworld
./scripts/sim.sh --app benchmark
perl -e 'alarm 5; exec @ARGV' ./scripts/sim.sh --app blinky
```

The blinky app loops forever; `perl -e 'alarm 5; exec @ARGV'` kills it after 5 seconds (macOS has no `timeout` command).
Confirm expected output appears (e.g. `[HelloWorld] Hello, World!`, `[Benchmark] TOTAL: ... ms`, GPIO state changes).

### 2. Pre-commit suite

```bash
./scripts/pre-commit          # after every change
./scripts/pre-commit --full   # before pushing, and before a release
```

Both tiers must end with `==> All checks passed.`

`./scripts/pre-commit` (the default, and what the git hook runs) is scoped to
what actually changed and trimmed to the checks CI does not already cover: the
shadow-twin and cfg-hygiene guards, `hil-tests.conf` drift, `apply_jvm_env`,
markdown lint, and the binary-size ratchet always run, and the formatting,
clippy and firmware-build lanes are selected by which of Rust / Java+Kotlin /
markdown the change touched. Editing anything under `scripts/` promotes the run
to `--full` automatically.

`--full` is the unscoped gate: formatting (Java + Kotlin + `cargo fmt`), clippy
across every board and the host tools, the staged `handle-table-32` and opt-in
`mem-diag` legs, the embedded and flash-gate builds, Java compilation for all
apps, the Java and Kotlin conformance suites, all tests, and the size ratchet on
both boards.

`--list` prints the stages a run would execute; `--serial` runs the lanes one at
a time and streams to stdout, which is what to use when a parallel run fails and
you want readable output. Per-run logs are kept under `build/pre-commit/`.

Do not consider a code change complete until the sim smoke test and
`./scripts/pre-commit` pass; run `--full` before you push.

WiFi-enabled device builds (`testbench_rp2350w`, `pico_enviro_mon_w`) take `PICODROID_WIFI_SSID` / `PICODROID_WIFI_PASS` at build time; local credentials live in the gitignored `.wifi-creds.env` at the repo root.

> **When debugging:** Skip these checks during intermediate debugging steps. Only run them once you are confident the bug is fixed.
>
> **When debugging memory (heap growth, churn, OOM, corruption):** opt-in monitors and offensive checks exist — see `docs/memory-diagnostics.md` (`./scripts/sim.sh --app <app> --mem-diag`).
