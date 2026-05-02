# Cargo aliases

Picodroid intentionally ships no default cargo target — `[build] target = ...`
is not set in `.cargo/config.toml`. Bare `cargo build` therefore errors with
a clear "no target specified" message instead of silently mistyping the build
(the previous default of `thumbv6m-none-eabi` made every RP2350 IDE build
silently wrong).

Pick a board explicitly using one of the aliases below, or use the wrapper
scripts in `scripts/`.

## Aliases

| Alias | Equivalent invocation |
|---|---|
| `cargo b-testbench-rp2040` | `cargo build --target thumbv6m-none-eabi --no-default-features --features board-testbench-rp2040` |
| `cargo b-testbench-rp2350` | `cargo build --target thumbv8m.main-none-eabihf --no-default-features --features board-testbench-rp2350` |
| `cargo b-testbench-rp2350w` | `cargo build --target thumbv8m.main-none-eabihf --no-default-features --features board-testbench-rp2350w` |
| `cargo b-pico-enviro-mon` | `cargo build --target thumbv8m.main-none-eabihf --no-default-features --features board-pico-enviro-mon` |
| `cargo b-sim` | `cargo build --no-default-features --features sim,board-testbench-rp2350` (host target) |

`r-*` variants run `cargo run` instead of `cargo build` — these use the
`probe-rs` runner configured under the matching `[target.*]` block.

Adding a new board is mechanical: register `b-<board>` and `r-<board>` aliases
pointing at the new board feature and matching MCU target triple.

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
