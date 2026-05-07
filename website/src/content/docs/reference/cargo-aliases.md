---
title: "Cargo aliases"
description: "Per-board cargo aliases that pick the right target and feature flags."
---

Picodroid intentionally ships no default cargo target — `[build] target = ...`
is not set in `.cargo/config.toml`. Bare `cargo build` therefore errors with
a clear "no target specified" message instead of silently mistyping the build
(the previous default of `thumbv6m-none-eabi` made every RP2350 IDE build
silently wrong).

Pick a board explicitly using one of the aliases below, or use the wrapper
scripts in `scripts/`.

## RP family (RP2040 / RP2350)

| Alias | Equivalent invocation |
|---|---|
| `cargo b-testbench-rp2040` | `cargo build --target thumbv6m-none-eabi --no-default-features --features board-testbench-rp2040` |
| `cargo b-testbench-rp2350` | `cargo build --target thumbv8m.main-none-eabihf --no-default-features --features board-testbench-rp2350` |
| `cargo b-testbench-rp2350w` | `cargo build --target thumbv8m.main-none-eabihf --no-default-features --features board-testbench-rp2350w` |
| `cargo b-pico-enviro-mon` | `cargo build --target thumbv8m.main-none-eabihf --no-default-features --features board-pico-enviro-mon` |
| `cargo b-sim` | `cargo build --no-default-features --features sim,board-testbench-rp2350` (host target) |

## ESP family (ESP32-S3)

The ESP family lives in its own workspace at `platforms/esp/` because `riscv-rt` (pulled in by `rp235x-hal`) conflicts with `xtensa-lx-rt`. Run the ESP aliases from inside that directory or pass `--manifest-path platforms/esp/Cargo.toml`.

| Alias | Equivalent invocation |
|---|---|
| `cargo b-tdeck-plus` | `cargo build --target xtensa-esp32s3-none-elf --no-default-features --features board-tdeck-plus` |
| `cargo r-tdeck-plus` | `cargo run --target xtensa-esp32s3-none-elf --no-default-features --features board-tdeck-plus` (uses `espflash`) |

Requires the `xtensa-esp32s3-none-elf` Rust toolchain installed via `espup`. See [ESP32-S3 toolchain](/reference/esp32s3-toolchain/) for the full setup.

## `r-*` variants

`r-*` variants run `cargo run` instead of `cargo build`. RP boards use the `probe-rs` runner (via `cargo embed`) configured under the matching `[target.*]` block. ESP boards use `espflash` configured the same way.

## Adding a new board

Mechanical: register `b-<board>` and `r-<board>` aliases pointing at the new board feature and matching MCU target triple. For RP, edit `.cargo/config.toml` at the repo root. For ESP, edit `platforms/esp/.cargo/config.toml`.

## rust-analyzer

`rust-analyzer` invokes `cargo` directly without the alias machinery, so it
needs to be told which target and feature set to use for analysis. Set the
following in your editor's workspace settings, swapping the values for
whichever board you are currently working on:

### VS Code (`.vscode/settings.json`)

```json
{
  "rust-analyzer.cargo.target": "thumbv8m.main-none-eabihf",
  "rust-analyzer.cargo.noDefaultFeatures": true,
  "rust-analyzer.cargo.features": ["board-testbench-rp2350"],
  "rust-analyzer.check.extraArgs": [
    "--target", "thumbv8m.main-none-eabihf",
    "--no-default-features",
    "--features", "board-testbench-rp2350"
  ]
}
```

For sim work, set `"rust-analyzer.cargo.features": ["sim", "board-testbench-rp2350"]`
and remove the `target` entries.

### Other editors

Pass the same `--target` / `--no-default-features` / `--features` flags via
your editor's `cargo.extraArgs` (or equivalent) hook.
