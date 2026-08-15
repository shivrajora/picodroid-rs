---
title: "Known issues & current limits"
description: "User-visible limitations in the current release: networking constraints, simulator/hardware gaps, and platform caveats."
---

What doesn't work (yet), as of v0.12.0. Items here are confirmed and tracked — not speculative.

## Networking (Pico 2 W)

- **Open and WPA2-AES networks only.** No WPA3, no WPA2-Enterprise. IPv4 only.
- **No TLS.** `HttpURLConnection` speaks plain HTTP; there is no `https://` support.
- **Coarse error reporting.** Failed DNS lookups, refused connections, and timeouts currently surface as a single generic exception whose message carries little detail. Distinguishing failure causes from Java is not yet possible.
- **Socket throughput is chunked.** Socket I/O crosses the native boundary in 256-byte chunks; large transfers work correctly but pay a per-chunk cost.
- **Socket slots are not reclaimed on device.** On hardware, closing a socket does not yet free its slot in the internal socket table — apps that open and close sockets in a long loop will eventually exhaust the table. (The simulator is unaffected.)
- **A retrying join is noisy.** If the configured network is out of range or the password is wrong, the join retries and emits repeated `net: down` lines over RTT. This is log spam, not a crash.
- **Boot-time race.** The network takes up to ~10 s after reset (WiFi join + DHCP); apps must poll `NetworkInfo.isConnected()` before their first socket call — see [WiFi & networking setup](/get-started/networking/).

## Simulator ↔ hardware gaps

- **The sim is single-core.** The simulator runs the real FreeRTOS kernel, but on the single-core POSIX port — cross-core interactions (and cross-core races) only exist on hardware.
- **Finished threads park instead of exiting.** A sim thread whose `run()` returns leaves its FreeRTOS task parked; an app churning through tens of thousands of short-lived threads will exhaust host threads. Long-running worker threads are unaffected.
- **Sim networking is the host stack.** `picodroid.net` in the simulator uses your machine's network directly — connection timing, buffer limits, and error codes differ from the device's FreeRTOS+TCP stack.
- **macOS is untested** since the simulator moved onto the FreeRTOS scheduler. Linux (windowed and headless) is exercised continuously.

## Platform

- **RP2040 flash is nearly full.** A `--release` RP2040 image sits at ~99% of the 896 K program region. `scripts/build.sh` handles this (it disables LTO on RP2040, which paradoxically shrinks the image); a raw `cargo build --release` for RP2040 can overflow FLASH at link time.
- **Stale-widget detection is sim-first.** The generation-tagged widget handle table that catches use-after-delete runs in the 64-bit simulator; on 32-bit devices it is staged behind the default-off `handle-table-32` feature pending a hardware soak. A stale-handle bug can therefore surface on hardware without reproducing in the sim.
- **BME688 gas resistance is constant on hardware.** The gas sensor's heater profile is never programmed, so gas/IAQ readings sit at a fixed value on the device (temperature, humidity, and pressure are fine). Affects the picoenvmon IAQ tile cosmetically.

## Where these are tracked

Networking items carry NET-* IDs in [`docs/networking-followups-2026-08.md`](https://github.com/shivrajora/picodroid-rs/blob/main/docs/networking-followups-2026-08.md); broader quality items live in [`docs/quality-roadmap.md`](https://github.com/shivrajora/picodroid-rs/blob/main/docs/quality-roadmap.md).
