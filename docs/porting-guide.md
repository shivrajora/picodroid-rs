# Porting Guide: Adding a New MCU to picodroid

This guide explains the picodroid Hardware Abstraction Layer (HAL) and how to
add support for a new MCU family.

## Architecture overview

All chip-specific code lives in `src/hal/`. A single `#[cfg]` dispatch in
`src/hal/mod.rs` selects the active chip family at compile time:

```
src/hal/
  mod.rs            # feature-gated dispatch
  rp/               # RP2040 + RP2350 (Raspberry Pi Pico family)
  sim/              # Host simulator (no hardware)
  <your-family>/    # Your new MCU family goes here
```

The rest of the codebase (`system/`, `pdb/`, `packagemanager/`) calls
`crate::hal::uart::init()`, `crate::hal::gpio::set_value()`, etc. without
knowing which chip is underneath. This is a zero-cost abstraction: module-level
free functions selected at compile time, no trait objects, no dynamic dispatch.

## What a new port must provide

Create a directory `src/hal/<family>/` containing a `mod.rs` that declares
these ten public modules:

```rust
// src/hal/<family>/mod.rs
pub mod adc;
pub mod boot;
pub mod flash;
pub mod gpio;
pub mod i2c;
pub mod pdb_uart;
pub mod pwm;
pub mod spi;
pub mod system_clock;
pub mod uart;
```

Each module must export the functions listed below with the exact signatures.
The simplest way to start is to copy `src/hal/sim/` and replace the stubs with
real hardware drivers.

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
disable XIP. See `src/hal/rp/flash.rs` for the RP family's approach.

### pdb_uart.rs

```rust
pub fn init();
pub fn queue_read_byte() -> u8;
pub fn queue_read_byte_timeout() -> Option<u8>;
pub fn queue_read_u32_le() -> u32;
pub fn drain_tx();
```

- `init`: allocate the RX queue, configure the PDB UART, enable RX interrupts.
- `queue_read_byte`: blocking read from the ISR-fed RX queue.
- `queue_read_byte_timeout`: 2-second timeout, returns `None` on timeout.
- `drain_tx`: spin until the TX shift register has finished transmitting.
- You must also provide a `#[no_mangle] extern "C"` ISR that drains the UART
  RX FIFO into the queue. The ISR name is chip-specific (e.g. `UART1_IRQ` on
  RP, `UARTE0_UART0` on nRF52).

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
`src/hal/rp/FreeRTOSConfig.h`.

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

Add a `#[cfg]` path entry in `src/hal/mod.rs`:

```rust
#[cfg(all(not(any(feature = "sim", test)), feature = "family-nrf"))]
#[path = "nrf52/mod.rs"]
mod chip;
```

## Build system

Update `build.rs` to handle the new family:

1. **Memory layout**: add `memory_nrf52840.x` at the repo root and select it
   based on `CARGO_FEATURE_CHIP_NRF52840`.
2. **FreeRTOS port**: select the standard Cortex-M4F port
   (`portable/GCC/ARM_CM4F`) instead of the RP-specific SMP ports.
3. **FreeRTOS config**: point `freertos_config()` to `src/hal/nrf52/`.
4. **C shim**: the RP family needs `pico_shim_*.c` files (in
   `src/hal/rp/port/`) that fake the pico-sdk C API expected by the
   RP-specific FreeRTOS SMP ports. These shims are compiled into
   `libfreertos.a` and are never called from Rust — they exist purely to
   satisfy the C linker. Standard Cortex-M FreeRTOS ports (ARM_CM4F, ARM_CM33)
   use CMSIS directly and do **not** need a shim or shadow headers. If your
   MCU's FreeRTOS port depends on a vendor SDK, you may need to provide
   similar stubs in `src/hal/<family>/port/`.

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

The RP family (`src/hal/rp/`) is the reference implementation. Study these
files for patterns and conventions:

| File | What it demonstrates |
|------|---------------------|
| `uart.rs` | Multi-instance peripheral (UART0/UART1), baud rate calculation, GPIO pin routing |
| `gpio.rs` | Direct register access via PAC, RP2350 ISO bit handling |
| `flash.rs` | XIP-disabled flash operations from RAM, core parking with inline asm |
| `boot.rs` | Dual-core task creation, chip-specific boot blocks |
| `pdb_uart.rs` | ISR → FreeRTOS queue pattern, NVIC interrupt setup |
