---
title: "Porting Guide: Adding a New MCU to picodroid"
description: "How to bring up a new MCU family against the Picodroid HAL contract."
---

This guide explains the picodroid Hardware Abstraction Layer (HAL) and how to
add support for a new MCU family.

## Architecture overview

Chip-specific code lives under `platforms/<family>/`. Each family is a separate cargo crate and (for ESP) potentially a separate workspace. The shared core that doesn't know what chip it's running on lives in `picodroid-core/`. The HAL surface is governed by **HAL CONTRACT v1** — a documented set of required public symbols and signatures that every family implements identically.

```text
platforms/
  rp/               # RP2040 + RP2350 (Raspberry Pi Pico family)
    src/hal/
      mod.rs            # HAL re-export point + the v1 contract docblock
      contract.rs       # compile-time enforcement of v1 signatures
      rp/               # chip-family implementations
      sim/              # Host simulator (no hardware)
      <your-family>/    # Your new MCU family
  esp/              # ESP32-S3 (Espressif) — separate workspace
mcus/<chip>.toml     # per-chip clock speeds, FreeRTOS config, build args
boards/<board>.toml  # per-board pinout, display, touch, sensors
picodroid-core/      # cross-family shared code (no HAL imports)
```

### HAL CONTRACT v1

The required public symbols and signatures are documented in `platforms/rp/src/hal/mod.rs` (the docblock at the top) and enforced at compile time by `platforms/rp/src/hal/contract.rs`. Drift in any signature breaks `cargo clippy` for both targets.

Symbols are tiered:

- **Always required** (sim + every hardware family): `adc`, `display`, `gpio`, `i2c`, `pwm`, `spi`, `system_clock`, `touch`, `uart`.
- **Hardware-only**: `boot`, `flash`, `pdb_usb`.

A new family port only needs the always-required tier to compile; hardware-only modules can stub-panic until the M2 milestone.

### Family vs. board

The HAL exposes chip-level capability; per-board configuration (display controller, touch controller, SPI bus assignment, etc.) lives in `boards/<board>.toml` and `mcus/<chip>.toml`. A new MCU port typically only needs a HAL implementation; a new board on an already-supported MCU only needs new TOML entries.

The rest of the codebase (`system/`, `pdb/`, `packagemanager/`) calls `crate::hal::uart::init()`, `crate::hal::gpio::set_value()`, etc. without knowing which chip is underneath. This is a zero-cost abstraction: module-level free functions selected at compile time, no trait objects, no dynamic dispatch.

## What a new port must provide

Create a directory `platforms/<family>/src/hal/` containing a `mod.rs` that
declares these public modules:

```rust
// platforms/<family>/src/hal/mod.rs
pub mod adc;
pub mod boot;
pub mod display;
pub mod flash;
pub mod gpio;
pub mod i2c;
pub mod pdb_usb;
pub mod pwm;
pub mod spi;
pub mod system_clock;
pub mod touch;
pub mod uart;
```

Each module must export the functions listed below with the exact signatures.
The simplest way to start is to copy `platforms/rp/src/hal/sim/` and replace the
stubs with real hardware drivers.

### uart.rs

```rust
pub fn init(uart_id: u8);
pub fn reconfigure(uart_id: u8, baudrate: i32, data_size: i32,
                   parity: i32, stop_bits: i32, hw_flow: i32);
pub fn write_byte(uart_id: u8, byte: u8);
pub fn read_byte(uart_id: u8) -> i32;  // returns -1 if RX FIFO empty
```

- `uart_id`: 0 or 1 (UART0 / UART1).
- `init` configures GPIO pins and applies a default 9600 8N1 configuration.
- `write_byte` is blocking (polls TX FIFO).
- `read_byte` is non-blocking (returns -1 when nothing is available).
- The v1 contract (`contract.rs`) enforces only `init`, `write_byte`, and `read_byte`. `reconfigure` is not contract-checked but is needed to back the Java `UartDevice.set*` methods.

### gpio.rs

```rust
pub fn set_direction(pin: u8, direction: i32);
pub fn set_value(pin: u8, high: bool);
```

- `direction`: 1 = output initially high, 2 = output initially low.
- `set_direction` must configure the pin as a GPIO output.

### spi.rs

```rust
use pico_jvm::array_heap::ArrayHeap;

pub fn init(spi_id: u8);
pub fn reconfigure(spi_id: u8, freq_hz: u32, mode: u32);
pub fn transfer(spi_id: u8, tx_idx: u16, rx_idx: u16,
                len: usize, arrays: &mut ArrayHeap) -> i32;
pub fn write(spi_id: u8, data_idx: u16, len: usize,
             arrays: &ArrayHeap) -> i32;
```

- `spi_id`: 0 or 1.
- `mode`: 0-3 (standard SPI CPOL/CPHA modes).
- `transfer` does full-duplex SPI; `write` discards received bytes.
- Array data is accessed via `arrays.load(idx, offset)` / `arrays.store(...)`.
- Return value: number of bytes transferred, or -1 on error.

### i2c.rs

```rust
use pico_jvm::array_heap::ArrayHeap;

pub fn init(i2c_id: u8);
pub fn set_speed(i2c_id: u8, hz: u32);
pub fn write(i2c_id: u8, address: u32, data_idx: u16,
             len: usize, arrays: &ArrayHeap) -> i32;
pub fn read(i2c_id: u8, address: u32, buf_idx: u16,
            len: usize, arrays: &mut ArrayHeap) -> i32;
```

- `i2c_id`: 0 or 1.
- `set_speed`: standard (100 kHz) or fast (400 kHz).
- Return value: number of bytes transferred, or -1 on NACK/abort.
- **v1 contract:** `contract.rs` enforces `init`, `set_speed`, and the slice-based `write_slice` / `read_slice` (see [slice-based I/O](#i2c--slice-based-io) below). The `ArrayHeap`-based `write` / `read` shown here back the Java `I2cDevice` API; native drivers (e.g. BME688) use the slice form.

### pwm.rs

```rust
pub fn init(pin: u8);
pub fn apply(pin: u8, freq_hz: f64, duty_cycle: f64, enabled: bool);
```

- `duty_cycle`: 0.0 to 100.0 (percentage).
- `init` configures the pin for PWM with defaults: 1 kHz, 0% duty, disabled.

### adc.rs

```rust
pub fn init(pin: u8);
pub fn read(pin: u8) -> f64;  // returns voltage in volts
```

- Pins are GPIO numbers (e.g. 26-29 on RP2040).
- `read` performs a single blocking ADC conversion.

### system_clock.rs

```rust
pub fn sleep(ms: u32);
```

- Blocks the calling FreeRTOS task for `ms` milliseconds.
- Use `freertos_rust::CurrentTask::delay()` on real hardware.

### boot.rs

```rust
pub fn clock_init();
pub fn start_tasks(boot_apk: &'static [u8]) -> !;
```

- `clock_init` configures the system clock (PLL, crystal, etc.).
- `start_tasks` creates FreeRTOS tasks and starts the scheduler (never
  returns). On dual-core MCUs (RP2040/RP2350), PDB runs on core 1 and JVM
  on core 0 with core affinity. On single-core MCUs, both tasks run on
  the same core, differentiated by priority only (PDB at higher priority).
- Optional: chip-specific boot blocks (e.g. RP2350's `IMAGE_DEF`).
- Provide a `FreeRTOSConfig.h` in the same directory.

### flash.rs

```rust
pub const PAPK_FLASH_XIP_BASE: usize;
pub const PAPK_FLASH_META_OFFSET: u32;
pub const PAPK_MAX_DATA_SIZE: usize;
pub const PAPK_BOOT_META_SIZE: usize;    // typically 4096
pub const PAPK_FLASH_MAGIC: u32;          // 0x5044_4231 ("PDB1")

pub unsafe fn read_flash_papk() -> Option<&'static [u8]>;
pub unsafe fn flash_erase_papk_region(papk_len: usize);
pub unsafe fn flash_write_page(page_index: u32, data: &[u8; 256]) -> bool;
pub unsafe fn flash_commit_metadata(len: u32);
pub unsafe fn park_for_flash();
pub fn flash_trigger_reset() -> !;
```

- `read_flash_papk`: check the PAPK flash region for a valid install; return
  a `'static` slice pointing into memory-mapped flash, or `None`.
- `flash_erase_papk_region`: erase sectors needed for `papk_len` bytes + metadata.
- `flash_write_page`: write a 256-byte page into the PAPK data region.
- `flash_commit_metadata`: write the PapkBootMeta header (atomic commit).
- `park_for_flash`: on dual-core MCUs, park the calling core in a RAM spin
  loop while the other core writes flash. On single-core MCUs, this is a no-op
  (flash writes simply disable interrupts).
- `flash_trigger_reset`: trigger a full chip reset (typically via watchdog).

All flash write/erase functions must run from RAM (not flash) and may need to
disable XIP. See `platforms/rp/src/hal/rp/flash.rs` for the RP family's approach.

In addition to the PAPK region above, a port that wants to support
`picodroid.io` / `picodroid.content.SharedPreferences` on hardware must reserve a
separate flash region for the LittleFS volume in its linker memory layout
(the RP family's layout is generated at build time by `boards::place_memory_x` in `platforms/rp/build.rs`) and expose it to the
filesystem driver. Sim builds back the same API with a host file image and
do not need this.

### pdb_usb.rs

The PDB (Picodroid Debug Bridge) host transport. It was originally a dedicated
UART (`pdb_uart`); it is now USB CDC-ACM, hence the module name.

```rust
pub fn init();
pub fn queue_read_byte() -> u8;
pub fn queue_read_byte_timeout() -> Option<u8>;
pub fn queue_read_u32_le() -> u32;
pub fn write_bytes(data: &[u8]);
pub fn drain_tx();
```

- `init`: allocate the RX queue, bring up the USB CDC device, enable RX interrupts.
- `queue_read_byte`: blocking read from the ISR-fed RX queue.
- `queue_read_byte_timeout`: 2-second timeout, returns `None` on timeout.
- `write_bytes`: send a response frame back to the host.
- `drain_tx`: spin until the TX path has finished transmitting.
- You must also provide the USB interrupt handler that drains the CDC RX
  endpoint into the queue. The mechanism is chip-specific (e.g. the
  `USBCTRL_IRQ` handler on RP2040/RP2350).

## FreeRTOSConfig.h

Each MCU family provides its own `FreeRTOSConfig.h` in its HAL directory.
Key settings that differ per family:

| Setting | Dual-core (RP) | Single-core (nRF52, STM32) |
|---------|---------------|----------------------------|
| `configCPU_CLOCK_HZ` | 125/150 MHz | varies |
| `configNUMBER_OF_CORES` | 2 | 1 |
| `configUSE_CORE_AFFINITY` | 1 | 0 |
| `configTICK_CORE` | 0 or 1 | N/A |
| `configSMP_SPINLOCK_*` | 26, 27 | N/A |
| `configSUPPORT_PICO_SYNC_INTEROP` | 1 | 0 |
| `configENABLE_FPU` | chip-dependent | chip-dependent |
| `configTOTAL_HEAP_SIZE` | 128 KB | depends on RAM |

Settings that are the same across all families (tick rate, stack sizes, hook
enables, API includes) can be found in the existing RP config at
`platforms/rp/src/hal/rp/FreeRTOSConfig.h`.

## Cargo features

Add your chip and family features to `Cargo.toml`:

```toml
[features]
chip-nrf52840 = ["dep:nrf52840-hal", "family-nrf"]
family-nrf = []
```

Add the HAL crate as an optional, target-gated dependency:

```toml
[target.'cfg(target_arch = "arm")'.dependencies]
nrf52840-hal = { version = "...", optional = true }
```

## HAL dispatch

Add a `#[cfg]` path entry in `platforms/rp/src/hal/mod.rs`:

```rust
#[cfg(all(not(any(feature = "sim", test)), feature = "family-nrf"))]
#[path = "nrf52/mod.rs"]
mod chip;
```

## Build system

Update `build.rs` to handle the new family:

1. **Memory layout**: emit a `memory_<chip>.x` from `build.rs` and select it
   based on `CARGO_FEATURE_CHIP_*`. The RP family generates its layout at build
   time via `boards::place_memory_x` (see `platforms/rp/build.rs`) rather than
   committing a file.
2. **FreeRTOS port**: select the standard Cortex-M4F port
   (`portable/GCC/ARM_CM4F`) instead of the RP-specific SMP ports.
3. **FreeRTOS config**: point `freertos_config()` to `platforms/<family>/src/hal/nrf52/`.
4. **C shim**: the RP family needs `pico_shim_*.c` files (in
   `platforms/rp/src/hal/rp/port/`) that fake the pico-sdk C API expected by the
   RP-specific FreeRTOS SMP ports. These shims are compiled into
   `libfreertos.a` and are never called from Rust — they exist purely to
   satisfy the C linker. Standard Cortex-M FreeRTOS ports (ARM_CM4F, ARM_CM33)
   use CMSIS directly and do **not** need a shim or shadow headers. If your
   MCU's FreeRTOS port depends on a vendor SDK, you may need to provide
   similar stubs in `platforms/<family>/src/hal/<family>/port/`.

## .cargo/config.toml

Add the target entry:

```toml
[target.thumbv7em-none-eabihf]
runner = "probe-rs run --chip nRF52840_xxAA --protocol swd"
linker = "flip-link"
rustflags = [
  "-C", "link-arg=--nmagic",
  "-C", "link-arg=-Tlink.x",
  "-C", "link-arg=-Tdefmt.x",
]
```

## Single-core vs dual-core considerations

picodroid's RP port uses a dual-core architecture: PDB on core 1, JVM on
core 0. This affects `boot.rs` (core affinity) and `flash.rs` (core parking).

On single-core MCUs:

- **Task scheduling**: both PDB and JVM tasks run on the same core. PDB
  preempts JVM via higher FreeRTOS priority.
- **Flash writes**: `park_for_flash()` is a no-op. Flash erase/write simply
  disables interrupts, performs the operation, and re-enables.
- **`CoreCoordinator`**: `request_stop_and_park()` stops the JVM task;
  `wait_for_park()` returns immediately (single core = already "parked").
- **No `configUSE_CORE_AFFINITY`**: omit `.core_affinity()` calls in
  `start_tasks()`.

## board.toml reference

Every physical board ships a `board.toml` under `boards/<name>/`. The build script parses it and emits Rust `cfg`s and `const`s — do not edit `boards/*/mod.rs` to configure a display or sensor, configure it here. All coordinates are RP2040/RP2350 GPIO numbers.

:::tip[App developers: what board.toml means for you]
You don't edit `board.toml` to write an app, but it determines what your app can do on a given board. A few keys are worth knowing: `lv_mem_kb` sets the LVGL render pool (smaller pools cap how many focusable list rows fit — see [Limits & memory budgets](/reference/limits/)); the presence of a `[touch]` section vs. `[[button]]` entries decides whether the board is touch- or button-driven (see [Button-only navigation](/guides/button-navigation/)); `idle_timeout_ms` controls display sleep; and `[jvm]` tunes the heap/GC tradeoff ([JVM tunables](/reference/jvm-tunables/)).
:::

### Top-level properties

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `mcu` | string | yes | `"rp2040"`, `"rp2350"`, or `"esp32s3"` (the schema is family-agnostic). |
| `has_network` | bool | no | If `true`, compiles in the networking stack (FreeRTOS+TCP + driver). |
| `network_type` | string | no | Needed for a working network build when `has_network = true` (not parser-enforced). Only `"cyw43"` is supported today. |
| `lv_dpi` | int | no | Override LVGL's reported DPI (default 130). Used for small-screen boards. |
| `lv_mem_kb` | int | no | LVGL render-pool size in KiB (default 64). |
| `idle_timeout_ms` | int | no | Idle time before the display sleeps (default 60000; `0` disables sleep). Only takes effect on boards with `[[button]]` entries. |
| `linker_script` | string | no | Path to a custom `memory.x` (defaults to `mcus/<family>/<mcu>.x`). |

### `[display]` — display controller (ST7789 over SPI)

| Key | Type | Description |
|-----|------|-------------|
| `driver` | string | Documentation-only; the HAL hardcodes ST7789. |
| `spi_id` | int | SPI peripheral ID (0 or 1). |
| `spi_freq` | int | SPI clock in Hz (e.g. `62500000`). |
| `spi_sck`, `spi_mosi`, `spi_miso` | int | Optional SPI pad overrides; default to the chip's SPI pins (e.g. SPI0 SCK=GP2/MOSI=GP3 on RP2350). The Enviro+ Pack uses these to route SPI0 to GP18/GP19. |
| `pin_dc`, `pin_cs`, `pin_bl` | int | Data/command, chip-select, backlight GPIOs. |
| `pin_rst` | int | Reset pin (optional; some displays don't expose one). |
| `width`, `height` | int | Panel dimensions in pixels (**required** when `[display]` is present). |
| `madctl` | int (hex) | ST7789 memory-access-control register (controls rotation / mirroring). |
| `band_height` | int | LVGL partial-render band in pixels (**required**). |
| `scroll_limit` | int | LVGL scroll hysteresis threshold (**required**). |

Omit the whole `[display]` section for a headless board; the build then falls back to safe 320×240 defaults and leaves `has_display` unset.

### `[touch]` — touch controller (XPT2046 over SPI)

| Key | Type | Description |
|-----|------|-------------|
| `driver` | string | Currently only `"xpt2046"`. |
| `spi_freq` | int | SPI clock in Hz. |
| `pin_cs`, `pin_irq`, `pin_miso` | int | Chip-select, pen-down IRQ, MISO GPIOs. |
| `cal_x_min`, `cal_x_max`, `cal_y_min`, `cal_y_max` | int | Raw ADC bounds from touch calibration. |
| `swap_xy` | bool | Transpose X/Y axes (for rotated panels). |

### `[[sensor]]` — array of environmental sensors

| Key | Type | Description |
|-----|------|-------------|
| `kind` | string | Driver selector: `"bme688"` or `"ltr559"`. |
| `bus` | string | `"I2C0"` or `"I2C1"`. |
| `addr` | int | 7-bit I2C address (decimal or hex). |

Each entry here becomes a `Sensor` visible to [`SensorManager`](/api/sensors/).

### `[[button]]` — array of hardware buttons

| Key | Type | Description |
|-----|------|-------------|
| `pin` | int | GPIO number. |
| `lv_key` | string | One of `"PREV"`, `"NEXT"`, `"ENTER"`, `"ESC"` — drives LVGL focus navigation. |
| `keycode` | int | Android `KeyEvent.KEYCODE_*` value delivered to Java listeners. |

Declaring at least one `[[button]]` enables the idle display-sleep + wake-on-button feature (the sleep delay is `idle_timeout_ms`, default 60 s; set it to `0` to keep the panel always on, as `pico_enviro_mon` does). See [api/ui.md → Key events](/api/ui/#key-events) and the [Button-only navigation](/guides/button-navigation/) guide.

### `[background_pool]` — optional thread-pool tuning

All keys optional; defaults in parentheses.

| Key | Type | Description |
|-----|------|-------------|
| `threads` | int | Worker count (4), range 1..=32. |
| `priority` | int | FreeRTOS BG-tier priority (5), range 1..=10. |
| `stack_bytes` | int | Per-worker stack in bytes (4096). |
| `queue_depth` | int | Shared job queue depth (32). |

Surfaced via [`Executors.backgroundExecutor()`](/api/system/#picodroidconcurrentexecutors).

### `[jvm]` — optional CPU↔memory tradeoff knobs

Five compile-time `pub const`s sourced from this section, all optional. See [JVM tunables](/reference/jvm-tunables/) for the full schema, tuning workflow, and worked recipes.

| Key | Type | Description |
|-----|------|-------------|
| `gc_alloc_threshold` | int | Allocations between auto-GC cycles (256), range 16..=8192. |
| `slot_chunk_shift` | int | Chunk size = `1 << shift` for heap slot storage (6), range 3..=8. |
| `inline_array_data` | int | Array elements held inline rather than in the arena (8), range 0..=32. |
| `activity_stack_depth` | int | Max nested Activities (8), range 1..=32. |
| `pending_op_queue` | int | Max queued startActivity/startService ops per frame (8), range 1..=64. |

## HAL additions (v0.2.0)

The HAL has grown a few modules beyond the original 10-module surface. New boards do not *have* to implement these to boot, but skipping them disables features (button input, sensors, display sleep).

### `gpio.rs` — edge-triggered IRQ + event queue

```rust
pub enum EdgeTrigger { Rising, Falling, Both }
pub struct GpioEvent { pub pin: u8, pub rising: bool }

pub fn init_gpio_irq();                                // idempotent
pub fn enable_edge_irq(pin: u8, edge: EdgeTrigger);
pub fn disable_edge_irq(pin: u8);
pub fn drain_gpio_event() -> Option<GpioEvent>;        // non-blocking
pub fn has_pending_event() -> bool;
pub fn wait_for_button_event();                        // blocks on semaphore
pub fn read(pin: u8) -> bool;                          // synchronous read
```

The ISR enqueues events into a lock-free ring and signals a binary semaphore. `wait_for_button_event` is what the idle-sleep path blocks on.

### `i2c` — slice-based I/O

```rust
pub fn write_slice(i2c_id: u8, addr: u8, data: &[u8])     -> i32;
pub fn read_slice (i2c_id: u8, addr: u8, buf: &mut [u8]) -> i32;
```

Interrupt-driven, 1 s timeout. Returns byte count on success or `-1` on NACK/abort. Complement the existing `ArrayHeap`-based `write` / `read` used by the Java I2C API; native drivers (e.g. the BME688 driver) use the slice form.

### `display.rs` — composite sleep / wake

```rust
pub fn display_sleep();  // backlight off → DISPOFF → SLPIN
pub fn display_wake();   // SLPOUT (120 ms delay) → DISPON → backlight on
```

Called from the lifecycle event loop after `IDLE_TIMEOUT_MS` (the `idle_timeout_ms` board key, default 60 s) with no input, and again on the next GPIO edge. The wake-triggering edges are consumed before they reach LVGL / Java listeners.

## Verification

After implementing your port, run the full pre-commit suite:

```bash
# Sim smoke test (verifies picodroid business logic is not broken)
./scripts/sim.sh --app helloworld
perl -e 'alarm 5; exec @ARGV' ./scripts/sim.sh --app blinky

# Full suite: formatting, clippy (all targets), build, tests
./scripts/pre-commit
```

For on-device testing, flash with your chip's target:

```bash
cargo run --target thumbv7em-none-eabihf \
          --no-default-features --features chip-nrf52840
```

## Reference implementation

The RP family (`platforms/rp/src/hal/rp/`) is the reference implementation. Study these
files for patterns and conventions:

| File | What it demonstrates |
|------|---------------------|
| `uart.rs` | Multi-instance peripheral (UART0/UART1), baud rate calculation, GPIO pin routing |
| `gpio.rs` | Direct register access via PAC, RP2350 ISO bit handling |
| `flash.rs` | XIP-disabled flash operations from RAM, core parking with inline asm |
| `boot.rs` | Dual-core task creation, chip-specific boot blocks |
| `pdb_usb.rs` | USB CDC ISR → FreeRTOS queue pattern, NVIC interrupt setup |
