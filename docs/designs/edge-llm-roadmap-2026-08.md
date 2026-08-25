# Roadmap: Edge-LLM showcase — Pico Plus 2 W and STM32H7

**Goal:** a picodroid app that runs a small language model *on the MCU*, written
against an Android-shaped Java API, streaming tokens into a `TextView` — plus a
second board family whose memory lets the same app run a real (100M-class) model.

**Hardware:**

- **Track A — Pimoroni Pico Plus 2 W** (RP2350B, 16 MB QSPI flash, 8 MB PSRAM on
  GP47, Raspberry Pi RM2 = CYW43439 on the *same* GPIO 23/24/25/29 as the Pico
  2 W, BOOT button on GP45) + the 320x240 ST7789/XPT2046 touchscreen already used
  by the testbench boards. Stays inside `platforms/rp`.
- **Track B — STM32H747I-DISCO** (STM32H747XI: Cortex-M7 @ 480 MHz + M4, 2 MB
  internal flash, ~1 MB SRAM, 32 MB SDRAM, 2x64 MB QSPI NOR, 4" 800x480 MIPI-DSI
  capacitive-touch LCD, Ethernet, on-board STLINK-V3E with VCP). New family
  `platforms/stm32`. Stretch target once the family exists: **STM32H7S78-DK**
  (600 MHz M7, 32 MB hexa-SPI PSRAM, 128 MB OSPI flash, 5" RGB LCD — but only
  64 KB internal flash, so it needs an external-flash XiP boot stage; see B1).
- **Track S — shared framework work** (inference crate, `picodroid.ai` SDK,
  PAPK/weights plumbing, remote backend). Hardware-independent, sim-first, and a
  prerequisite for both A and B.

**Process:** each numbered session is planned and implemented in its own agentic
session, sized like one substantial PR, ending in a verifiable state with
`./scripts/pre-commit` green. Update the Status table and the session's
subsection as work lands; divergences get recorded in AMENDMENTS at the bottom.
Deep decisions (checkpoint format, Java API, STM32 memory map) belong in the
design docs Sessions S1 and B1 produce, not here.

## Status (2026-08-24)

| # | Session | Status |
|---|---------|--------|
| S1 | `llm-infer` crate: llama-family inference, Q8/Q4 checkpoints, host bench | NOT STARTED |
| S2 | Java SDK `picodroid.ai` + natives + inference task + `examples/aidemo` (sim, 260K model in the PAPK) | NOT STARTED |
| S3 | Remote backend (native streaming HTTP client) + showcase `examples/aichat` | NOT STARTED |
| A1 | Board `pico_plus2w` + PSRAM bring-up + 16 MB flash layout | NOT STARTED |
| A2 | `MODEL` flash region + weight loader + tier-2 model on device, measured | NOT STARTED |
| A3 | Dual-core inference + on-device tuning (optional) | NOT STARTED |
| B1 | Family design doc `docs/designs/family-stm32.md` + toolchain spike | NOT STARTED |
| B2 | `platforms/stm32` skeleton: boot, FreeRTOS, defmt, `helloworld` in the JVM | NOT STARTED |
| B3 | SDRAM as the FreeRTOS heap + cache/MPU policy | NOT STARTED |
| B4 | QSPI storage: LittleFS + PAPK slot + `pdb install` over the STLINK VCP | NOT STARTED |
| B5 | LTDC/DSI display + capacitive touch → `displaydemo` | NOT STARTED |
| B6 | GPIO/I2C/SPI/UART/ADC/PWM HAL + buttons | NOT STARTED |
| B7 | Ethernet networking via FreeRTOS+TCP → `http_get` | NOT STARTED |
| B8 | Tier-3 model on STM32 (`MODEL` region in QSPI, SmolLM2-class attempt), measured | NOT STARTED |
| B9 | CI lanes, HIL, website, nightly closure | NOT STARTED |

Dependency order: **S1 → S2 → {S3, A1} → A2 → A3**. Track B is independent up
to B7; **B8 needs S1/S2 + A2's weight-region design**. Recommended sequencing
for a demo-first path: S1, S2, A1, A2, S3, then B.

## Scope decisions (proposed 2026-08-24 — ratify at the start of S1)

- **The model runs in native Rust; Java owns prompt, UI and orchestration.** The
  interpreter is ~50–100x slower than native and the 416 KB heap is shared with
  LVGL/TCP/JVM; a matmul in Java is not a demo. This mirrors Android's split
  (AICore runs Gemini Nano out-of-process; apps call `GenerativeModel`).
- **One Java API, two backends.** `picodroid.ai.GenerativeModel` mirrors the
  Google AI Edge AICore SDK shape (`GenerativeModel(GenerationConfig)`,
  `generateContent(prompt)`, streaming variant). Backend `ON_DEVICE` runs
  `llm-infer`; backend `REMOTE` streams from a LAN llama.cpp / Ollama server over
  the existing `picodroid.net` stack — implemented **natively** in
  `picodroid-core`, so the app never parses JSON/SSE and no `picodroid.json`
  class is needed (SDK classes cost flash on every board).
- **Model tiers**, all llama-architecture so one runtime serves every tier:
  - Tier 1: `stories260K` (Karpathy llama2.c, 260K params, vocab 512). Q8 is
    ~260 KB → ships **inside the PAPK** as a raw-blob asset, hot-swappable via
    `pdb install`, runs in the sim and on every RP2350 board with no new
    hardware. Reference: ESP32-S3 @240 MHz dual-core with SIMD reaches ~19 tok/s;
    a single M33 @150 MHz without SIMD is estimated at 3–8 tok/s (**measure in
    S1/A2**).
  - Tier 2: `stories15M` (15.2M params; dim 288, 6 layers, vocab 32000, seq 256)
    or a custom-trained 2–6M model with the llama2.c `tok4096` tokenizer. Needs
    the Pico Plus 2 W: weights in the `MODEL` flash region, KV cache + logits in
    PSRAM (fp32 KV at seq 256 is ~3.5 MB; logits 128 KB). Reference: ESP32-S3
    ~3 tok/s dense fp32; RP2350 estimate 0.3–1 tok/s single core (**measure**).
    Note 9.2M of the 15.2M params are the (shared) embedding/classifier matrix —
    the output matmul dominates per-token cost, which is why a smaller-vocab
    custom model may beat stories15M on both speed and flash.
  - Tier 3 (Track B only): SmolLM2-135M Q4 (~70–80 MB, 49k-vocab BPE) or
    stories42M/110M. Estimated 0.3–1 tok/s on a 480 MHz M7 from QSPI — the least
    certain number in this document.
- **Region / model ecosystem stays llama2.c-compatible**: the checkpoint format
  is Karpathy's `export.py` v1 (fp32) / v2 (Q8_0 groups of 64) plus one
  picodroid-specific Q4 variant, so any llama2.c-exportable HF model (TinyLlama,
  SmolLM2, custom TinyStories runs) is a host-side conversion away.

## Cross-cutting decisions

- **Inference lives in a new workspace crate `llm-infer/`** (no_std + alloc-free
  core, fixed-capacity workspaces, zero platform deps — the `papk-format/` and
  `pdb-protocol/` precedent). Host-testable with plain `cargo test`; a host
  `bench` binary reports tok/s and workspace bytes for a given checkpoint. No
  shadow-twin obligations. `picodroid-core` gets only glue: the FreeRTOS
  inference task, the token mailbox, the JVM natives, the weight/workspace
  sources.
- **Two weight sources behind one trait** (`WeightSource -> &'static [u8]`):
  (1) a PAPK raw-blob asset (tier 1; mapped from XIP flash exactly like image
  assets in `picodroid-core/src/graphics/assets.rs`), (2) a dedicated `MODEL`
  linker region flashed separately (tiers 2–3). The sim maps (2) to a host file
  (`--model <path>`).
- **PAPK change is minimal**: the `ASST` record already carries
  `[u16 width][u16 height][u8 cf][u8 reserved0][u16 stride]` — add a
  `cf = RAW_BLOB` code with width/height/stride 0. The image lookup ignores
  blobs; `papk-pack` gains `assets/*.bin` (it is PNG-only today, per
  `website/src/content/docs/guides/assets.md`); `papk-info` prints blob sizes.
  No file-header change, so no PAPK version bump and no
  `FrameworkVersionMismatch` risk.
- **Workspace memory never comes from the JVM heap.** `llm-infer` takes a
  caller-provided `Workspace` (KV cache, activations, logits). On plain RP2350
  boards it is a static/board-tunable SRAM slab (`llm_workspace_kb`) and
  `max_seq_len` is clamped to fit; on the Pico Plus 2 W it is a bump allocator
  over the memory-mapped PSRAM; on STM32 it is SDRAM. This keeps the interpreter's
  heap and GC pacing untouched and keeps FreeRTOS `heap_4` single-arena.
- **Token delivery clones the sensor pipeline**
  (`picodroid-core/src/hardware/sensors/{mailbox,sampler,mod}.rs`): the
  inference task publishes `(token_id, piece_len, piece[16])` records into an
  atomics-only seqlock ring; `lifecycle.rs` drains it once per ~16 ms UI tick and
  fires at most one interpreted callback per tick. The only per-token JVM
  allocation is the piece `String` (counts toward GC pacing — the native-alloc
  rule from `project_native_alloc_gc_gap`). No JVM heap reference ever crosses a
  task boundary; the inference task must **not** touch the JVM heap at all (the
  SMP equal-priority wake-yield hazard in
  `project_picoenvmon_soak_child_thread_gc_death` applies to anything that
  does).
- **Inference task placement**: RP2350 core 1 (shared with the flash parker
  (priority 30) and cyw43 (22) — inference runs *below* both, priority
  `PRIORITY_JVM_NORM`, so WiFi and flash ops preempt it), core 0 fallback via
  a board tunable. It runs at JVM priority so it never starves the UI tick; the
  RP2350 XIP cache is shared by flash and PSRAM — measure cache thrash in A2
  before committing to dual-core (A3).
- **Frozen Java surface** (S2 pins the exact signatures):
  `picodroid.ai.GenerativeModel` (`isAvailable()`, `generateContent(String)`,
  `generateContent(String, StreamingCallback)`, `cancel()`, `close()`),
  `picodroid.ai.GenerationConfig` (+ `Builder`: `maxOutputTokens`,
  `temperature`, `topK`, `seed`, `backend`, `remoteHost`, `remotePort`),
  `picodroid.ai.StreamingCallback` (`onToken(String)`, `onComplete(String)`,
  `onError(String)`). Callbacks arrive on the main (UI) thread, as Android's do.
- **Remote protocol**: llama.cpp `POST /completion` with `"stream": true` (SSE
  `data:` lines) as primary, Ollama `/api/generate` NDJSON as secondary. Both
  require **chunked transfer-encoding** in the HTTP client — not found in
  `picodroid-core/src/net/http_*.rs` as of 2026-08-24; S3 adds it. Plain HTTP
  only (no TLS in the stack): LAN or a local proxy, never a cloud endpoint
  directly. Respect the `Socket.send <= 256 B/call` gotcha and the 2 KB TCP
  buffers on `pico_enviro_mon_w`.
- **Budget**: the `picodroid/ai/*` SDK classes and `llm-infer` cost flash on
  every board. RP2040 excludes the classes via `framework_class_excludes`
  (precedent: the nine `picodroid/net/*` exclusions in
  `platforms/rp/boards/testbench_rp2040/board.toml`) and the RP2040 release gate
  is re-checked by hand. `llm-infer` sits behind a `picodroid-core` feature
  `llm` forwarded by board features (`assert_forwarded_features_match` requires
  it); non-`llm` boards get a native stub that makes `isAvailable()` false, so a
  portable app degrades gracefully (the `net_stub.rs` pattern).
- **Naming**: crate `llm-infer`; feature `llm`; Java package `picodroid.ai`;
  board `pico_plus2w` (Track A); family `stm32`, MCU `stm32h747xi`, board
  `stm32h747i_disco` (Track B); apps `examples/aidemo` (thin API exerciser,
  tier 1, runs everywhere including CI sim) and `examples/aichat` (showcase).

## Out of scope (deferred, by design)

Training pipelines (host-side scripts may live in `tools/` but are not part of
the firmware deliverable); speech/keyword spotting (no microphone on the RP
boards); TLS / cloud LLM APIs; Bluetooth on the RM2; the STM32H747's Cortex-M4
(held in reset or parked — never a second JVM core); DMA2D acceleration for
LVGL; NPU-class parts (STM32N6, Alif, Renesas RA8 — no Rust ecosystem yet);
ESP32 (removed 2026-07-25 in `8300bf8` as too complicated; not revisited here);
a generic `org.json`-style SDK class (flash cost on every board; the remote
backend parses natively).

## Track S — shared framework

### Session S1 — `llm-infer` crate + design doc

Deliverables: `llm-infer/` workspace crate and `docs/designs/llm-infer.md`
(house style per `docs/designs/net-typed-exceptions.md`: `## 0. Why this
exists` / numbered Decisions / Seams / Stages / Status / Amendments).

- `checkpoint.rs` — parse llama2.c v1 (fp32) and v2 (Q8_0, group 64) headers
  from a `&[u8]`; add a Q4 variant (group 32, fp16 scale) with an `export`
  subcommand in the host bench tool. Header validation must never panic (it
  reads flash the user flashed).
- `model.rs` — llama forward pass: RMSNorm, RoPE, GQA-aware attention (needed
  for SmolLM2 in B8), SwiGLU FFN, tied/untied classifier. Integer/fixed-point
  paths for Q8 dot products (`SMLAD`-friendly loops on `thumbv8m`; plain scalar
  on host). All buffers from a `Workspace` trait; `max_seq_len` is a runtime
  parameter clamped by workspace size.
- `tokenizer.rs` — llama2.c `tokenizer.bin` (BPE merge by score) for vocab 512 /
  4096 / 32000 / 49152. Decode pieces to UTF-8 incrementally (multi-byte tokens).
- `sampler.rs` — argmax / temperature / top-k with a caller-supplied RNG seed.
- `bench` host binary: `llm-bench <ckpt> <tokenizer> --prompt ... --steps N`
  prints tok/s, workspace bytes, peak KV bytes.

Verify: golden vectors — the first 32 tokens of `stories260K` for a fixed
prompt/seed match Karpathy's `run.c` byte-for-byte; Q8 output matches within a
documented tolerance; `cargo test -p llm-infer` under `./scripts/test.sh`;
pre-commit.

### Session S2 — Java SDK `picodroid.ai` + natives + inference task + `examples/aidemo`

- SDK classes (`sdk/java/picodroid/ai/`), added to `PICODROID_NATIVE_CLASSES`
  (`picodroid-core/src/native_handler/class_registry.rs`) and
  `method_tables.rs` (paste rows from the test failure, never transcribe —
  `project_native_method_crosscheck`). Native handler module
  `picodroid-core/src/native_handler/ai.rs`; dispatch arm names must match the
  Java method names (`project_android_fidelity_roadmap`).
- `picodroid-core/src/ai/`: `task.rs` (FreeRTOS inference task, spawned lazily
  on first `generateContent`, request queue depth 1, cancel flag), `mailbox.rs`
  (token seqlock ring), `weights.rs` (`WeightSource` impls: PAPK blob, `MODEL`
  region, sim file), `workspace.rs` (SRAM slab / PSRAM / SDRAM / host `Vec`
  — the sim allocation goes through the heap-cap `bypass()` guard, the minifb
  precedent in `project_sim_heapcap_minifb_window`).
- `lifecycle.rs` drain hook (one callback per tick); GC roots for the callback
  object and the `GenerativeModel` singleton (**root it** — the unrooted
  `Display` singleton bug in `project_switch_gc_root_gap_nosuchmethod` is the
  exact failure shape).
- PAPK: `RAW_BLOB` asset kind in `papk-format`, `papk-pack` `.bin` scan,
  `papk-info` output; `examples/aidemo/assets/stories260K.q8.bin` +
  `tokenizer.bin`.
- Board tunables (`[jvm]`-style `[llm]` table in board.toml, parsed in
  `build_support/board_cfg.rs` with a parser test — unknown sections are
  silently dropped today): `llm_workspace_kb`, `llm_max_seq_len`, `llm_core`.

Verify: `./scripts/sim.sh --app aidemo` streams a coherent TinyStories
continuation; `--mem-diag` shows steady-state heap after 20 generations (no
growth); on `testbench_rp2350` hardware via `pdb install`, RTT shows tokens at
the measured tok/s and the display keeps animating; pre-commit (langsuite,
`every_native_class_is_registered`, RP2040 flash gate with the exclusion).

### Session S3 — Remote backend + showcase `examples/aichat`

- `picodroid-core/src/net/http_*.rs`: chunked transfer-encoding decode
  (verify absence first; add with a host test against canned responses).
- `picodroid-core/src/ai/remote.rs`: native streaming client over the existing
  socket layer — request builder (llama.cpp `/completion`, Ollama
  `/api/generate`), an SSE/NDJSON line scanner, a minimal JSON string-field
  extractor (only `"content"` / `"response"`), pushes pieces into the same
  mailbox as the on-device path. Timeouts everywhere (`HttpURLConnection`
  timeouts precedent, `project_nightly_20260818_sim_failures`).
- `examples/aichat`: `EditText` + `Keyboard` prompt entry, streaming `TextView`
  in a `ScrollView`, a `Spinner` for backend/model, `SharedPreferences` for the
  remote host, `Toast` on error, `AlertDialog` on cancel. Buttons-only boards
  (`pico_enviro_mon*`) get a canned-prompt list instead of free text
  (`project_picoenvmon_4button_nav` guideline: cap focusable rows ~12).

Verify: sim against a local `llama-server` (SmolLM2-360M) streams; on
`testbench_rp2350w` hardware the same over WiFi; `netexception`-style negative
tests (server down, mid-stream disconnect) raise the typed net exceptions
(`project_net_typed_exceptions_plan`: never `InvalidReference` for I/O);
pre-commit.

## Track A — Pico Plus 2 W

### Session A1 — Board `pico_plus2w` + PSRAM bring-up + 16 MB layout

Buildable / sim-runnable / flashable board before any model code.

- `platforms/rp/boards/pico_plus2w/board.toml`: copy `testbench_rp2350w`
  verbatim (display/touch/cyw43 — **the RM2 pins are identical to the Pico 2 W**:
  `WL_REG_ON=23, DATA=24, CS=25, CLOCK=29` per the pico-sdk board header, so
  `cyw43_configport.h` is unchanged), plus `linker_script =
  "boards/pico_plus2w/memory.x"` (the `linker_script` override in
  `build_support/boards.rs::place_memory_x` already exists) and `psram_cs_pin =
  47`, `psram_kb = 8192`.
- `memory.x`: keep the first 4 MB **byte-identical** to `mcus/rp/rp2350.x`
  (FLASH 2816K, FS 256K @0x102C0000, PAPK 1M @0x10300000) so install/FS code
  needs no change, and add `MODEL : ORIGIN = 0x10400000, LENGTH = 12M` and
  `PSRAM : ORIGIN = 0x11000000, LENGTH = 8M`.
- PSRAM init in `platforms/rp/src/hal/rp/psram.rs`: GP47 → `XIP_CS1n`
  function, QMI M1 timing/format registers, APS6404L reset-enable/reset/QPI
  sequence — port MicroPython's `ports/rp2/rp2_psram.c` (the canonical ~100
  lines). Keep it separate from `flash.rs`'s QMI **M0** save/restore in
  `with_xip_disabled!` (`project_xip_restore_after_flash_ops`): M1 must survive
  a flash write, so add an M1 restore to that macro and a test-by-inspection
  note. Verify a write/read-back pattern over all 8 MB at boot in debug builds.
- RP2350B: `rp235x-hal 0.4` has no A/B package feature; confirm GP47's pin type
  exists, else set `IO_BANK0.GPIO47_CTRL` + pads via the PAC. Nothing here uses
  PIO on GPIO >= 32, so the RP2350 PIO `GPIOBASE` constraint does not bite —
  note it in the board file for future users.
- Registration checklist (do first — pre-commit catches misses 15 minutes
  late): `platforms/rp/Cargo.toml` board feature (`chip-rp2350`,
  `picodroid-core/board-pico-plus2w`, `network-cyw43`, `llm`),
  `picodroid-core/Cargo.toml` features, `.cargo/config.toml` alias,
  `scripts/pre-commit` clippy matrix (line ~135), `.github/workflows/ci_checks.yml`,
  `scripts/lib.sh` board→target, website `build.md` board table / `limits.md`
  row (SRAM 520 KB, flash 16 MB, PSRAM 8 MB) / `cargo-aliases.md`, README table.

Verify: `cargo b-pico-plus2w`; `./scripts/sim.sh --app displaydemo --board
pico_plus2w`; flash `displaydemo` + WiFi smoke (`http_get`) on the real board
with the display stacked; PSRAM pattern test passes in the RTT log; pre-commit.

### Session A2 — `MODEL` region + weight loader + tier-2 model, measured

- `WeightSource::Region` reads the `MODEL` linker symbols; a 64-byte picodroid
  header (magic, checkpoint kind, byte length, crc32, model name) precedes the
  llama2.c checkpoint so a stale/absent model is detected, not executed.
- Flashing: `./scripts/flash-model.sh <model.bin>` wrapping `probe-rs download
  --binary-format bin --base-address 0x10400000` (never overlap with
  `flash.sh` — `feedback_probe_serialize_flash`). Optional follow-up: a pdb
  `install-model` command reusing the PAPK install path's flash writer with
  the core-1 parker (`project_rp2350_core1_park_xip`).
- PSRAM `Workspace` (bump allocator; fp32 or int8 KV selectable), `llm_core = 1`.
- Model selection by measurement, in this order, all with the host bench first:
  `stories15M` Q8 (15.2 MB — **exceeds the 12 MB region**; shrink FLASH to
  1536K in `memory.x` only if the release image stays under it, else Q4),
  `stories15M` Q4 (~7.6 MB), and a custom 2–6M `tok4096` model. Record tok/s,
  first-token latency, and XIP-cache behavior with the display animating.

Verify: `aichat` on the Pico Plus 2 W generates a multi-sentence story from a
touch-typed prompt at the recorded rate with WiFi up; `pdb sysmon` heap flat
across 20 generations; `bootcount`-style power-cycle shows the model survives
(`reference_fs_persistence_test`); pre-commit + RP2040 gate.

### Session A3 — Dual-core inference + tuning (optional)

Split the classifier matmul (the dominant cost) across both cores with a
FreeRTOS task on each; measure against the A2 baseline. Also: Q8 `SMLAD`
inner loops, fp16 KV, XIP-cache-friendly weight layout (row-major groups
contiguous per head). Stop when the gain is < 1.3x — the demo does not need it.

## Track B — STM32H7 family

### Session B1 — Family design doc `docs/designs/family-stm32.md` + toolchain spike

Pin every seam before writing platform code. Must settle, with evidence:

- **Board**: `STM32H747I-DISCO` primary (internal 2 MB flash → simple boot; 32 MB
  SDRAM; 128 MB QSPI; mature `stm32h7xx-hal` + `embassy-stm32` coverage).
  `STM32H7S78-DK` documented as the stretch target with its blocker: the
  H7S7L8 has 64 KB internal flash, so firmware must run XiP from external OSPI
  behind ST's boot stage (`embassy-stm32` lists `stm32h7s7z8`; confirm the
  H7S7L8 variant and the XiP boot flow before promising it).
- **Cortex-M4 policy**: keep it in reset via option byte `BCM4 = 0` (or park it
  at boot). Never a JVM core.
- **FreeRTOS**: `freertos_port = "GCC/ARM_CM7/r0p1"` from
  `third_party/FreeRTOS-Kernel` (present in the vendored tree), single-core —
  which removes the SMP hazards Track A carries. Verify
  `build_support/freertos.rs` treats `pico_shim` / `init_array_segment` as
  optional (both are RP quirks); add an `stm32` branch only if unavoidable.
- **Memory map**: vector table + code in internal flash; `.data/.bss` + task
  stacks in AXI SRAM (512 KB); FreeRTOS `heap_4` arena in SDRAM (24 MB —
  `heap_kb = 24576`, which also becomes the sim's default cap); LVGL
  framebuffer and `MODEL` workspace in the remaining SDRAM; QSPI memory-mapped
  for XIP reads
  (weights, PAPK, image assets), indirect mode for prog/erase. MPU: SDRAM
  write-back cacheable, QSPI read-only cacheable, DMA buffers non-cacheable.
- **pdb transport = USART over the STLINK VCP** implementing
  `picodroid_core::pdb::PdbTransport` (`picodroid-core/src/pdb/mod.rs`) — no
  USB stack at all (the RP `pdb_usb` is a hand-rolled register-level driver;
  don't port it). `pdb -s /dev/ttyACM*` works unchanged.
- **Display**: the DISCO panel is MIPI-DSI (OTM8009A) — vendor ST's BSP C files
  (`third_party/stm32h747i-disco-bsp`, DSI + OTM8009A init only) behind a
  small C shim like `pico_shim_rp2350.c`; LTDC scans a 800x480 RGB565
  framebuffer in SDRAM. `HalDisplay::set_window/write_pixels` becomes a
  band → framebuffer copy (DMA2D later). New `[display] driver = "ltdc"` and
  `[touch] driver = "ft6206"` (I2C cap touch; `set_calibration` is a no-op)
  with parser/codegen in `build_support/board_cfg.rs` + tests.
- **Networking**: on-board Ethernet (LAN8742 RMII) via
  `vendor/freertos-plus-tcp/source/portable/NetworkInterface/STM32` (present)
  — no WiFi credentials, no cyw43. The `FreeRTOS_*` extern block in
  `platforms/rp/src/hal/rp/net.rs` is family-neutral; move it into
  `picodroid-core` per `docs/designs/family-neutral-residue.md` ("networking
  pending") so both families share it.
- **Workspace**: `platforms/stm32` as a root workspace member if `Cargo.lock`
  tolerates `stm32h7xx-hal`; otherwise its own `[workspace]`
  (`project_platforms_layout` rule 4).
- Spike output: a bare `cortex-m-rt` blink + defmt-over-RTT on the DISCO using
  the chosen HAL, committed as evidence, not as platform code.

### Session B2 — `platforms/stm32` skeleton → `helloworld` in the JVM

`platforms/stm32/{Cargo.toml, build.rs, src/, boards/stm32h747i_disco/,
mcus/stm32/stm32h747xi.{toml,x}}`. `build.rs` imports `build_support/*` via
`#[path]`. Boot: clock tree to 480 MHz, caches on, `hal::HalClock`, defmt-RTT,
FreeRTOS started with the JVM task + pdb task (no flash parker, no sensor
sampler). `glue.rs` implements `PlatformHooks` (`picodroid-core/src/host.rs`)
and the `__pd_rtos_*` externs (`picodroid-core/src/rtos.rs`) — copy
`platforms/rp/src/glue.rs` structurally, delete the RP-only parts. Unimplemented
HAL traits are explicit `unimplemented!()` stubs listed in the design doc.

Verify: `helloworld` and `benchmark` PAPKs embedded at build time print over
RTT; `benchmark` TOTAL recorded as the family baseline; shadow-twin and
cfg-hygiene guards pass; pre-commit (new clippy lane).

### Session B3 — SDRAM heap + cache/MPU policy

FMC init for the 32 MB SDRAM (ST BSP timings), `heap_4` arena relocated to
SDRAM via the linker script + `configAPPLICATION_ALLOCATED_HEAP`, MPU regions
per B1. Run `heapstress`, `gcstress`, `threaddemo`, `perfbench`; compare
`perfbench` composite with the arena in AXI SRAM vs SDRAM (D-cache makes the
difference; record both). Confirm `mem-diag` offensive checks run on-device
(`docs/memory-diagnostics.md`).

### Session B4 — QSPI storage: LittleFS + PAPK + `pdb install`

QSPI driver (dual-flash memory-mapped reads; indirect-mode sector erase/page
program) → `picodroid_core::fs::FsBackingStore` impl (the 85-line
`platforms/rp/src/fs/storage.rs` is the template), `FS_FLASH` and `PAPK_FLASH`
regions in QSPI, `read_flash_papk` from the mapped address, the install path's
flash writer (no core parking needed: code executes from internal flash).
`PdbTransport` over USART.

Verify: `pdb install build/apks/blinky.papk` hot-swaps; `bootcount` persists
across power cycles; `prefs_demo`; `test-future-version-rejection.sh` against
the board; pre-commit.

### Session B5 — Display + touch → `displaydemo`

DSI/OTM8009A bring-up through the vendored BSP shim, LTDC framebuffer in SDRAM,
`HalDisplay` (band copy), FT6206 → `HalTouch` (raw = calibrated; the PDB
`inject_override` path must still work for `pdb input tap`), `lv_dpi`,
`lv_mem_kb = 512`. Idle sleep off by default (touch-only board — see
`limits.md` "Display idle sleep").

Verify: `displaydemo`, `animdemo`, `keyboarddemo`, `dragdemo`, `gesturedemo` on
hardware; `pdb input tap/swipe` drives them; sim window is 800x480 for
`--board stm32h747i_disco`; pre-commit.

### Session B6 — GPIO/I2C/SPI/UART/ADC/PWM + buttons

Arduino-header pins → `HalGpio` (incl. the edge-IRQ queue and
`wait_for_button_event`), `HalI2c`, `HalSpi`, `HalUart` (a second USART, not the
pdb one), `HalAdc`, `HalPwm`; user button as a `[[button]]`. Verify with
`blinky` (LED), `i2cdemo`/`spidemo`/`uart`/`adcdemo`/`pwmdemo`, `keydemo`;
`every_native_class_is_registered`; pre-commit.

### Session B7 — Ethernet networking → `http_get`

FreeRTOS+TCP with the STM32 `NetworkInterface` (C, built by an `stm32` arm in
`build_support/network.rs`), PHY init, DHCP, `HalNet` impl on the shared
`FreeRTOS_*` bindings moved in B1, `has_network = true`, `network_type =
"eth"`. Verify `netdemo`, `http_get` (`PICODROID_NET_TEST_HOST`), `netexception`,
`picoenvmon`'s dashboard serving pattern (100-curl gate, judge by `http_code`),
and the S3 remote backend end-to-end; pre-commit.

### Session B8 — Tier-3 model on STM32, measured

`MODEL` region in QSPI (e.g. 64 MB), `WeightSource::Region` unchanged from A2,
SDRAM `Workspace`. Run, in order, with the host bench first: `stories15M` Q8
(baseline vs. A2), `stories42M`/`110M` Q4, then **SmolLM2-135M Q4** (GQA + 49k
BPE — the S1 tokenizer/attention generality is exercised here). Record tok/s,
first-token latency, QSPI bandwidth utilisation. Accept whatever rate the
hardware gives; the deliverable is the measurement and a working `aichat`.

### Session B9 — CI lanes, HIL, website, nightly closure

Clippy/build lanes in `scripts/pre-commit` and `ci_checks.yml`, `hil-run.sh`
support for a second probe/board (`scripts/hil-tests.conf`), the nightly cron
(`reference_nightly_cron`), website pages (`build.md`, `limits.md` board rows,
a `platforms/stm32` note in `ARCHITECTURE.md`, `release-notes.md`), README
table. Soak: 1 h `aichat` + nav under `--mem-diag` on both boards.

## Codebase anchor points (for the implementing sessions)

- HAL seam: `picodroid-core/src/hal/traits.rs` (11 traits: Display, Gpio,
  Clock, Touch, I2c, Adc, Pwm, Spi, Uart, Net, Fs); RP impls in
  `platforms/rp/src/glue.rs`; sim impls in `picodroid-core/src/hal/sim/`.
- RTOS seam: `picodroid-core/src/rtos.rs` (`__pd_rtos_*`); platform hooks:
  `picodroid-core/src/host.rs::PlatformHooks`.
- FreeRTOS C build: `build_support/freertos.rs`, keyed by the MCU toml
  (`freertos_port`, `freertos_port_extra_includes`, `freertos_c_defines`,
  `freertos_vector_aliases`, `init_array_segment`, `pico_shim`, `heap_kb`).
  Kernel at `third_party/FreeRTOS-Kernel` (`portable/GCC/ARM_CM7` present).
- Linker script placement: `build_support/boards.rs::place_memory_x` (board
  `linker_script` key). Current RP2350 map: `platforms/rp/mcus/rp/rp2350.x`.
- Board config parsing/codegen: `build_support/config.rs`,
  `build_support/board_cfg.rs` (unknown sections silently dropped — add tests).
- Networking build: `build_support/network.rs`; TCP stack
  `vendor/freertos-plus-tcp` (shivrajora fork — re-carry the RST patch on any
  rebase, `project_freertos_tcp_fork_rst_fix`); RP socket bindings
  `platforms/rp/src/hal/rp/net.rs`; Java-facing layer `picodroid-core/src/net/`.
- pdb: `picodroid-core/src/pdb/mod.rs::PdbTransport`; RP impl
  `platforms/rp/src/pdb/platform.rs` + `platforms/rp/src/hal/rp/pdb_usb/`.
- Storage: `picodroid-core/src/fs` (`FsBackingStore`); RP impl
  `platforms/rp/src/fs/storage.rs`; RP flash ops `platforms/rp/src/hal/rp/flash.rs`.
- PAPK: `papk-format/src/lib.rs` (ASST record layout), `tools/papk-pack`,
  `tools/papk-info`; XIP asset mapping `picodroid-core/src/graphics/assets.rs`.
- Token-delivery template: `picodroid-core/src/hardware/sensors/{mailbox,sampler,mod}.rs`;
  drain site `picodroid-core/src/lifecycle.rs`.
- Natives: `picodroid-core/src/native_handler/{class_registry,method_tables}.rs`,
  `picodroid-core/src/dispatch_sites.rs`; GC roots
  `picodroid-core/src/gc_roots.rs` / `gc_root_registration.rs`; GC entry
  `jvm/src/lib.rs::collect_now`.
- Task priorities: `picodroid-core/src/task_priority.rs`; RP task spawn
  `platforms/rp/src/boot_tasks.rs`.
- Board budgets today (RP2350): release image ~903 KB text of the 2816 KB
  `FLASH` region; heap 416 KB; PAPK cap 1020 KB; LittleFS 256 KB.
- External references: Pico Plus 2 W board header
  (`pico-sdk/src/boards/include/boards/pimoroni_pico_plus2_w_rp2350.h`),
  MicroPython `ports/rp2/rp2_psram.c`, Karpathy `llama2.c` (`run.c`, `runq.c`,
  `export.py`), ST `stm32h747i-disco-bsp`, `embassy-stm32` / `stm32h7xx-hal`.

## End-to-end acceptance

1. `./scripts/sim.sh --app aidemo` streams a TinyStories continuation from the
   PAPK-embedded 260K model; `aichat` in the sim streams from a local
   `llama-server`.
2. Pico Plus 2 W: `aichat` generates from a touch-typed prompt with the tier-2
   model at the A2-recorded tok/s while WiFi is associated; heap flat over 20
   generations under `pdb sysmon`; model survives power-cycle.
3. STM32H747I-DISCO: the same `aichat` PAPK, installed over the STLINK VCP,
   runs the tier-3 model at the B8-recorded rate, with the remote backend
   working over Ethernet.
4. Every existing board still passes `./scripts/pre-commit`, the RP2040 flash
   gate, and the nightly sim/HIL runs; `testbench_rp2040` reports
   `GenerativeModel.isAvailable() == false` without a native miss.

## AMENDMENTS

Append here as execution diverges; amendments override the body above.
