# Simulator ↔ MCU Parity Audit

Audited 2026-07-18 on `main` (2e8bbbc), RP2350 primary / RP2040 secondary.
Scope: `platforms/rp` (the ESP32-S3 platform is a Milestone-1 bootstrap with no real
display path and is excluded; see §6).

## 1. Thesis and method

The simulator is only worth having if a bug that would appear on hardware appears in the
simulator first, and a bug that does not appear on hardware does not appear in the
simulator. **Every divergence between the two is a simulator bug, even when the
simulator's behavior is "better." Host-only headroom is a defect, not a convenience.**

This is not hypothetical for this repo. `platforms/rp/mcus/rp/FreeRTOSConfig.h:31-52`
records two production incidents of exactly this failure class: gcstress (2026-04-22) and
picoenvmon (2026-05-10) both booted in the sim and died on hardware with
`alloc 90112 bytes failed` against a fragmented device heap. Both were mitigated by
growing the device arena (256→384→416 KB) rather than by making the simulator honest.
The config comment itself states the diagnosis: *"the budget shortfall is FreeRTOS
overhead (TCBs + task stacks + queues, ~25-30 KB) eating into the heap on hardware that
sim doesn't see."*

Confidence labels used throughout:

- **V (verified)** — read directly from source semantics, or measured by an experiment in
  Appendix A.
- **D (derived)** — computed from verified facts (e.g. a struct size derived from layout
  rules) but not independently measured.
- **I (inferred)** — believed from context; the register row names the experiment that
  would settle it.

Severity is impact-based, independent of tier: **S1** — the sim materially lies (a
program passes in sim and fails on device, or vice versa); **S2** — numbers are wrong but
direction is preserved; **S3** — cosmetic, latency-only, or latent. Classification:
**IS** — intentional and safe (no parity impact); **IPB** — intentional but
parity-breaking; **ACC** — accidental.

## 2. Divergence register

Sim builds are `--features sim,<board>` on the host triple with the HAL swapped by
`#[path]` (`platforms/rp/src/hal/mod.rs:118-124`); everything not listed below is shared
code, byte-identical by construction. Rows reference fixes in §4 (M-memory, P-perf,
G-graphics, X-cross-cutting).

### Memory / allocator (MEM)

| ID | Divergence (sim / device) | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| MEM-01 | Sim heap = `AtomicUsize` byte counter over glibc (`sim_allocator.rs:156-218`); device = FreeRTOS heap_4 first-fit free list w/ coalescing over a fixed arena (`build_support/freertos.rs:56`). No arena, no contiguity, no fragmentation in sim | IPB | App OOMs on device via fragmentation ("free bytes but no contiguous block") at a point where sim sails through — the exact recorded 2026 incident class | **S1** | V | M1 |
| MEM-02 | Sim counts raw `layout.size()`; device pays +8 B header, 8 B round-up, 16 B min block per alloc | IPB | Undercount — measured only **+0.26 % at peak** for benchmark/picoenvmon (V5), so this alone is minor; header modeling without an arena is not worth doing | S3 | V (V5) | M1 |
| MEM-03 | Sim heap **unlimited by default** (`PICODROID_HEAP_LIMIT_KB` unset → `usize::MAX`, `sim_allocator.rs:126-137`; `sim.sh` only sets it with `-l`) vs device hard 416 KB (RP2350) / 128 KB (RP2040) | IPB | Default sim run can never OOM; all day-to-day development happens with infinite memory | **S1** | V | M2 |
| MEM-04 | FreeRTOS TCBs + all task stacks + queues allocated from the same device arena (`configSUPPORT_DYNAMIC_ALLOCATION 1`); sim models none of it. V4 measured **~82-85 KB** of non-app overhead (416 KB arena → 336.5 KB free with blinky idle) — ~3× the "~25-30 KB" config-comment estimate | IPB | Effective JVM budget on RP2350 is **~331 KB, not 416 KB**; sim budget is the full cap. Root cause of the 2026-05-10 +32 KB heap bump | **S1** | V (V4) | M4 |
| MEM-05 | `freertos-rust` allocator passes only `size` to `pvPortMalloc`, dropping `layout.align()`; heap_4 returns 8-aligned always. Sim honors any alignment via glibc | ACC | An align>16 allocation would be *misaligned on device only*. **V5 measured zero align>8 allocations** in benchmark + picoenvmon, so latent today | S3 | V (V5) | M1 (sim aborts loudly on align>8) |
| MEM-06 | Emergency GC (`need_gc` on allocation failure) fires on heap_4 exhaustion/fragmentation on device vs byte-cap (or never, per MEM-03) in sim; threshold GC (256 allocs) is parity-identical shared code | IPB | Different emergency-collection points in app terms; consequence of MEM-01/03, disappears with M1+M2 | S2 | V | M1+M2 |
| MEM-07 | Device task stacks are KB-sized arena allocations with overflow hooks (`hal/rp/boot.rs:125-131`); sim threads get host ~8 MB stacks. JVM call frames are heap-allocated either way (iterative trampoline, `interpreter/mod.rs:410-413`), so *Java* recursion depth parity holds; only native-stack overflow differs | IPB | Deep native recursion or big stack locals overflow on device only | S3 | V | register-only |
| MEM-08 | Sim main-stack conditions differ from device: firmware BSS is 509.6 KB of 520 KB RAM leaving ~10 KB Cortex-M main-stack headroom (measured at build, `arm-none-eabi-size`), thinner than the ~28 KB the FreeRTOSConfig comment claims | — | Device-side robustness observation surfaced by the audit (stale comment); not a sim divergence per se | S3 | V | doc fix |

### Object model (OBJ)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| OBJ-01 | `Option<JvmObject>` = **24 B host / 12 B device** (V1-measured; `Box<[Value]>` fat pointer). Object-slot chunks (`ChunkedSlots`, 64/chunk): 1536 B host vs 768 B device | IPB | Sim's real memory per object is 2× device's; under an arena (M1) sim would OOM earlier than device | S2 | V (V1) | M5 (reporting) / M6 (deletion) |
| OBJ-02 | `Frame` = **80 B host / 40 B device** (V1) — per-call-stack-entry cost doubles in sim | IPB | Same as OBJ-01 for deep call stacks | S2 | V (V1) | M5/M6 |
| OBJ-03 | `StringTable.ptrs: Vec<*const u8>` — 8 B/entry host vs 4 B device (`jvm/src/heap.rs:21`) | IPB | Same, for string-heavy apps | S3 | V | M5/M6 |
| OBJ-04 | `Value` = 16 B, references = u16 indices into side tables — **identical on all targets** (V1: 16/16/16) | IS | None — this is the load-bearing good design; JVM references never carry pointer width | — | V (V1) | — |
| OBJ-05 | `Option<JvmArray>` = 40 B, `ArrayData` = 36 B on *all three* targets (V1) — inline `[i32;8]`/arena offsets, no pointers | IS | None | — | V (V1) | — |
| OBJ-06 | `ObjectHeap::live_bytes()` / `ArrayHeap::live_bytes()` use host `size_of` (`object_heap/mod.rs:515`, `array_heap.rs:248`) → `Runtime.usedMemory()` reports host-inflated numbers in sim | ACC | An app tuning against `usedMemory()` in sim sees ~2× per-object cost vs device | S2 | V | M5 |

### Threading (THR)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| THR-01 | `Thread.start()` in sim prints a warning and **never runs the Runnable** (`native_handler/os.rs:111-126`); device spawns a real FreeRTOS task with a fresh `Jvm` on the shared heap (16 KB stack from the arena) | IPB (documented) | Any threaded app is untestable in sim: its logic silently doesn't execute, its allocations never happen, its bugs (races, ordering, heap pressure) are invisible | **S1** | V | M7 |
| THR-02 | `BackgroundExecutor`: device = pre-spawned FreeRTOS worker pool with own `Jvm`s (`executors/background_pool.rs:24-132`); sim = `submit()` re-queued onto the main/UI queue (`:134-149`) | IPB | Background work serializes with UI in sim: no interleaving, no parallel heap pressure, latency hidden | **S1** | V | M7 |
| THR-03 | `synchronized` monitors are real recursive mutexes in both (sim compiles the `family-rp` path) but sim is effectively single-threaded, so contention/deadlock never occurs | IPB | Deadlocks reachable on device cannot manifest in sim (follows from THR-01/02) | S2 | V | follows M7 |
| THR-04 | Device FreeRTOS config declares SMP 2 cores (`configNUMBER_OF_CORES 2`) while heap safety rests on a "one JVM task at a time" single-core-pinning argument (`native_handler/concurrent.rs:34-59`, `hal/rp/boot.rs:120`) | — | Device-side soundness question flagged by the audit, not fully traced; if JVM tasks can truly interleave across cores, the sim (and the heap!) model is weaker than assumed | S2 | I | investigate (X1) |

### Time / tick (TIM)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| TIM-01 | LVGL tick = fixed 16 ms step on both; backing = FreeRTOS software timer (device) vs `std::thread` + `Instant` pacing (sim) (`executors/tick_source.rs:25-128`). Animation *phase* is deterministic and parity-identical; wall-clock pacing differs | IS/IPB | Frame-count-deterministic behavior is a genuine parity asset; only real-time pacing diverges | S3 | V | — |
| TIM-02 | `elapsed_realtime_nanos`: hardware TIMER from boot vs host `Instant` from **first call** (`hal/sim/system_clock.rs`) | ACC | Small time-base skew: sim t=0 is first-use, not boot | S3 | V | one-line fix (init epoch at sim main start) |
| TIM-03 | `System.currentTimeMillis` = uptime (not epoch) on **both** targets (`native_handler/os.rs:20-23`) | IS | Android-fidelity quirk, but parity-consistent — register note only | — | V | — |
| TIM-04 | `SimDelay::delay_ns` is a no-op; device busy-waits on the cycle counter | IPB | Driver timing paths run instantaneous in sim (mostly device-only code anyway) | S3 | V | register-only |
| TIM-05 | Tick/sensor callback cadence differs in wall-clock terms → the shared `alloc_count` GC threshold (256) is crossed at different app-visible moments | IPB | GC pauses land at different points in app terms even though trigger logic is identical | S3 | D | P1 counters make it observable |

### APK / packaging (APK)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| APK-01 | Device APK lives in flash (XIP, `&'static`, zero heap; `build_support/papk.rs:403-450`); sim reads it from disk and `Box::leak`s it **charged to the heap counter** (`papk.rs:336-359`) — V3 log shows the 40,004-byte picoenvmon APK inside the `post-jvm-new` delta | ACC | Sim overcharges the cap by the full APK size (~40 KB for picoenvmon); under M2 default caps this is pure error | S2 | V (V3) | M3 |
| APK-02 | Asset `lv_image_dsc_t` descriptors are `Box::leak`ed by *shared* code (`graphics/assets.rs:86`) — device pays them from the arena, sim counts them | IS | Parity-consistent; asset *pixel data* rides on APK-01's placement | — | V | — |
| APK-03 | Framework `.class` bytes are `include_bytes!` rodata on both | IS | None | — | V | — |
| APK-04 | Sim deliberately omits cargo rerun tracking for the APK (runtime load); device re-links the flash image | IS | Build-freshness semantics only; no runtime divergence | — | V | — |

### Build / profiles (BLD)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| BLD-01 | Device dev-profile firmware disables `debug-assertions` + `overflow-checks` to fit RP2040 flash (`scripts/lib.sh:245-255`); sim dev builds keep both on. **CI is unaffected** — sim-run.sh and HIL both use `--release`, where profiles agree | IPB | A debug-assert/overflow panic can fire in local sim runs but be compiled out of the firmware being debugged — confusing, but errs toward catching bugs | S3 | V | policy: parity/bench lanes always `--release` (P2) |
| BLD-02 | Sim builds keep `family-rp` **active** (board feature chain), so `not(feature="sim")`-style gates must be spelled explicitly; `all(not(sim), family-rp)` patterns are correct but fragile | ACC-hazard | A future gate written as `not(family-rp)` silently includes device-only code in sim (or vice versa); already footnoted in `fs/mod.rs:168-170` | S3 | V | X2 (clippy-style grep in pre-commit) |
| BLD-03 | ESP is a separate workspace with its own lockfile; `--workspace` runs miss it | IS | Covered explicitly by `test.sh:44-47` | — | V | — |

### Display / graphics (DSP, LVG)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| DSP-01 | Render path **shared end-to-end**: one `lv_conf.h` (RGB565, `LV_COLOR_16_SWAP 1`, SW renderer), same partial-band draw buffer, same `flush_cb`; sim converts the finished BE-RGB565 band to ARGB8888 into a host `FRAMEBUF`, minifb 2× upscale strictly after (`hal/sim/display.rs:145-175,101-105`) | IS | The framebuffer bytes the harness should hash are device-identical by construction — the graphics tier's foundation | — | V | G1 exploits this |
| DSP-02 | ST7789 command stream not modeled: sim `set_window` stores 4 ints; device sends CASET/RASET/RAMWR + init (COLMOD/MADCTL/INVON, `drivers/st7789.rs:102-156`) | IPB | Wrong MADCTL rotation or inversion looks correct in sim, wrong on glass | S3 | V | G2 (cmd log), else register-only |
| DSP-03 | Pixel transport cost: device pushes ~12.8 KB bands over 62.5 MHz SPI via DMA with completion blocking (`hal/rp/spi/mod.rs:364-401`); sim `write_pixels` is a memcpy-speed conversion | IPB | Render-bound benchmarks in sim omit the dominant device cost; graphicsbench numbers not comparable across environments | S2 | V | P2 ratio-tracking; register the gap |
| DSP-04 | Sim cannot tear (atomic full-FB blit at 60 fps); device has no TE sync and can | IPB | Tearing artifacts invisible in sim | S3 | V | register-only |
| DSP-05 | Sim RGB565→ARGB conversion + minifb blit sit *outside* any shared code path — a sim-side conversion bug shows wrong colors in the window while the actual band bytes are right | ACC | Developer misdiagnoses correct device output from a wrong sim window (or vice versa) | S3 | V | G1 hashes upstream of it; eyeball remains for the window itself |
| LVG-01 | Board LVGL config injected as compile-time `-D` from board.toml (`lv_dpi`, `lv_mem_kb`): enviro = 166 dpi / **48 KB** `LV_MEM_SIZE`, testbench = defaults 130 dpi / **64 KB**. CI sim lane builds **only testbench** (`sim-run.sh:129`) | ACC (CI wiring) | The real app's LVGL heap ceiling is never CI-simulated; known symptom class: >12 focusable list rows hangs the 48 KB renderer — reproducible in sim only if the right board is built. V3 verified `sim.sh -b pico_enviro_mon` boots fine today | **S1** | V (V3) | G3 (board matrix) |
| LVG-02 | LVGL's own pool (`LV_MEM_SIZE`) is a static C array on both targets — **outside** both the device arena and the sim counter; a same-sized separate pool on both sides | IS | Parity-consistent as-is; note it when reasoning about "total RAM" (it's why sim heap floors sit far below device arena needs) | — | V | — |
| LVG-03 | `-fshort-enums` ABI compensation for host LVGL FFI (`build_support/lvgl.rs:67-75`) | IS | Deliberate parity fix; without it, struct layouts diverge | — | V | — |

### Input (TCH)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| TCH-01 | Sim synthesizes XPT2046 12-bit ADC codes from mouse/FIFO and runs them through the **same driver** (median filter, calibration, ±2 LSB jitter, `hal/sim/touch.rs:57-204`); buttons converge on the same `GpioEvent` queue (`hal/sim/gpio.rs:97-102` vs `hal/rp/gpio.rs:185-249`) | IS | Downstream input pipeline byte-identical; a genuine parity asset | — | V | — |
| TCH-02 | 4-point touch calibration compiled out of sim entirely (`lvgl/calibration.rs`) | IPB | Calibration UI/logic untestable in sim | S3 | V | register-only |
| TCH-03 | Real-world electrical noise (phantom IRQ edges — the GP15 incident) only approximated by injected sequences | IPB | IRQ-storm/bounce classes need HIL or scripted-noise injection | S3 | V | register-only |

### Peripherals / HAL stubs (HAL)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| HAL-01 | Sensors: sim emits synthetic triangle waves (BME688 22 °C ± 0.5 etc., LTR559 300 lx ± 50; `sensors.rs:432-441,539-546`); device drives real I²C hardware | IPB | Values differ (fine); *cadence* and event-delivery code is shared — value realism is the only loss | S3 | V | register-only |
| HAL-02 | ADC constant 1.65 V; PWM/backlight/boot/delay println-or-no-op stubs | IS | Expected HAL stubbing, contract-enforced | S3 | V | — |
| HAL-03 | UART `read_byte` always −1 in sim (no input path); device reads real UART | ACC | Any UART-consuming app silently gets no data in sim | S2 | V | wire FIFO control channel to UART (backlog) |
| HAL-04 | `packagemanager`, `pdb`, PIO, boot handlers compiled out of sim *and* tests | IPB | PDB install/stop-JVM paths (a known 6-bug area) have zero sim coverage — HIL-only | S2 | V | register-only (HIL owns it) |
| HAL-05 | LVGL widget handles: 32-bit device = raw pointer cast, **no invalidation** — deleted-handle use dangles into freed LVGL memory; 64-bit sim = 4096-entry indirection table nulled on delete + opt-in `PICODROID_HANDLE_SANITIZER` abort (`handle_table.rs:16-91`) | IPB | Use-after-delete hangs the device but is silently absorbed (or cleanly aborted, with sanitizer) in sim — a recorded HW-only bug class (animation-engine incident) | **S1** | V | X3: sanitizer on by default in sim + CI |

### Filesystem (FS)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| FS-01 | LittleFS storage: flash driver vs host-file image (0xFF-filled, byte-compatible; `fs/storage_host.rs`); same LittleFS core | IS | Image-level parity is good; a device flash dump is loadable in sim | — | V | — |
| FS-02 | Device FS ops hop through a core-0-pinned worker task (queue + semaphore); sim runs the closure synchronously under a mutex (`fs/mod.rs:31-174`) | IPB | Ordering/latency differences around FS access; no known symptom | S3 | V | register-only |

### Logging / diagnostics (DBG)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| DBG-01 | defmt/RTT (`Tag: msg`) vs stdout `[Tag] msg`; level ladder vs level-agnostic println | IS | Test patterns already normalize across both (`hil-tests.conf` categories) | — | V | — |
| DBG-02 | Crash shape: panic-probe/HardFault/bkpt vs std unwind/SIGSEGV; `check_no_crash` greps both vocabularies | IS | — | — | V | — |
| DBG-03 | Sim-only env knobs (HEADLESS, PERFECT_TOUCH, TAP_DEBUG, CTRL_FIFO, SANITIZER, SLOW_HANDLER_MS, SIM_FS*) have no device counterpart | IS | Knobs only relax/instrument the sim; SANITIZER should be *on* in parity lanes (X3) | — | V | — |

### Performance measurement (PERF)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| PERF-01 | No deterministic work counters exist on either side (`insn_count: u8` only batches interrupt checks, `interpreter/mod.rs:333-339`); all benchmarks time wall-clock | ACC | A host-measured "optimization" can regress the device; nothing detects it | S2 | V | P1 |
| PERF-02 | perfbench and graphicsbench are not in `hil-tests.conf`: their SCOREs are never captured, compared, or regression-gated anywhere | ACC | No perf history exists on either environment | S2 | V | P2 |
| PERF-03 | Sim GC/alloc elasticity: benchmark completes at 80 KB cap but peaks at 267 KB uncapped (V2/V5) — threshold GC lets garbage accumulate when memory is infinite | IPB | "Peak heap" from a default (uncapped) sim run wildly overstates minimal footprint; developers reading it size hardware wrong | S2 | V (V2/V5) | M2 makes caps the default |

### Board coverage (BRD)

| ID | Divergence | Class | Symptom | Sev | Conf | Fix |
|---|---|---|---|---|---|---|
| BRD-01 | CI sim lane: only `board-testbench-rp2350`. Enviro (buttons-only, no touch, 240×240, 48 KB LVGL) never CI-simulated (see LVG-01); RP2040-class boards never sim'd at all | ACC | Board-conditional code (`has_buttons`-only paths, per-board tunables) reaches hardware without ever running in sim CI | **S1** | V (V3) | G3 |

## 3. Ranked findings (S1s, in order of expected pain)

**1. MEM-01 + MEM-03 + MEM-04 — the sim heap is a fiction (three compounding lies).**
By default the sim has infinite memory; when capped, it meters bytes with no arena, no
contiguity, and no FreeRTOS share. The measured distance between the two accountings is
stark: picoenvmon boots in the sim under a **~97 KB** raw-counter cap (V2) while the same
app needed the device arena raised to **416 KB** to boot reliably — a >4× gap composed of
fragmentation slack, FreeRTOS overhead, transient allocation peaks, and post-boot growth
that the counter never sees. The repo's own history (two `alloc 90112 bytes failed`
incidents) shows this gap is where real hardware failures live. V4 puts a hard number on
one component: FreeRTOS structures and task stacks eat **~85 KB** of the 416 KB arena —
triple the config comment's estimate — so even a developer who conscientiously runs
`sim.sh -l 416` is testing against a budget that does not exist. V5 adds a sharp detail:
modeling heap_4's per-allocation headers changes the counter by only **+0.26 % at peak** —
so half-measures (metering + headers, without an arena) are demonstrably not worth
building. Either the sim runs a real first-fit arena (M1) or it keeps lying.

**2. THR-01/THR-02 — threaded apps are silently not tested.**
`Thread.start()` warns and drops the Runnable; background executors fold into the UI
queue. An app using either runs *differently in kind* in the sim — its worker logic never
executes at all. This is the purest violation of the thesis in the codebase, and it also
distorts memory parity (device charges 16 KB stack + TCB per Java thread; sim charges
nothing, and the thread's allocations never happen).

**3. LVG-01 + BRD-01 — CI simulates a board nobody ships.**
The only board CI ever simulates is the 64 KB-LVGL/touch testbench, while the real app
runs on a 48 KB-LVGL/buttons-only enviro board. The known ">12 focusable rows hangs the
renderer" class lives exactly in that per-board delta. V3 confirmed the enviro board
boots headless under sim *today* — the entire fix is CI wiring, not code.

**4. HAL-05 — the sim structurally hides use-after-delete widget bugs.**
On 64-bit hosts a deleted LVGL handle resolves to a nulled table slot; on 32-bit hardware
it dangles into freed memory. This asymmetry has already produced a hardware-only hang
(animation-engine incident). The sanitizer that closes the gap exists but is opt-in;
nothing in CI runs it.

**5. PERF-03 / OBJ-01 — the numbers developers do see are misleading.**
Uncapped sim runs report peaks ~3.3× above the app's true floor (267 KB vs 80 KB for
benchmark, V2/V5), and `Runtime.usedMemory()` reports host-doubled object sizes (24 B vs
12 B slots, V1). Both numbers point the wrong direction for anyone sizing hardware.

## 4. Fix designs

Preference order per the thesis: delete the divergence; only where it must remain,
compensate loudly and say why. "Couple-modules" fixes need no further approval;
ask-first items are marked.

### Memory tier

- **M3 — stop charging the APK to the heap (trivial, first).** Wrap the generated sim
  `apk_data()` body (`build_support/papk.rs:346-358`) in `sim_allocator::bypass()` —
  models XIP flash, where the device APK actually lives. One generated-string change.
- **M2-interim — cap ON by default.** `const DEVICE_HEAP_BYTES` (416 K / 128 K) selected
  by `chip-rp2350`/`chip-rp2040` feature in the sim allocator, cross-checked against
  `FreeRTOSConfig.h` by a grep test; `sim.sh -l` / `PICODROID_HEAP_LIMIT_KB` become
  overrides. Uncapped runs require an explicit `-l 0`. V2 says current apps fit with wide
  margin — nothing breaks.
- **M1 — the flagship: a real arena (Rust port of heap_4 with 32-bit arithmetic).**
  New `platforms/rp/src/sim_heap4.rs`: `#[repr(align(8))] static ARENA` of
  `DEVICE_HEAP_BYTES`; in-arena 8-byte block headers (`{next_off: u32, size_and_flag:
  u32}` — u32 deliberately, because compiling the vendored C on a 64-bit host doubles
  `BlockLink_t` and changes every accounting constant); faithful first-fit, split iff
  remainder > 16, address-ordered insert with two-sided coalescing; `HeapStats` mirror of
  `vPortGetHeapStats` as the harness's snapshot payload. Routing: **global-through-arena
  is correct** (the device FreeRTOS heap serves *all* firmware allocations too), with
  `bypass()` reserved for genuinely host-only sources (minifb — already; the M3 APK;
  `std::thread` spawn internals; FIFO/control threads; stdio). Pre-main allocations pass
  through until an `ARMED` flag flips in sim main; `dealloc` routes by pointer-range
  (in-arena vs host), which also retires the fragile "balance rule" in today's bypass
  doc. Alignment: mirror the device — 8-byte only; `align>8` **aborts loudly** naming the
  freertos-rust align-drop hazard (V5 measured zero such allocations today, so this
  costs nothing and arms a tripwire). Locking: one process-wide mutex standing in for
  `vTaskSuspendAll`. Validation: unit tests transcribed from heap_4 semantics + the V6
  device oracle trace (Appendix A) replayed against the port — stats streams must match
  op-for-op. *Known consequence:* host-inflated JVM structures (OBJ-01/02) occupy real
  arena space, so sim OOMs somewhat earlier than device until M6 lands — strict-direction
  (conservative) and accepted; decided at check-in.
- **M4 — boot-overhead pre-charge from a shared budget table.** New
  `hal/boot_budget.rs`: `BOOT_TASKS: &[BootTask { name, stack_words, .. }]` consumed by
  *both* device boot (`hal/rp/boot.rs` / `os.rs` stack literals become these consts —
  single source by construction) and sim boot, which performs **real arena allocations**
  in boot order (modeling the long-lived low-address blocks first-fit behavior depends
  on), TCB size calibrated by V4. `Thread.start` charges its 16 KB at spawn (pairs with
  M7). A HIL assertion (`modeled ≈ measured free-after-boot ± 2 KB`) turns drift into CI
  failure.
- **M5 — device-truth reporting.** Replace host `size_of` in
  `ObjectHeap::live_bytes`/`ArrayHeap::live_bytes`/string accounting with explicit device
  constants (12/40/…, V1 numbers), each pinned by
  `#[cfg(target_pointer_width = "32")] const _: () = assert!(size_of::<T>() == N)` so
  every device build re-verifies them forever. `Runtime.usedMemory()` then reports device
  numbers in sim. Two files.
- **M2-final (ask-first).** Single-source `heap_kb` in `mcus/rp/*.toml` → build.rs emits
  the C `configTOTAL_HEAP_SIZE` define and a rustc-env for M1/M2, retiring the duplicated
  constant. Touches build_support + header + tomls.
- **M7 — Thread.start runs for real (ask-first).** Sim spawns a host thread mirroring
  the device closure (fresh `Jvm` on the shared heap), charges 16 KB + TCB from the
  arena at spawn, frees on exit, under a process-wide JVM lock released at the device's
  yield points (sleep, queue waits, tick boundaries) to approximate single-core
  preemption. Requires auditing sim-HAL single-threaded statics. Same pass upgrades
  `background_pool` to real workers. Fallback if declined: `parity-strict` mode makes
  `Thread.start` a hard failure so parity CI can never silently pass a threaded app.
- **M6 — 32-bit-clean object layout (ask-first, last, own design).** Replace
  `Box<[Value]>`/`*const u8` with u32 offsets into heap-owned storage (the ChunkedSlots
  pattern), making host sizes ≡ device sizes and deleting OBJ-01/02/03 outright rather
  than compensating. jvm-crate-wide.
- **X1 — investigate THR-04** (SMP-2-core config vs single-core heap-safety argument) on
  the device side; outcome may add a register row or a config change.
- **X2 — cfg-gating lint**: pre-commit grep for `not(feature = "family-rp")`-style gates
  that should name `sim` explicitly (BLD-02 hazard).
- **X3 — handle sanitizer on by default** in sim.sh and CI sim lanes (HAL-05); opt-out
  env stays for the rare legitimate case.

### Performance tier (P)

- **P1 — deterministic counters, asserted equal.** `parity-metrics` feature on both
  builds: u64 total bytecode dispatches (extend the existing u8 batching at
  `interpreter/mod.rs:333-339` — `+256` per wrap, exact remainder at read), alloc_count,
  GC cycles + dispatch-index at each GC, bands/bytes flushed. One
  `parity_checkpoint!(label)` macro emits a single greppable line (defmt on device,
  stdout in sim) consumed by the existing `check_patterns` machinery. Counter equality is
  the primary cross-environment assertion — where counts diverge, a real divergence
  (usually memory or threading) exists; wall-clock never enters the comparison.
- **P2 — ratio tracking for wall-clock.** benchmark + perfbench + graphicsbench run
  nightly on sim (`--release`, headless — policy) and HIL; SCORE/TOTAL lines appended to
  `bench/parity/history.csv` by `scripts/parity-bench.sh`; alert when device/sim ratio
  drifts >30 % from its trailing median. Absolute prediction is explicitly a non-goal
  (§6). Rejected alternative: cycle/DMA cost modeling — effort out of proportion to
  benchmarking value; DSP-03 stays registered instead.

### Graphics tier (G)

- **G1 — band-stream CRC at the shared flush seam.** `parity-fbhash` feature: CRC32 of
  `px_map` per flush, logged as `(x0,y0,x1,y1,crc)` — computed on the BE-RGB565 bytes
  *before* the sim's ARGB conversion, so sim and device hash identical data by
  construction (DSP-01). Device side over RTT; sim side stdout + an `fbhash` control-FIFO
  verb. Compare sequences textually with existing tooling. Scenes: graphicsbench first
  (fixed-step tick ⇒ deterministic frame count), then first-N-frames of shipping apps.
  This deliberately sidesteps golden-image brittleness: the load-bearing comparison is
  sim-vs-device at the same LVGL revision, which survives theme/font bumps.
- **G2 — command-stream log (cheap).** Feature-gated log of `set_window`/MADCTL-relevant
  state in the sim display so rotation/inversion regressions are diffable; full ST7789
  state modeling rejected (DSP-02 stays S3).
- **G3 — CI board matrix.** sim-run.sh gains a board dimension: testbench-rp2350 +
  pico-enviro-mon (V3-verified). Closes LVG-01/BRD-01 with script-only changes.

### Sequencing

No-ask (≤2 modules each): M3 → M2-interim → M1 → M4 → M5, then G3, X2, X3, P1, P2, G1,
G2. Ask-first, each separately: M2-final, M7, X1, M6 (last, own design doc). The perf and
graphics tiers (P/G) start only after check-in approves them.

## 5. Parity harness

Everything reuses the existing text-pattern machinery (`scripts/lib.sh::check_patterns`,
`hil-tests.conf`, sim-run.sh headless lane, hil-run.sh probe-rs RTT capture) — no new
comparison infrastructure.

1. **Heap snapshots at checkpoints.** M1's arena exposes a `vPortGetHeapStats` mirror;
   the device answers the same query via pdb sysmon (`xPortGetFreeHeapSize` /
   `xPortGetMinimumEverFreeHeapSize`, already bound). A `parity_checkpoint!(label)` line
   carries `free / min_ever / allocs / frees` on both sides; CI compares the sequences.
   The M4 boot-budget assertion (`modeled boot overhead ≈ device free-after-boot ± 2 KB`)
   runs in the HIL lane and fails on drift — V4's measurement becomes a permanent test.
2. **Allocation traces.** The V6 oracle (Appendix A) is the acceptance test for M1's
   arena: replaying the same LCG op sequence must reproduce the device's free/min_ever
   curve *exactly* at every logged op. Future allocator changes re-run it on HIL nightly.
3. **Deterministic counters (P1)** asserted equal per benchmark scene; any inequality is
   a divergence detector (memory, threading, or dispatch), not a perf signal.
4. **Framebuffer hashes (G1).** Per-flush band CRC32 sequences, sim stdout vs device
   RTT, on graphicsbench + first-N-frames scenes. Device capture rides the existing
   hil-run flow; scene length stays inside the RTT capture window (or the parity category
   extends it).
5. **Wall-clock ratios (P2)** in `bench/parity/history.csv`, alarmed at >30 % drift from
   trailing median — trend detection, never absolute prediction.

## 6. Honest limits — what the simulator will still not tell you

- **Timing is not modeled and will not be.** Cortex-M instruction timing, XIP cache
  misses, DMA/SPI transfer time (DSP-03), ISR jitter: a sim wall-clock number predicts
  nothing about device wall-clock. Only counter equality (P1) and tracked ratios (P2)
  are meaningful. Anyone benchmarking "how fast" must use hardware.
- **Preemption granularity.** Even after M7, a host GIL released at yield points is
  coarser than FreeRTOS time-slicing; instruction-level interleavings (and the races
  only they expose) remain HIL-only. THR-04's dual-core soundness question compounds
  this until X1 resolves it.
- **Host object inflation until M6.** With M1's arena, sim OOMs strictly *earlier* than
  device (24 B vs 12 B object slots) — conservative, but real capacity tuning near the
  ceiling needs hardware until M6 deletes the difference.
- **Address determinism is not claimed.** M1 reproduces arena *layout behavior*
  (first-fit, fragmentation), not identical pointer values; traces compare stats
  streams, not addresses.
- **Electrical reality.** Sensor values are synthetic (HAL-01), touch noise is modeled
  jitter (TCH-01), IRQ storms/bounce (TCH-03), display artifacts on glass (DSP-02/04),
  and UART input (HAL-03) are HIL-only.
- **PDB / package-manager / flash tooling** (HAL-04) has zero sim coverage by design.
- **RTT capture window** after flashing bounds device-side trace length; long-scene
  graphics comparisons need the capture window extended.
- **ESP32-S3**: the platform is a bootstrap (no real display, no FreeRTOS); nothing in
  this audit's harness applies there yet.

## Appendix A — experiment log (all run 2026-07-18, RP2350 testbench rig, `main` @ 2e8bbbc)

**V1 — type sizes** (stable-toolchain const-probe, `cargo check` per target; probes
reverted):

| Type | thumbv8m (RP2350) | thumbv6m (RP2040) | x86_64 host |
|---|---|---|---|
| `Option<JvmObject>` | **12** | 12 | **24** |
| `Option<JvmArray>` | 40 | 40 | 40 |
| `ArrayData` | 36 | 36 | 36 |
| `Frame` | **40** | 40 | **80** |
| `Value` | 16 | 16 | 16 |

**V2 — sim cap bisection** (headless, `sim.sh -l`): picoenvmon (enviro board) boots at
≥98 KB, OOMs at ≤96 KB → boot floor ≈ 97 KB raw-counter. benchmark (testbench) completes
at ≥80 KB (`TOTAL: 734 ms`), OOMs at ≤72 KB → full-run floor ≈ 76-80 KB. OOM path
verified (`[sim] OOM: tried N B…` + emergency GC keeps apps alive under pressure).

**V3 — enviro board sim smoke**: `sim.sh -b pico_enviro_mon -a picoenvmon` headless
boots to `[PicoEnvMon] Home.onCreate`; buttons configured, no-touch path taken. Boot
checkpoints show the 40,004 B APK charged inside `post-jvm-new` (+46,148 B).

**V4 — device heap occupancy** (instrumented then reverted; blinky on testbench_rp2350):
pre-tasks free = 413,384 B (12,600 B consumed by clock/fs init — sim's post-fs-init
checkpoint reads 14,228 B, a close cross-check). Steady-state via `pdb sysmon` at 85 s
uptime: **free 336,512 B of 425,984 B → 89,472 B consumed**; min-ever 336,424 B. Task
set (10): jvm (8192 w = 32 KB stack), pdb (2048 w), fs (2048 w), 4× jvm-bg (4 KB each),
Tmr Svc + 2× IDLE (128 w each). Stack total ≈ 67 KB; + TCBs/queues + 12.6 KB pre-task
init ≈ **~82-85 KB non-app overhead**, vs the "~25-30 KB" comment estimate (which
evidently excluded task stacks). **Effective JVM budget on RP2350 ≈ 331 KB, not
416 KB.** Also observed: firmware BSS 509.6 KB of 520 KB RAM → ~10 KB main-stack
headroom (MEM-08, config comment stale).

**V5 — allocation-shape trace** (temp heap_4-cost model in `CappedAllocator`, reverted):
benchmark peak raw 267,182 B vs heap_4-modeled 267,888 B (**+0.26 %**); picoenvmon boot
67,032 vs 67,144 B (+0.17 %). Baseline-only skew (548 raw vs 1,600 modeled — tiny
startup allocs) washes out at scale. **`align > 8` allocations: 0** in both apps
(MEM-05 latent). Conclusion: header modeling without an arena is worthless;
fragmentation and boot overhead are the whole story.

**V6 — heap_4 oracle trace** (deterministic; captured over RTT, patch saved at
`scratchpad/v6-heaptrace.patch` for Stage-D replay): 400 ops, 64 slots, LCG
`x = x*1664525 + 1013904223` seed `0x1234_5678`; call order: 1 draw for slot index, 2
draws for size (`16 + (r%512)*((r%4)+1)`) only on alloc. Curve (free/min_ever every 40
ops): 402392/400344 → 395216/391288 → 392584/391288 → 391840/390784 → 383880/383880 →
386704/381416 → 392104/381416 → 393912/381416 → 388472/381416 → 391960/381416; final
free 413,384 = starting free exactly (balanced), global min-ever 381,416. M1's Rust port
must reproduce every logged pair bit-for-bit.

**Environment**: probe-rs CMSIS-DAP `2e8a:000c`; no gcc-multilib (32-bit C differential
oracle not available — device-as-oracle used instead); stable toolchain only.

## Appendix B — implementation log (2026-07-18/19, same session as the audit)

Landed after CHECK-IN 1 approved all recommended options (fixes M3→M2i→M1→M4→M5,
strict early-OOM, parity-strict threading with M7 deferred, both P and G tiers):

- **M3** — sim APK load wrapped in `bypass()` (generated `apk_data()`,
  `build_support/papk.rs`; ESP crate gets a documented no-op since it has no capped
  allocator). Log line now says `flash-modeled: uncounted`.
- **M2-interim** — cap ON by default: `DEVICE_HEAP_BYTES` (416 K/128 K by chip feature)
  in `sim_allocator.rs`; `-l 0` = uncapped; pre-commit cross-checks the constants
  against FreeRTOSConfig.h.
- **M1** — `platforms/rp/src/sim_heap4.rs`: bit-faithful heap_4 port (u32 in-arena
  headers, first-fit, split-iff->16, two-sided coalescing, `HeapStats` mirror).
  `sim_allocator.rs` reworked: `arm()` at sim main, pre-arm passthrough,
  pointer-range dealloc routing (bypass balance rule retired), align>8 loud abort,
  OOM diagnostics now report largest-free-block/fragmentation. **The V6 hardware
  oracle replays bit-for-bit** (`replays_rp2350_hardware_oracle_trace`), plus 7
  semantics tests. Host thread internals (lvgl-tick, control-channel) bypassed.
  Gotcha for posterity: leaked pre-charge allocations must pass through
  `black_box` — optimized builds legally elide unused mallocs.
- **M4** — `boot_budget.rs`: shared task table (device spawn sites consume the
  constants; sim performs real arena allocations in boot order + charges
  `Thread.start` 16 KB+TCB at call time). Sim boot now reads 83.2 KB at
  post-fs-init vs the device's 89.5 KB steady state — within ~5 %, calibrated by
  V4; TCB/queue estimates documented for the HIL assertion to tighten.
- **M5** — `live_bytes()` reports device sizes everywhere (PER_OBJECT = 12 pinned
  by a `target_pointer_width="32"` compile assert; PER_SLOT = 40 asserted on all
  targets).
- **THR interim** — `PICODROID_PARITY_STRICT=1` (default in sim-run.sh) makes
  `Thread.start` a hard failure; threaddemo is skipped with an honest reason
  instead of false-PASSing. Non-strict runs still charge the device-side 16 KB.
- **P1** — `parity-metrics` feature: per-dispatch and per-allocation counters
  (`jvm/src/parity.rs`, AtomicUsize for thumbv6m), flushed-band counters at the
  shared seam, one identical `parity: insns=… allocs=… gcs=… bands=… fbytes=…`
  line on both sinks. Verified deterministic across runs AND build profiles
  (dev == release: 65,965,594 insns / 80,017 allocs / 321 gcs for benchmark).
- **P2** — `scripts/parity-bench.sh` + `bench/parity/history.csv`
  (utc,commit,env,app,metric,value); `--hil` lane via
  `PICODROID_EXTRA_FEATURES` pass-through in `lib.sh::build_firmware`;
  `--check` alarms on >30 % hil/sim wall-clock ratio drift vs trailing median.
- **G1** — `parity-fbhash` feature: CRC32 of each flushed band at the shared
  seam (before the sim's ARGB conversion), identical line format both sinks.
  Sim: 1000-band graphicsbench sequence bit-identical across runs.
- **Cross-environment proof (2026-07-19, RP2350 testbench)**: graphicsbench
  flashed with both parity features vs the same build simulated —
  **all 1000 framebuffer band CRCs identical** (every rendered pixel
  byte-equal on device and host), and **every deterministic counter exactly
  equal**: `insns=147899 allocs=4831 gcs=18 bands=1000 fbytes=9648348` on
  both. The P1 equality assertion and the G1 hash comparison are validated
  against real hardware, not just sim-vs-sim.
- **G2** — resolved by existing sim logs + the fbhash rect stream (set_window
  rects ride every fbhash line; backlight/sleep already logged). Full ST7789
  command modeling remains rejected; DSP-02 stays register-only.
- **G3** — sim-run.sh gained the enviro-board smoke lane (runs in full matrices
  and via `--app picoenvmon`, which CI now includes); **X3** sanitizer default-ON
  in sim.sh (`--no-sanitize-handles` to opt out) and sim-run.sh (CI already set
  it); **X2** pre-commit tripwire pins the four deliberate
  `not(feature = "family-rp")` gates.

- **M2-final** (approved 2026-07-19) — `heap_kb` in `mcus/rp/<mcu>.toml` is now
  the single source: build_support/freertos.rs injects `configTOTAL_HEAP_SIZE`
  into the FreeRTOS (and cyw43/FreeRTOS-TCP) C builds, FreeRTOSConfig.h
  `#error`s if it's absent, and `build.rs::emit_heap_config` generates the
  sim's `DEVICE_HEAP_BYTES` from the same key. The pre-commit constant
  cross-check is retired — there is no second copy left to drift.
- **M6** (approved 2026-07-19) — see its entry below.

Deferred, ask-first (recorded, not scheduled): M7 (real sim threads + GIL),
X1 (SMP heap-safety soundness trace).

Additional honest limits discovered while implementing: parity counters use
`AtomicUsize` (wrap at ~4.3e9 on device — keep scenes shorter); `parity-fbhash`
emits one line per band (~12/frame), so device capture must fit the RTT window —
keep scenes to a few seconds or extend the hil-run capture window; the CRC adds
~1-2 ms/band on-device, so `parity-fbhash` builds are for sequence comparison,
never wall-clock measurement.
