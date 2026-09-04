# Roadmap: Meshtastic board + example app

**Goal:** a new picodroid board and example app that together form a **real Meshtastic node** — on-air compatible with stock Meshtastic devices on the default US915 LongFast channel.

**Hardware:** Raspberry Pi Pico 2 W (RP2350) + Waveshare Pico-LoRa-SX1262 module (915 MHz variant) + the 320x240 ST7789/XPT2046 touchscreen already used on the testbench boards, all stacked.

**Process:** each numbered session below is planned and implemented in its own agentic session, sized like one substantial PR, ending in a verifiable state with `./scripts/pre-commit` green. Update the Status table and the session's subsection as work lands; divergences from this doc get recorded in AMENDMENTS at the bottom, and deep protocol decisions belong in the Session 1 design doc (`docs/designs/meshtastic-node.md`), not here.

## Status (2026-08-19)

| # | Session | Status |
|---|---------|--------|
| 1 | Design doc `docs/designs/meshtastic-node.md` | NOT STARTED |
| 2 | Board `meshnode_rp2350w` + `[lora]` board.toml plumbing | NOT STARTED |
| 3 | `mesh-protocol` crate: wire format + crypto (host-only) | NOT STARTED |
| 4 | SX1262 driver + device wiring + real-air RX proof | NOT STARTED |
| 5 | Mesh engine: router, node DB, radio task | NOT STARTED |
| 6 | Sim mesh backend: UDP ether + synthetic peer | NOT STARTED |
| 7 | Java SDK `picodroid.mesh` + natives + `examples/meshdemo` | NOT STARTED |
| 8 | Showcase app `examples/meshchat` | NOT STARTED |
| 9 | Interop hardening, soak, HIL, docs closure | NOT STARTED |

## Scope decisions (settled with the project owner, 2026-08-19)

- **Full Meshtastic interop**: real protobufs, AES-CTR channel encryption with the default LongFast PSK, node DB, flood-routing subset, want_ack handling. Verified against stock Meshtastic node(s) with the phone app as observer/traffic source.
- **Native Rust stack**: SX1262 driver + protocol in Rust; Java gets a high-level `picodroid.mesh` API the example app codes against.
- **Region**: US915 (915 MHz module). Region stays build-time configurable via board.toml.

## Cross-cutting decisions

- **Protocol code lives in a new workspace crate `mesh-protocol/`** (no_std, fixed-capacity buffers, zero platform deps — the `pdb-protocol/` and `papk-format/` precedent). Host-testable with plain `cargo test`, no shadow-twin obligations. `picodroid-core` gets only glue: HAL radio seam, FreeRTOS task, mailbox, JVM natives.
- **Protobuf: hand-rolled** codec for the needed subset (Data, User, Routing encode+decode; Position, Telemetry decode-only) — house style (`papk-format/`, `pdb-protocol/`), pinned by golden vectors generated with the official `meshtastic` Python lib. Fallback if hand-rolling proves error-prone: `femtopb`. **Crypto: RustCrypto `aes` + `ctr`** as normal Cargo deps (pure-Rust no_std; `third_party/` ceremony is only for C submodules).
- **Radio task**: dedicated FreeRTOS task, priority 21 (RT band), **pinned core 1** (cyw43=22 also core 1; pdb=21 is core-0-pinned so no contention). Owns SX1262 + router + protocol timing. DIO1 via GPIO IRQ -> `vTaskNotifyGiveFromISR` (cyw43 hostwake pattern; NVIC is core-banked — register on core 1). Never the shared button GPIO event queue.
- **JVM boundary clones the sensor pipeline** (`picodroid-core/src/hardware/sensors/{sampler,mailbox,mod}.rs`): atomics-only mailbox (seqlock), drained once per ~16 ms UI tick from `lifecycle.rs`, 1 interpreted callback/tick pacing, **recycled MeshMessage event object** (steady-state RX delivery allocates only the on-demand text String). No JVM heap ref crosses a task boundary. TX = Java enqueues into the mailbox, radio task drains.
- **SPI1 shared three ways** (display 62.5 MHz / touch 2 MHz / SX1262 <= 16 MHz) via the existing `SpiFreqSwitch` per-transaction switching + `SPI1_LOCK`. Blocking SPI for the radio (frames <= 256 B at 16 MHz is ~130 us; DMA ch0-3 belong to the display — don't touch). Rule: **wait for BUSY-low before acquiring the bus lock**, never hold the lock while waiting on BUSY.
- **GP15 reset conflict** (display RST = LoRa HAT RESET when stacked): shared boot-time reset — display init already pulses GP15 (resets both chips); SX1262 init runs strictly after and never touches reset (recovers via SPI SetStandby + status verification). `[lora] pin_rst` optional in the schema. Document the hardware fallback (bend the HAT's RESET pin / cut the trace) for users needing independent resets.
- **WiFi coexists** (no resource overlap: cyw43 = PIO0 SM0 + DMA ch4/5 + GP23/24/25/29). The showcase app uses it for NTP so chat rows get real timestamps.
- **Budget**: RP2350 flash is loose (2816K). Estimated adds: driver ~10-15K flash, mesh-protocol ~35-50K flash; ~10-14K static RAM (node DB 64 entries, dedup ring 64, TX queue 4x256 B, RX ring 4x256 B, 4K task stack). **RP2040 is the constraint**: all `picodroid/mesh/*` SDK classes go into `framework_class_excludes` in `platforms/rp/boards/testbench_rp2040/board.toml` (precedent: the 9 excluded net classes) and the rp2040 release gate gets re-checked by hand.
- **Naming**: board `meshnode_rp2350w`; capability feature `lora-sx1262` in `picodroid-core/Cargo.toml` (forwarded by the board feature — `assert_forwarded_features_match` requires it); cfg gates like sensors (`#[cfg(any(lora_sx1262, feature = "sim", test))]`); Java package `picodroid.mesh`; apps `examples/meshdemo` (thin API exerciser) and `examples/meshchat` (showcase).

## Out of scope (deferred, by design)

BLE/serial/TCP phone-client API (the phone app is only an observer via the stock node); MQTT gateway; remote admin; store-and-forward; traceroute/range-test; **PKI / DM-encryption-v2** (PSK channel crypto only — stock nodes accept channel-PSK DMs); channel management beyond LongFast slot 0; GPS/position transmit (decode-only); telemetry transmit (decode-only); SNR-weighted rebroadcast delays (plain randomized contention window is a spec-permitted simplification).

## Sessions

### Session 1 — Design doc `docs/designs/meshtastic-node.md`

Pin every protocol constant and seam so later sessions never re-litigate. House style per `docs/designs/net-typed-exceptions.md` (`## 0. Why this exists` / numbered Decisions / Seams / Stages / Status / Amendments, file:line cites). Must pin, with worked byte-level examples generated host-side with the official `meshtastic` Python lib:

- Exact US915 LongFast RF params (BW/SF/CR, sync word 0x2B, preamble, CRC) + the computed LongFast frequency slot in MHz.
- The 16-byte header bit layout, incl. hop_start / next-hop / relay-node policy (transmit 0, ignore on RX — verify stock firmware tolerates it).
- AES-CTR nonce construction + expansion of the `AQ==` default PSK.
- Protobuf field tables for the message subset.
- **At least 3 golden vectors in hex** (encrypted text packet, NodeInfo, ROUTING ack): plaintext, key, nonce, ciphertext, header.
- Flood-routing subset: dedup ring 64 keyed (sender, packet_id), hop_limit 3, contention window formula, rebroadcast conditions, implicit-ack + ROUTING-ack semantics.
- Node ID derivation (RP2350 unique flash ID -> low 32 bits, `!hexid` display name).
- Seams: `hal::lora` trait signature, mailbox record layouts, frozen Java API surface, `[lora]` board.toml schema, GP15 policy.
- The out-of-scope list above, verbatim.

**Pin the doc to the firmware version running on the stock test nodes** (Meshtastic 2.x drifted: hop_start, next-hop fields). Verify: vectors cross-checked two ways (Python lib encrypt vs. manual header+nonce construction); pre-commit.

### Session 2 — Board `meshnode_rp2350w` + `[lora]` board.toml plumbing

Buildable / sim-runnable / flashable board before any radio code. Copy `platforms/rp/boards/testbench_rp2350w/board.toml` verbatim (display/touch/cyw43) + a new section:

```toml
[lora]
driver = "sx1262"
spi_id = 1
spi_freq = 16000000
pin_cs = 3
pin_busy = 2
pin_dio1 = 20
# pin_rst intentionally omitted - shared with display RST (GP15), see GP15 policy
region = "US915"
```

**Mandatory parser work**: `build_support/config.rs::parse_board_toml` silently drops unknown sections — add `[lora]` parsing + codegen in `build_support/board_cfg.rs` (one generator, called from both build.rs, unconditional `cargo:rustc-check-cfg`) + a parser test so a typo'd section can't silently emit nothing.

Registration checklist (do first — pre-commit catches misses 15 minutes late): `platforms/rp/Cargo.toml` board feature with forwards (`chip-rp2350`, core marker, `network-cyw43`, `lora-sx1262`), `picodroid-core/Cargo.toml` features, `.cargo/config.toml` aliases, `scripts/pre-commit` clippy matrix, `.github/workflows/ci_checks.yml`, website `build.md` / `limits.md` / `cargo-aliases.md` + README board table.

Verify: build via alias; `./scripts/sim.sh --app displaydemo --board meshnode_rp2350w`; **flash displaydemo + WiFi smoke with both HATs physically stacked** (proves display/touch/WiFi unaffected by the LoRa HAT present with floating CS); pre-commit.

### Session 3 — `mesh-protocol` crate: wire format + crypto (host-only)

Byte-exact encode/decrypt of real Meshtastic packets, zero hardware. New workspace crate:

- `wire.rs` — 16-byte header pack/unpack, flags bitfield.
- `crypto.rs` — AES-128/256-CTR wrapper, nonce builder, default PSK constant.
- `proto.rs` — hand-rolled protobuf; shared varint/length-delimited primitives; **unknown-field skip mandatory**; decoder must never panic (it feeds a radio) — return `Err`.
- `channel.rs` — channel-name hash, US915 frequency-slot calculation, LongFast preset constants.

API shape: encode into caller-provided `&mut [u8; 256]`, return length — no alloc anywhere. Golden vectors from Session 1 as unit tests + malformed-input tests.

Gotchas: little-endian header fields and nonce layout are the classic silent failures (the vectors exist to catch them); proto3 zero-field elision — assert byte-equality decrypt-side, semantic equality on re-encode if the Python lib's serialization choices prove fragile.

Verify: `cargo test -p mesh-protocol` under `./scripts/test.sh`; decrypt of the Python-generated text vector to the exact plaintext; pre-commit.

### Session 4 — SX1262 driver + device wiring + real-air RX proof

Highest-risk hardware unknown; everything after it is software.

- `picodroid-core/src/drivers/sx1262.rs` — pure chip driver generic over embedded-hal-style traits (`st7789.rs`/`xpt2046.rs` precedent): standby/config, sync word, frequency, buffer R/W, TX, continuous RX, IRQ status read/clear, RSSI/SNR, **init-without-hard-reset path** (GP15 policy). Host unit tests with a fake SPI asserting exact command bytes (`FakeXptSpi` precedent).
- New `hal::lora` seam in `picodroid-core/src/hal/` (device impl only this session).
- `platforms/rp/src/hal/rp/lora.rs` — wiring: `RpSpiBus::handle(1)` + third `SpiFreqSwitch` frequency, `RpOutputPin` CS, `RpInputPin` BUSY, DIO1 GPIO IRQ -> task-notify on core 1.
- Temporary debug "sniffer" task (becomes Session 5's radio task skeleton): configures LongFast, RXes continuously, logs header + `mesh-protocol`-decrypted payload + RSSI/SNR.

**Verify the Waveshare HAT's TCXO-on-DIO3 configuration against its schematic during the session.**

Verify: fake-bus host tests vs the datasheet; on HW, send a text from the phone via the stock node -> decrypted text + correct sender ID in the RTT log (proves pins, SPI sharing, RF params, slot, header parse, and crypto at once — the golden-vector-tested crypto localizes any failure to RF config); display keeps animating while packets arrive; pre-commit.

### Session 5 — Mesh engine: router, node DB, radio task — our node joins the mesh

Native-only mesh participation (no Java yet).

- `mesh-protocol/src/router.rs` + `nodedb.rs` as **pure state machines behind Clock/Rng traits**, host-tested FIRST with multi-node scenarios over an in-memory lossy ether (dedup, hop exhaustion, ack retry/backoff, self-rebroadcast suppression) — flood-routing correctness is proven here, not on air.
- `picodroid-core/src/hardware/mesh/`: `radio_task.rs` (replaces the sniffer; owns SX1262 + router, driven by DIO1 notifications + timeouts) and `mailbox.rs` (sensor-mailbox template: seqlock RX ring of 256 B records, TX request slots, node-DB snapshot cells).
- TRNG (`platforms/rp/src/hal/rp/trng.rs`) seeds packet_id + contention jitter. Node identity from the RP2350 unique flash ID; placeholder owner name until Java sets it. Rebroadcast ON by default.

If the session runs long, stub the want_ack retry path (broadcast + node DB are the demo-critical 80%).

Verify: host router tests; on HW — (1) our node appears in the phone app's node list with name within one beacon interval, (2) phone text -> our log, (3) native debug-hook text -> **appears in the phone app**, (4) relay demonstrated (hop-limit test or out-of-range topology); pre-commit.

### Session 6 — Sim mesh backend: UDP ether + synthetic peer

The whole stack runs under `sim.sh`; Sessions 7-8 iterate without a bench.

- `picodroid-core/src/hal/sim/lora.rs`: frames (header + ciphertext + simulated RSSI/SNR) on a loopback UDP ether so N sim instances mesh with each other; the radio task runs unchanged (std::thread, sampler precedent).
- Synthetic peer mode (env-gated, like the sensors' synthetic waves): a host thread playing a fake stock node — beacons NodeInfo, echoes texts after 1 s, acks DMs — implemented **using mesh-protocol** so it regression-tests encode paths too.
- Sim models the radio (encrypted frames), not a decoded side-channel — sim/device stay identical above the HAL seam. Env-var packet-loss injection, default 0.

Watch UDP port collisions between concurrent sim instances; enforce the 237 B payload cap exactly like the driver.

Verify: sim boots with the mesh task alive; two sim instances see each other's NodeInfo; synthetic-peer echo observed; `./scripts/test.sh`; pre-commit.

### Session 7 — Java SDK: `picodroid.mesh` + natives + `examples/meshdemo`

API frozen in Session 1; sketch:

```java
MeshManager.getInstance()
  void setOwner(String shortName, String longName)   // re-beacons NodeInfo
  void start(); void stop()
  int  sendText(String text)                          // broadcast, returns packetId
  int  sendText(int destNodeId, String text, boolean wantAck)
  void setMessageListener(MeshMessageListener l)      // null clears
  void setNodeListener(MeshNodeListener l)
  int  getNodeCount(); MeshNode getNode(int index)    // index-based, no List
  int  getMyNodeId()
MeshNode:    getNodeId/getShortName/getLongName/getSnr/getRssi/getLastHeardSeconds/getHopsAway
MeshMessage: getFromNodeId/getToNodeId/getText/getSnr/getRssi/getPacketId/isDirect  // RECYCLED - copy out
MeshMessageListener: onMessageReceived(MeshMessage m); onDeliveryStatus(int packetId, boolean delivered)
MeshNodeListener:    onNodeUpdated(int nodeId)
```

Native side: `picodroid-core/src/native_handler/mesh.rs` + dispatch hook in `native_handler/mod.rs`, `method_tables.rs` rows, `class_registry.rs` entries, appended `DISPATCH_SITES` positions, delivery drained from the `lifecycle.rs` tick per the sensor `deliver_event` pattern (incl. the StackOverflow -> `collect_now` -> redeliver-once and InvalidReference -> drop-registration error paths), `atomic_section` guards around compound heap ops.

**GC root provider FIRST** (a miss is a silent use-after-free under GC stress, not a build failure): `gc_root_registration.rs`, bump `EXPECTED_PROVIDERS`; then GC-stress on sim with mesh active. `framework_class_excludes` for `picodroid/mesh/*` on `testbench_rp2040` + re-verify the 896K release gate. **Sanitize non-ASCII in getText()/names** (emoji are common on Meshtastic; the LVGL font is ASCII-only).

`examples/meshdemo` via `./gradlew newApp`: single Activity — set owner, button sends a broadcast text, Toast incoming texts, log node updates. Deliberately thin; it is the API regression vehicle.

Verify: the three build-breaking guard tests pass in both shrink lanes; root-scan test at the bumped count; sim meshdemo + synthetic peer -> Toast; HW: phone -> Toast and meshdemo send -> phone app; rp2040 gate; pre-commit.

### Session 8 — Showcase app: `examples/meshchat`

The device a user actually holds. Multi-Activity touch app, 320x240, modeled on `examples/displaydemo` + `examples/keyboarddemo`:

- **ChatActivity** (home): append-only message list as LinearLayout-in-ScrollView (avoids the ~12-focusable-row ListView cap and `refreshFromAdapter()` focus resets on live lists), sender shortnames from the node DB, HH:MM timestamps, ack check-marks from `onDeliveryStatus`, compose bar = EditText + auto-popup Keyboard (mind keyboard overlay vs list height — give compose its own screen if cramped).
- **NodesActivity**: ListView of nodes (name/hops/SNR/last-heard), tap -> AlertDialog detail with "Send DM" (want_ack). Refresh on resume/user action, never on a timer while focused.
- Message history in an app-scoped singleton thread (picoenvmon NetworkManager precedent — deliberately not a Service), ring of ~30 messages copied out of the recycled event. NTP via one background `picodroid.concurrent.Thread` for wall-clock. Hold every callback-bearing View in Java fields.
- Ship checklist: `website/src/content/docs/examples.md` row (+ tutorial page and `astro.config.mjs` sidebar entry if we write one), README mention.

Verify: two sim instances chat with each other; synthetic-peer echo; HW acceptance script — phone -> stock node -> our screen, touchscreen-typed reply -> phone app, DM check-mark, node list with sane SNR; pre-commit.

### Session 9 — Interop hardening, soak, HIL, docs closure

- Interop edge cases against stock nodes: 237 B max-length texts both ways, want_ack retry exhaustion, hop_limit-0 packets, unknown portnums/proto fields silently ignored (node DB still updated), duplicate storms, node-DB eviction at 64.
- Overnight soak with beaconing stock nodes; `--mem-diag` watching for arena drift (goal: zero steady-state allocation in the RX -> deliver path); GC stress under traffic.
- HIL: mesh smoke row in `scripts/hil-tests.conf` (boot + radio-init self-test; the manual interop script documented alongside).
- Flash/RAM audit recorded in website `limits.md`; rp2040 gate re-confirmed. Design-doc Status updated + Amendments for every divergence.

Triage rule: anything affecting stock-node behavior toward us is fix-now; our-cosmetics-only can defer as Amendments. Soak failures here are usually seqlock races or SPI-lock/GP15 invariant erosion — re-read those invariants before chasing heap ghosts; reproduce routing jams in the Session 5 host harness, not on air.

## Codebase anchor points (for the implementing sessions)

- Template board: `platforms/rp/boards/testbench_rp2350w/board.toml`. Parser/codegen: `build_support/config.rs`, `build_support/board_cfg.rs`.
- Async-event + recycling pattern to clone: `picodroid-core/src/hardware/sensors/{sampler,mailbox,mod}.rs`; drain hook in `picodroid-core/src/lifecycle.rs`.
- Driver precedents: `picodroid-core/src/drivers/{st7789,xpt2046}.rs` (+ `SpiFreqSwitch` in `drivers/mod.rs`); device wiring `platforms/rp/src/hal/rp/{spi_bus,output_pin,input_pin,gpio}.rs`; DIO1 IRQ model = cyw43 hostwake in `gpio.rs`.
- Native exposure: `picodroid-core/src/native_handler/{class_registry,method_tables,mod}.rs`, `picodroid-core/src/dispatch_sites.rs`, `picodroid-core/src/gc_root_registration.rs` (EXPECTED_PROVIDERS).
- Design-doc house style: `docs/designs/net-typed-exceptions.md`; binary-format crate precedent: `pdb-protocol/`.
- Pin map on the new board: SPI1 GP10/11/12 shared; display DC=8 CS=9 BL=13 RST=15; touch CS=16 IRQ=17; LoRa CS=3 BUSY=2 DIO1=20 (RESET=GP15 shared — see GP15 policy); cyw43 GP23/24/25/29.

## End-to-end acceptance

Each session ends with `./scripts/pre-commit` green (+ `./scripts/test.sh` where host tests changed). The roadmap's acceptance test is Session 8's hardware script: a text typed in the phone app reaches our touchscreen via a stock Meshtastic node, a reply typed on our touchscreen appears in the phone app, a want_ack DM shows its delivered check-mark, and the node list shows the stock node(s) with plausible SNR/last-heard — followed by Session 9's overnight soak with zero steady-state heap growth.

## AMENDMENTS

(none yet)
