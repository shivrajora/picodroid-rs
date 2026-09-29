---
title: "Known issues & current limits"
description: "User-visible limitations in the current release: networking constraints, concurrency limits, simulator/hardware gaps, and platform caveats."
---

What doesn't work (yet), as of v0.35.0 and `main` since. Items here are confirmed and tracked — not speculative.

## Networking (Pico 2 W)

- **Personal auth only.** Open, WPA2-AES, and WPA3-SAE (`PICODROID_WIFI_AUTH`) — no WPA2/WPA3-Enterprise. IPv4 only.
- **TLS is RP2350-only and minimal.** `https://` works on boards with `has_tls = true` (every
  RP2350 WiFi board; the RP2040 has no flash for it): TLS 1.3, `TLS_AES_128_GCM_SHA256`, P-256
  key exchange, a compiled-in store of 14 roots, no client certificates, no session resumption.
  A handshake is 1.1–2.0 s and needs the wall clock set (`SntpClient`), or it is refused. See
  [HTTPS](/api/networking/#https).
- **Socket throughput is chunked.** Socket I/O crosses the native boundary in 256-byte chunks; large transfers work correctly but pay a per-chunk cost.
- **The device closes HTTP connections with RST.** After a complete response the FreeRTOS+TCP side resets rather than closing cleanly, so a client sees a transport error alongside a full, correct payload (`curl` exits 56 with `http_code` 200). Scripts probing a device HTTP server must judge success by the status code and body, not the client's exit code. Simulator connections close normally.
- **Boot-time race.** The network takes up to ~10 s after reset (WiFi join + DHCP); an app opens its first socket from a `ConnectivityManager.NetworkCallback`'s `onAvailable`, or, with no Activity to deliver callbacks, polls `NetworkInfo.isConnected()` first — see [WiFi & networking setup](/get-started/networking/).
- **No `LinkProperties`.** `ConnectivityManager` has Android's `NetworkCallback`, `Network`, `NetworkCapabilities` and `NetworkRequest`, but no `LinkProperties` / `onLinkPropertiesChanged`: the address is `NetworkInfo.getIpAddress()`, and a DHCP renewal that changes it fires no callback. `onLosing` / `onUnavailable` are never called.

## Concurrency

- **Equal-priority threads do not round-robin.** Only one task interprets Java at a time: every Java task runs at one FreeRTOS priority with time slicing off, and since 2026-09-15 holds a kernel mutex (the JVM run lock) that it gives up only where it blocks — that is what keeps the shared heap safe without per-object locks — so a thread yields only when it blocks. A compute-bound thread therefore starves its siblings until it calls `sleep`, `join`, `wait`, or blocking I/O. A safepoint-yield fix is designed but ungated on performance; see `docs/quality-roadmap.md`.
- **`Thread.setPriority` is advisory.** The value is stored and reported back, never applied, for the same reason.
- **`volatile` is ignored and the interpreter emits no memory barriers.** Correct only while a single core interprets Java, which is the invariant every JVM-adjacent task is pinned to. As of 2026-08-31 it is enforced rather than remembered: `platforms/rp/src/task_affinity.rs` is the only way the RP family creates a task — it names the core and makes create+pin scheduler-atomic — and its source scan fails `scripts/test.sh` for any spawn that bypasses it. What remains is `volatile` itself, which parses and is ignored — tracked in `docs/quality-roadmap.md` (THR-04 / X1 in `docs/parity-audit.md` records the trace).
- **`java.util.concurrent` is excluded on `testbench_rp2040`.** The board drops the `picodroid.concurrent` pool, `Future`, atomic and latch classes to stay inside its flash budget, so an app using them fails to resolve there rather than failing at runtime. `Thread`, `synchronized` and `Object.wait`/`notify` are available on every board.
- **`picodroid.json` is a board capability.** Only boards with `has_json = true` in their `board.toml` ship `JSONObject`/`JSONArray`/`JSONException` (every RP2350 board today; not `testbench_rp2040`). The native node pool behind them is capped at 2048 nodes and 16 KiB of string bytes across all live documents: a larger parse throws `JSONException`, a larger `put` throws `OutOfMemoryError`.
- **`picodroid.protobuf` is a board capability.** Only boards with `has_protobuf = true` in their `board.toml` ship `CodedInputStream`/`CodedOutputStream` and the classes beside them (every RP2350 board today; not `testbench_rp2040`). An app built for a board without it fails `verifyApiContract` rather than resolving on the device.
- **Spurious wakeups are possible.** `Object.wait` may return without a matching `notify`, as the JLS allows — always wait in a loop over the condition.

## Simulator ↔ hardware gaps

- **The sim is single-core.** The simulator runs the real FreeRTOS kernel, but on the single-core POSIX port — cross-core interactions (and cross-core races) only exist on hardware.
- **Finished threads park instead of exiting.** A sim thread whose `run()` returns leaves its FreeRTOS task parked; an app churning through tens of thousands of short-lived threads will exhaust host threads. Long-running worker threads are unaffected.
- **Sim networking is the host stack.** `picodroid.net` in the simulator uses your machine's network directly — connection timing, buffer limits, and error codes differ from the device's FreeRTOS+TCP stack.
- **macOS is untested** since the simulator moved onto the FreeRTOS scheduler. Linux (windowed and headless) is exercised continuously.

## Platform

- **RP2040 flash is tight.** A `--release` `testbench_rp2040` image sits at 81% of the 1152 K program region (964,704 of 1,179,392 bytes for `helloworld` at the 2026-09-28 size baseline), and a committed size ratchet gates any growth. The region was 896 K until the framework's class link tables moved into flash and overflowed it by about 40 KB; the board's app region gave up 256 KB (1024 → 768 KB) to make the room. `scripts/build.sh` disables LTO on RP2040 (which paradoxically shrinks the image) and leaves line numbers out of its debug image; a raw `cargo build --release` for RP2040 links a larger image than the one the scripts measure.
- **A PAPK packed before 2026-09-28 does not install.** The package format is major version 2 (classes carry link tables built at pack time) and there is no reader for version 1: `pdb install` refuses the file on the host, a firmware build refuses to embed it, and a board does not load a version 1 image left in its app region. Re-pack the app with the current toolchain (`./scripts/build-apk.sh`).
- **BME688 gas resistance is constant on hardware.** The gas sensor's heater profile is never programmed, so gas/IAQ readings sit at a fixed value on the device (temperature, humidity, and pressure are fine). Affects the picoenvmon IAQ tile cosmetically.

## Where these are tracked

Networking items carry NET-* IDs in [`docs/networking-followups-2026-08.md`](https://github.com/shivrajora/picodroid-rs/blob/main/docs/networking-followups-2026-08.md); broader quality items live in [`docs/quality-roadmap.md`](https://github.com/shivrajora/picodroid-rs/blob/main/docs/quality-roadmap.md).
