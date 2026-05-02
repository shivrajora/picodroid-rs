# ESP32-S3 Toolchain Setup

Picodroid targets the **Lilygo T-Deck Plus** (ESP32-S3) alongside its RP2040/RP2350 boards.
ESP32-S3 uses the Xtensa LX7 architecture, which requires a separate Rust compiler fork
maintained by the `esp-rs` team.

## Prerequisites

Install the ESP toolchain alongside your existing ARM toolchain:

```bash
cargo install espup
espup install
```

`espup install` downloads and registers:

- `esp` Rust toolchain (the Xtensa-capable compiler fork)
- `xtensa-esp32s3-none-elf` compilation target
- LLVM/Clang for Xtensa (`ldproxy` linker helper)

After install, source the environment file **in every terminal session** (or add to your shell rc):

```bash
. $HOME/export-esp.sh
```

Install flashing tools:

```bash
cargo install espflash
cargo install ldproxy   # linker proxy required by .cargo/config.toml
```

## Building for T-Deck Plus

With the ESP environment sourced:

```bash
# Build (uses RUSTUP_TOOLCHAIN=esp automatically via espup)
cargo b-tdeck-plus

# Flash + monitor over USB-Serial
cargo r-tdeck-plus
```

Both aliases are defined in `.cargo/config.toml` and expand to:

```bash
cargo build --target xtensa-esp32s3-none-elf --no-default-features --features board-tdeck-plus
```

## rust-analyzer

Add this to your workspace `.vscode/settings.json` (or Neovim LSP config) so the IDE
indexes the T-Deck Plus source tree:

```json
{
  "rust-analyzer.cargo.target": "xtensa-esp32s3-none-elf",
  "rust-analyzer.cargo.noDefaultFeatures": true,
  "rust-analyzer.cargo.features": ["board-tdeck-plus"],
  "rust-analyzer.rustc.source": "discover",
  "rust-analyzer.server.extraEnv": {
    "RUSTUP_TOOLCHAIN": "esp"
  }
}
```

To switch back to RP2350 indexing, change `target` to `thumbv8m.main-none-eabihf` and
`features` to `["board-testbench-rp2350"]`.

## Key differences from RP builds

| Aspect | RP family | ESP32-S3 family |
|---|---|---|
| Rust toolchain | stable | `esp` fork (via espup) |
| Target triple | `thumbv6m/thumbv8m` | `xtensa-esp32s3-none-elf` |
| Linker | `flip-link` | `ldproxy` + `linkall.x` |
| Flash | `probe-rs` / SWD | `espflash` / USB-Serial |
| defmt | `defmt-rtt` via SWD | no-op (stub only; real logger is a future milestone) |
| FreeRTOS | Bundled (freertos-rust-pd) | Bypassed (single-threaded) — Milestone 3 |

## Toolchain versions

As of picodroid v0.9.0:

| Crate | Version | Notes |
|---|---|---|
| xtensa-lx-rt | 0.21 | Entry point + linker scripts (replaces esp-hal until riscv-rt conflict resolved) |

esp-hal 1.1 is deferred: it transitively requires `riscv-rt ^0.16` (via esp-riscv-rt for
RISC-V chips) which conflicts with `rp235x-hal 0.4`'s `riscv-rt ^0.12` in the same workspace.
Once `rp235x-hal` bumps its riscv-rt dep, esp-hal will be added back for real peripheral access.

## Troubleshooting

**`error: toolchain 'esp' is not installed`**
Run `espup install` and `. $HOME/export-esp.sh`.

**`linker 'ldproxy' not found`**
Run `cargo install ldproxy`.

**`error: could not compile esp-hal`**
Ensure you're using the `esp` toolchain, not stable. Check with `rustup show active-toolchain`.

**`espflash` can't find the device**
Confirm the T-Deck Plus USB-C port is connected (the top USB-C port, not the charging port).
On Linux you may need `sudo chmod a+rw /dev/ttyUSB0`.
