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
| `cargo b-testbench-rp2040` | `cargo build -p picodroid --target thumbv6m-none-eabi --no-default-features --features board-testbench-rp2040` |
| `cargo b-testbench-rp2350` | `cargo build -p picodroid --target thumbv8m.main-none-eabihf --no-default-features --features board-testbench-rp2350` |
| `cargo b-testbench-rp2350w` | `cargo build -p picodroid --target thumbv8m.main-none-eabihf --no-default-features --features board-testbench-rp2350w` |
| `cargo b-pico-enviro-mon` | `cargo build -p picodroid --target thumbv8m.main-none-eabihf --no-default-features --features board-pico-enviro-mon` |
| `cargo b-pico-touch-kit` | `cargo build -p picodroid --target thumbv8m.main-none-eabihf --no-default-features --features board-pico-touch-kit` |
| `cargo b-sim` | `cargo build -p picodroid --no-default-features --features sim,board-testbench-rp2350` (host target) |

`pico_enviro_mon_w` and `pico_display2_w` have no alias. Build them through the scripts
(`./scripts/build.sh --board pico_display2_w`), or spell the invocation out with
`--features board-pico-enviro-mon-w` / `board-pico-display2-w` and the RP2350 target.

## What an alias does not do

An alias is the bare `cargo` invocation, which makes it the right tool for a compile check and
for rust-analyzer, and the wrong one for an image you mean to boot. `scripts/build.sh` and
`scripts/flash.sh` do four things around the same command:

- They build the app's PAPK and the board's system apps first and pass the PAPK as
  `PICODROID_APK_PATH`. Without that variable the build embeds no app and an empty framework
  class table, so the firmware links and boots with no Java classes at all.
- They pass `--config profile.dev.debug-assertions=false` and
  `--config profile.dev.overflow-checks=false` to every firmware build, and
  `--config profile.release.lto=false` on the RP2040. A debug build without the first two is
  about 41 KB larger.
- They add the `line-numbers` feature to a debug build on the RP2350 boards (never on the
  RP2040, never to `--release`; `PICODROID_LINE_NUMBERS=0|1` overrides) and pack the PAPK with
  its line tables to match.
- They print the flash, RAM and main-stack figures of the linked image and fail a build whose
  main stack is under the 8,192-byte floor.

## `r-*` variants

`r-*` variants run `cargo run` instead of `cargo build`. RP boards use the `probe-rs` runner configured as `runner = ...` under the matching `[target.*]` block in `.cargo/config.toml`.

Those blocks are keyed by `cfg`, not by target triple. One table,
`cfg(all(target_arch = "arm", target_os = "none"))`, carries the linker (`flip-link`) and the
link arguments for both chips; two more add `target_abi = "eabi"` (RP2040, `probe-rs run --chip
RP2040`) and `target_abi = "eabihf"` (RP2350, `probe-rs run --chip RP235x`) for the runner.
Cargo reads a table named by the dotted triple `thumbv8m.main-none-eabihf` only unquoted up to
1.98 and only quoted from nightly 1.101, so either spelling dropped the RP2350 link arguments
on one toolchain; a `cfg` key has no dots. A runner goes through `probe-rs` directly and takes
no [bench lease](/project/contributing/#sharing-the-bench); on a shared bench use
`scripts/flash.sh`.

## Adding a new board

Mechanical: register `b-<board>` and `r-<board>` aliases pointing at the new board feature and matching MCU target triple. For RP, edit `.cargo/config.toml` at the repo root. The linker, link arguments and runner come from the `cfg` tables and need no entry per board.

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
