# Picodroid Development Guidelines

## Project Goal

Picodroid brings Android-like Java app development to embedded systems. The Java API exposed to developers should stay as close to its Android counterpart as possible — class names, method signatures, semantics, and idioms should match `android.*` so that code and developer intuition transfer directly. When a design choice is forced by embedded constraints prefer the option that preserves the Android-facing API surface, even if the internal implementation diverges.

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

This runs: Java formatting check, `cargo fmt`, clippy (RP2040 + RP2350), embedded build, and all tests. Must end with `==> All checks passed.`

Do not consider a code change complete until both of these pass.

> **When debugging:** Skip these checks during intermediate debugging steps. Only run them once you are confident the bug is fixed.
