# Design + bug: cyw43 PIO transport, so WiFi can run on core 1 — 2026-08-14

**RESOLVED 2026-08-14:** the PIO transport shipped
(`platforms/rp/src/hal/rp/pio_spi.rs`) and the `cyw43` task is pinned to
**core 1** (`boot_tasks.rs`, `.core_affinity(0b10)`). Core 0 is the JVM's.
The history, evidence, and debug recipes below are retained because they are
the durable value of this investigation.

## Status summary

| Item | State |
|---|---|
| Flash/XIP core-1 lockup (RP2350) | **Fixed**, committed `d445785` |
| cyw43 pinned to core 0 | **Removed** — task runs on core 1 |
| Bug A — chip bring-up fails on core 1 (`F2 not ready`) | **Mooted by PIO** — no CPU-timed bus phases remain |
| Bug B — RX never delivers, join stalls | **Gone with the PIO transport** — see outcome below |
| PIO transport | **Done** — Rust, PIO0 SM0 + DMA ch4/5, 37.5 MHz |

## Outcome (what actually shipped)

- **Option 1** as designed: Rust transport in `hal/rp/pio_spi.rs`, exposing
  the six `cyw43_spi_*` functions `#[no_mangle] extern "C"`; the bit-bang
  `cyw43_bus_spi.c` and the dead `port/hardware/pio.h` shim are deleted.
  Raw-PAC register access (per-transfer EXECCTRL wrap rewrites are not
  expressible in rp235x-hal's typed PIO API). Program is pico-sdk's
  `spi_gap01_sample0`, assembled at compile time by `pio::pio_asm!`
  (note: at pio 0.3.0 the macro lives in the `pio` crate, not `pio_proc`).
- **Clock:** 150 MHz / clkdiv 2 / 2 SM-cycles-per-bit = **37.5 MHz** gSPI,
  pico-sdk's shipping rate on RP2350. No fallback divider was needed.
- **Transfer shapes:** the vendored driver only issues `(tx, N, NULL, 0)`
  writes and `(buf, N, buf, N)` reads, so the read path hardcodes a 4-byte
  command phase (X=31, Y=(N-4)*8-1, RX DMA into `buf+4`). A 16-byte aligned
  bounce buffer covers the two unaligned boot-time swap-register transfers.
  Write-only end-of-transfer = DMA done **then** FDEBUG.TXSTALL re-assert.
- **The PRIMASK guard is gone.** Frames complete autonomously under
  preemption; a park/preempt mid-transfer only delays CS deassert past an
  already-completed frame.
- **Bug A** needed no RAM placement at all — nothing timing-critical executes
  from flash anymore. (The unexplained core-0 boot crash of the reverted
  `.data.cyw43_spi` experiment remains unexplained, but no longer matters;
  the shipped image's `.data` is byte-identical in size/VMA to the baseline.)
- **Bug B never reproduced** on the PIO transport: with cyw43 on core 1 and
  blinky loading core 0, DHCP binds in ~4.5 s, `rx_ok` climbs with zero
  drops, host-wake asserts, and `wifi_join_state` sits at its post-join
  terminal value `0x1` (the driver collapses `WIFI_JOIN_STATE_ALL` back to
  `ACTIVE` on completion — `cyw43_ctrl.c:434`; transient AUTH/LINK/KEYED
  bits are only visible mid-join). Read-path timing (candidate 2) was
  evidently the cause; the host-wake gate (candidate 1) and the poll loop
  (candidate 3) were verified healthy via the counters below.
- **Validation on `testbench_rp2350w`:** blinky (core 0 busy, 200+ LED
  lines) → `ip 192.168.4.94`; http_get end-to-end (DNS + TCP,
  `status=200`, 571 body bytes) with its BASE_URL temporarily pointed at a
  real host; 10/10 `pdb install` soak alternating blinky/http_get, WiFi
  re-associating after every runtime flash write.
- **Standing instrumentation** (silent, gdb-read only — never log the hot
  path): `instr_tx_ok/tx_fail`, `instr_rx_ok/…nobuf/…queue/…noiface` in
  `NetworkInterface_CYW43.c`; `instr_hostwake_reads/high` in `cyw43_port.c`;
  `INSTR_CYW43_POLLS` in `wifi_task.rs` (~10/s baseline from the 100 ms
  fallback). These are the first read on any future core-1 RX regression.

## How we got here

`d445785` extended the core-1 flash parker to RP2350, which required
`configRUN_MULTIPLE_PRIORITIES=1`. That flag turned on *real* SMP for the first
time on this chip. Before it, the kernel default of `0` forbade tasks of
**different** priorities from running on both cores simultaneously — so whenever
the priority-22 cyw43 task held core 1, core 0 was **barred from running the
priority-15 JVM**. cyw43 had the machine largely to itself by accident.

Real SMP removed that accident and WiFi broke. Both bugs below were latent the
whole time; nothing about them is new except that they became reachable.

Symptom as first seen: link reports up, endpoint stuck at **0.0.0.0**, no
recovery over 3 minutes — but only when the app keeps core 0 busy. `blinky`
failed 4/4; `netdemo`, which mostly waits, bound normally every time.

## Bug A — chip bring-up corrupts (FIXED, uncommitted)

```
cyw43: F2 not ready
wifi: mac 02:50:49:43:4f:57      <- LAA fallback; chip never answered
wifi: join "CherryJam" failed: -1
```

`STATUS_F2_RX_READY` never asserts, so `cyw43_ensure_up` bails leaving
`cyw43_poll == NULL`, the MAC falls back to a locally-administered address, and
the join fails `-1`. Everything downstream is fallout.

**Cause.** The bit-banged gSPI ran from XIP flash with its clock period spun out
of instruction timing (`SPI_HALF_PERIOD()` = `delay_cycles(8)`). Running the JVM
on core 0 contends for the same XIP cache and QSPI bus, stretching core 1's
frames past what the chip tolerates with CS held low — the same failure mode
`d66882b`'s PRIMASK guard fixed for context switches, arriving from the other
core instead.

**Fix attempted, then REVERTED — do not simply re-apply it.** `CYW43_SPI_RAM_FUNC`
(`__attribute__((section(".data.cyw43_spi"), noinline, optimize("no-var-tracking-assignments"))))`)
on `cyw43_spi_transfer`, `spi_write_word`, `spi_read_word`, `load_be32`,
`store_be32`.

It **worked** with cyw43 on core 1: chip booted with core 0 fully loaded, real
OTP MAC, join requested, `tx_ok=3`, no `F2 not ready`. That is what establishes
the diagnosis above, and it is solid.

It **crashes at boot** with cyw43 on core 0 — the configuration currently in
`main`:

```
Firmware exited unexpectedly: Exception
```

Reproducible; reverting restores a clean boot with working WiFi on the same
hardware (`LED=191`, `ip 192.168.4.94`). Verified it is the fix and not stale
flash state by erasing first, and not a build artifact by stash/unstash A-B.

**Mechanism not established.** What is known:

- Symbols do land in SRAM (`cyw43_spi_transfer` at `0x20000b9c` via `nm`).
- The section is real and loaded — `.data` is `ALLOC, LOAD, CODE` with a flash
  LMA, so the boot copy covers it.
- `.data` grows **1804 → 2252 B** (+448) and its VMA shifts **`0x20000800` →
  `0x20000400`**. That downward VMA move is unexplained and is the most
  interesting thread to pull.
- This board is at **99% RAM** (527768 / 532480, ~4.7 KB free), so it has almost
  no margin for anything that grows a RAM section — though +448 B alone should
  not overflow, and the linker did not error.
- The `size` output the build script prints did **not** change between the two
  builds, so it is not a reliable check here — compare `objdump -h` section
  sizes instead.

Why configuration-dependent? Unknown, and worth understanding before trusting
any RAM-placement approach: the same binary change boots on core 1 and faults on
core 0.

**Implication for PIO.** A PIO transport removes the need for this entirely — the
state machine does not execute CPU instructions, so nothing needs to be
RAM-resident for timing. That is another point in PIO's favour over patching the
bit-bang. If RAM placement is revisited anyway, use the exact section the linker
copies, verify with `objdump -h` rather than the build script's RAM figure, and
test **both** core assignments.

## Bug B — RX never delivers (OPEN, primary work)

With the chip up and cyw43 on core 1, read from a live failing device by gdb:

```
tx_ok=3   tx_fail=0                      <- DHCP DISCOVER goes out fine
rx_ok=0   nobuf=0  queuefull=0           <- RX callback NEVER fires; not dropping
join_state=0x1                           <- ACTIVE only, never AUTH/LINK/KEYED
poll=0x10070f7d                          <- driver up, poll fn installed
```

So the join stalls because association events never arrive; DHCP failing is
downstream collateral, not the bug. We can **drive** the bus but never **sample**
anything back — while register reads demonstrably work, since the F2 gate now
passes.

Join-state bits (from the 2026-08 bring-up): `0x1` ACTIVE, `+0x200` AUTH,
`+0x400` LINK, `+0x800` KEYED; kind 3 = NONET.

### Not yet root-caused. Leading candidates

1. **Host-wake gate.** GP24 is *both* the shared data line and the active-high
   `WL_HOST_WAKE` IRQ. If the "chip has work" read is wrong when sampled from
   core 1, the driver never fetches packets. `d66882b` already fixed one
   inversion here (`WL_IRQ` active-low vs `WL_HOST_WAKE` active-high) and made
   the line idle as input with a pull-**down**.
2. **Read-path timing.** TX drives, RX samples — an asymmetry consistent with a
   residual turnaround/setup violation that register reads (short, infrequent)
   survive but packet reads do not.
3. **Poll not actually running.** The poll loop has a 100 ms timeout fallback, so
   it should poll regardless of the IRQ gate. Worth proving rather than assuming.

**First diagnostic in the new session:** add a counter for *poll invocations* and
for the host-wake pin read, then compare core-1 vs core-0. That single
measurement separates candidate 3 from 1 and 2, and 1 from 2.

## Why PIO is the structural answer

Bit-banging is not a design choice — it is forced by the wiring, and it is the
root reason bus timing is hostage to CPU execution at all.

The CYW43439 speaks **gSPI: half-duplex on a single shared data pin**:

```c
#define CYW43_PIN_WL_DATA_OUT   (24)
#define CYW43_PIN_WL_DATA_IN    (24)   // same pin — host drives, then turns around
#define CYW43_PIN_WL_HOST_WAKE  (24)   // and it doubles as the IRQ
#define CYW43_PIN_WL_CS         (25)
#define CYW43_PIN_WL_CLK        (29)
#define CYW43_PIN_WL_REG_ON     (23)
```

The RP2350 PL022 SPI block is full-duplex 4-wire with separate MOSI/MISO. It
cannot express "drive this pin for the 4-byte command phase, tri-state it, then
sample the chip's response on the same wire", and these pins are not routable to
its function set anyway. **Hardware SPI is not an option on this board.**

pico-sdk solves it with **PIO** (`cyw43_bus_pio_spi.c` + `cyw43_bus_pio_spi.pio`).
A PIO state machine clocks the bus from its own divider and performs the pin-
direction turnaround in the program itself. It is **indifferent to what either
CPU core is doing** — which is precisely the property this bug needs.

PIO would have prevented Bug A outright, and removes the timing candidate (2) for
Bug B. It does not necessarily fix a host-wake gating bug (candidate 1), so Bug B
must still be diagnosed on its own terms — do not assume PIO is a blanket fix.

## Why we cannot just use pico-sdk's implementation

We can use its *method*; we cannot link its *code*. Three reasons:

1. **This project deliberately does not link pico-sdk.** It builds on
   `rp235x-hal` (Rust) plus FreeRTOS, with `pico_shim_rp2350.c` reimplementing
   the handful of pico-sdk symbols the FreeRTOS SMP port needs — its own header
   says "without linking against the full pico-sdk". Pulling in pico-sdk now
   means either vendoring a large subtree or dragging in its CMake build.
2. **`cyw43_bus_pio_spi.c` is written against pico-sdk HAL APIs** —
   `hardware/pio.h`, `hardware/dma.h`, `hardware/gpio.h`, `hardware/clocks.h` —
   none of which exist here. Porting the file means reimplementing those calls
   against our HAL either way, at which point we are writing our own transport
   with pico-sdk as the reference.
3. **Its `.pio` source is built by `pioasm`**, a host tool in the pico-sdk build,
   generating a `.pio.h`. We have no pioasm step. (We *do* have the `pio-proc`
   crate, which assembles PIO at compile time from Rust — see below.)

So: **port the algorithm, not the file.** Treat pico-sdk's
`cyw43_bus_pio_spi.c` / `.pio` as the normative reference to match behaviour
against, and fetch it in the new session to read directly rather than trusting
this summary.

## Scope of work

### The seam is small — 5 functions

The driver only needs (`vendor/cyw43-driver/src/cyw43_spi.h`):

```c
int  cyw43_spi_init(cyw43_int_t *self);
void cyw43_spi_deinit(cyw43_int_t *self);
void cyw43_spi_gpio_setup(void);
void cyw43_spi_reset(void);
int  cyw43_spi_transfer(cyw43_int_t *self, const uint8_t *tx, size_t tx_length,
                        uint8_t *rx, size_t rx_length);
```

(plus `cyw43_spi_set_polarity` in our impl). Everything above this line — the
driver, the FreeRTOS+TCP glue, `NetworkInterface_CYW43.c` — is untouched. A PIO
transport is a drop-in replacement for one file.

### Resources are free

- **All three PIO blocks are unused.** Nothing in `platforms/rp/src` touches PIO
  today (verified by grep). PIO0 is the natural choice.
- **`rp235x-hal` 0.4 ships `pio.rs`**, and `pio` / `pio-proc` / `pio-parser` are
  already in the cargo cache as transitive deps — so `pio_proc::pio_asm!` can
  assemble the program at compile time with no new build step.
- **SPI1 is the display**, SPI0 is free on `testbench_rp2350w`. Not needed, but
  relevant if anyone reconsiders hardware SPI.
- `platforms/rp/src/hal/rp/dma.rs` exists but is display-oriented
  (`start_write(spi_id, data)`); PIO DMA would need its own channels.

### Implementation options

**Option 1 — Rust transport (recommended).** Implement the PIO program with
`pio_proc::pio_asm!` and drive it via `rp235x-hal`'s PIO API, exposing the 5
functions as `#[no_mangle] extern "C"`. Pros: uses the HAL we already depend on,
type-safe state-machine setup, no new build tooling, matches how the rest of the
port is written. Cons: PIO program must be hand-translated from pico-sdk's
`.pio`; the `cyw43_int_t *self` parameter needs an opaque type on the Rust side.

**Option 2 — C transport against a PIO shim.** Keep `cyw43_bus_pio_spi.c` close
to upstream and add PIO register access to `pico_shim_rp2350.c`. Pros: stays
line-by-line comparable with pico-sdk, easiest to re-sync on upstream changes.
Cons: grows the shim substantially (pio_sm_config, instruction encoding, DMA);
duplicates what `rp235x-hal` already provides.

**Option 3 — keep bit-bang, RAM-resident only.** Already done (Bug A fix). Not
sufficient on its own — Bug B remains, and the timing fragility is mitigated, not
removed.

Recommendation: **Option 1**, with pico-sdk's `.pio` as the reference to match
semantics against (bit order, turnaround point, clock polarity, side-set usage).

### Things the PIO program must get right

Verify each against upstream rather than inferring from our bit-bang, which has
its own hard-won corrections baked in (see `d66882b`):

- Buffer bytes travel in order, **MSB-first within each byte**; 32-bit words
  serialize buffer-order MSB-first in both directions.
- **Reads sample the data line BEFORE the clock pulse.** The chip presents each
  response bit before the clock edge — bit 0 is already valid during the
  turnaround after the command word.
- The command phase is the **first 4 bytes only**, then the shared DATA line
  tri-states.
- The DATA/IRQ line must idle as **input with a pull-down** — it doubles as the
  active-high host-wake interrupt, and a pull-up fakes "work pending".
- Clock rate: every known-good transport runs this chip at **10–33 MHz**; at
  sub-MHz the gSPI interface has been observed to stop decoding commands
  entirely. Our bit-bang deliberately sits in the low-MHz range.
- `SIO GPIO_IN` is at **+0x004** on RP2350 (+0x008 is `GPIO_HI_IN`) — a past bug.

### Acceptance criteria

With `cyw43` pinned to **core 1** (`.core_affinity(0b10)` in `boot_tasks.rs`) and
`configRUN_MULTIPLE_PRIORITIES=1`:

1. `blinky` installed and running (core 0 genuinely busy) → `net: up, ip <real>`,
   not `0.0.0.0`. This is the case that fails today; it is the bar.
2. `netdemo` joins + DHCP in 5–10 s and completes its TCP work.
3. 10/10 `pdb install` cycles with WiFi associated, link recovering after.
4. `join_state` reaches AUTH|LINK|KEYED, `rx_ok` climbs.
5. Full HIL suite on `testbench_rp2350` still 49 PASS / 0 FAIL (the parker change
   must not regress).
6. `./scripts/pre-commit` green.

## Reproduction — read this before touching hardware

The repro is easy to get **falsely passing**. Three traps cost real time:

**1. The app must actually be running.** On ARM the APK is *not* embedded in the
ELF (`build_support/papk.rs`) — it lives in a separate flash region.
`probe-rs run <elf>` flashes firmware only. If no valid PAPK is present the
firmware logs `FrameworkVersionMismatch` and **runs no app at all**, leaving core
0 idle — which is exactly the condition under which the bug does not reproduce.
Several "passing" runs were this artifact.

Correct sequence:

```bash
pkill -x probe-rs                      # -x, never -f: `-f probe-rs` kills your own shell
timeout 90 probe-rs run --chip RP235x --protocol swd \
  target/thumbv8m.main-none-eabihf/release/picodroid   # flash firmware
pkill -x probe-rs
target/x86_64-unknown-linux-gnu/debug/pdb install build/apks/blinky.papk
timeout 60 probe-rs attach --chip RP235x --protocol swd \
  target/thumbv8m.main-none-eabihf/release/picodroid > run.log 2>&1 </dev/null
```

**2. Verify which app ran, with the right pattern.** Device RTT uses `Tag: msg`,
not the sim's `[Tag] msg`. And `grep -i LED` also matches "**l**oad**ed**" in
`cyw43 loaded ok` — which produced convincing but meaningless counts. Use:

```bash
grep -cE '\] LED:' run.log     # >0 means blinky really ran
grep -oE 'ip [0-9.]+' run.log  # 0.0.0.0 = the bug; 192.168.x.x = healthy
```

**3. Do not log in the hot path.** Adding `defmt` calls to the TX/RX path
perturbs the timing enough to make the bug vanish — an early instrumented build
"fixed" it. Use silent counters and read them over gdb.

Build (credentials are baked in at build time; never commit an image built with
real ones — `target/` is gitignored):

```bash
env PICODROID_WIFI_SSID='…' PICODROID_WIFI_PASS='…' \
    PICODROID_APK_PATH="$PWD/build/apks/blinky.papk" \
  cargo build -p picodroid --release --target thumbv8m.main-none-eabihf \
    --no-default-features --features board-testbench-rp2350w
```

## Debug recipes

**Counters over gdb (zero perturbation).** Declare `volatile uint32_t` globals in
`NetworkInterface_CYW43.c`, then:

```bash
probe-rs gdb --chip RP235x --protocol swd &     # then, in a batch script:
#   target extended-remote :1337
#   printf "tx_ok=%u rx_ok=%u\n", instr_tx_ok, instr_rx_ok
#   printf "join=0x%x poll=%p\n", cyw43_state.wifi_join_state, cyw43_poll
gdb-multiarch -q -batch -x c.gdb target/thumbv8m.main-none-eabihf/release/picodroid
```

**Confirm RAM placement** of anything timing-critical:

```bash
arm-none-eabi-nm target/…/picodroid | grep -iE 'spi_write_word|cyw43_spi_transfer'
# 2000xxxx = SRAM (good), 10xxxxxx = flash (still XIP-dependent)
```

**Chip-state readbacks beat log-guessing.** GET ioctls via
`cyw43_ll_ioctl(&cyw43_state.cyw43_ll, raw_cmd<<1 | is_set, len, buf, 0)`:
`GET_SSID=25` returns the *associated* SSID (empty = not associated, not "not
stored"), `GET_BSSID=23` (`-17` = not associated), iovar 262 GET `"clmver"` /
`"country"` (country `"#n"` = regulatory never set → joins silently NONET).

**Other hardware gotchas.** A locked-up core will not answer reads to its own SCS
(`0xE000ED28`) — read its stacked exception frame from the *other* core, SRAM is
shared. `probe-rs` loses its RTT control block if the device reboots underneath
it (a `pdb install` does), producing a garbage backtrace that is not a device
fault. Leftover `probe-rs` holds the USB claim → "Failed to open probe".

## Current state

Everything described in this doc has landed: the PIO transport
(`hal/rp/pio_spi.rs`), the core-1 pin (`boot_tasks.rs`), and the counters
(permanent — see the Outcome section at the top). The gdb recipes above read
them by name; `INSTR_CYW43_POLLS` is a Rust `AtomicU32`, so cast it in gdb
batch scripts (`*(unsigned int *)&INSTR_CYW43_POLLS`).

## Related

- `docs/bugs-rp2040-flash-2026-08-01.md` — the flash parker work that exposed
  all of this; its "RP2350 recurrence" section covers `d445785`.
- `docs/networking-followups-2026-08.md` — NET-1..NET-9 backlog.
- `d66882b` — the WiFi bring-up commit; its gSPI corrections are the accumulated
  wire-protocol knowledge a PIO program must reproduce.
