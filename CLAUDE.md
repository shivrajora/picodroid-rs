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

### 2. Full pre-commit suite

```bash
./scripts/pre-commit
```

This runs formatting (Java + `cargo fmt`), clippy across every board and the host tools, the embedded and flash-gate builds, the shadow-twin and cfg-hygiene guards, Java compilation for all apps, a langsuite conformance run, and all tests. Must end with `==> All checks passed.`

Do not consider a code change complete until both of these pass.

WiFi-enabled device builds (`testbench_rp2350w`, `pico_enviro_mon_w`) take `PICODROID_WIFI_SSID` / `PICODROID_WIFI_PASS` at build time; local credentials live in the gitignored `.wifi-creds.env` at the repo root.

> **When debugging:** Skip these checks during intermediate debugging steps. Only run them once you are confident the bug is fixed.
>
> **When debugging memory (heap growth, churn, OOM, corruption):** opt-in monitors and offensive checks exist — see `docs/memory-diagnostics.md` (`./scripts/sim.sh --app <app> --mem-diag`).
