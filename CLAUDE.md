# Picodroid Development Guidelines

## After Every Code Change

Run these two checks without exception:

### 1. Sim smoke test
```bash
./scripts/sim.sh --app helloworld
./scripts/sim.sh --app blinky
```
Confirm expected output appears (e.g. `[HelloWorld] Hello, World!`, GPIO state changes).

### 2. Full pre-commit suite
```bash
./scripts/pre-commit
```
This runs: Java formatting check, `cargo fmt`, clippy (RP2040 + RP2350), embedded build, and all tests. Must end with `==> All checks passed.`

Do not consider a code change complete until both of these pass.
