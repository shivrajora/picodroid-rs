---
title: "ESP32-S3 quickstart"
description: "Bring up Picodroid on the Lilygo T-Deck Plus (ESP32-S3) — Milestone 1 (compile-only)."
---

Picodroid is in Milestone 1 (M1) bring-up on Espressif ESP32-S3 with the **Lilygo T-Deck Plus** as the reference board.

:::caution[M1 status: compile-only]
M1 produces a valid Xtensa ELF for the T-Deck Plus, but **FreeRTOS, networking, display, and the LVGL stack are no-ops at this milestone** and land in M3. The firmware will boot to a stub on-device. Use the [host simulator](/get-started/simulator/) for end-to-end app work today; this page is for working on the platform port itself.
:::

## Prerequisites

You need everything from the [RP build & flash](/get-started/build/) prerequisites (Rust toolchain, JDK, formatter), plus the Espressif Xtensa toolchain.

```bash
# Install espup (manages the xtensa-esp32s3 toolchain)
cargo install espup --locked
espup install

# Source the env-vars before each shell session
. ~/export-esp.sh   # path printed by `espup install`

# espflash for flashing the ESP32-S3
cargo install espflash --locked
```

For the full toolchain notes — what versions land where, how `ldproxy` is wired, why there's no RTT — see the [ESP32-S3 toolchain reference](/reference/esp32s3-toolchain/).

## Compile

```bash
# Picks up the b-tdeck-plus alias under .cargo/config.toml.
cargo b-tdeck-plus
```

This compiles the entire `picodroid-esp` workspace against `xtensa-esp32s3-none-elf`. It does **not** flash.

## Flash (M1)

Flashing works with espflash, but the on-device firmware is a stub:

```bash
cargo r-tdeck-plus
```

You'll see initial boot messages over the serial console, then nothing. That's expected at M1.

## What's deferred to later milestones

| Subsystem | Status | Notes |
|---|---|---|
| Compile to Xtensa ELF | M1 ✓ | This page. |
| FreeRTOS scheduler | M3 | Single-threaded `xtensa-lx-rt` today. `freertos-rust` not yet wired. |
| Display (ST7789) | M3 | LVGL stack present but the SPI driver is unbound. |
| Networking | M3 | `lwIP` / `esp-wifi` integration pending; `esp-hal` 1.1 dependency staging is the current blocker. |
| Logging | M2 | No RTT on Xtensa; `defmt` will pipe over UART instead. |

For day-to-day app work on a T-Deck Plus, follow the RP path on the simulator and revisit this page when M2 / M3 milestones land.

## Next steps

- [ESP32-S3 toolchain](/reference/esp32s3-toolchain/) — full toolchain reference (versions, environment, troubleshooting).
- [Cargo aliases](/reference/cargo-aliases/) — the per-board `b-` / `r-` cargo alias pattern, including the ESP entries.
- [Porting guide](/reference/porting-guide/) — HAL contract v1 and the `platforms/<family>/` layout, if you're adding a new family.
