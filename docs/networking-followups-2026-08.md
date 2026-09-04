# Follow-up backlog: Pico 2 W networking — 2026-08-12

The FreeRTOS+TCP + cyw43 stack was brought up and validated end-to-end on a
real Pico 2 W (`testbench_rp2350w`) on 2026-08-12: WPA2 join in ~6 s, DHCP
lease ~10 s after boot, `netdemo` TCP echo and `http_get` GET+POST both pass
against a LAN host. The bring-up fixed 20 defects across the driver FFI, gSPI
transport, RP2350 SMP shim, sockets HAL, and +TCP configuration — see commits
`d66882b`, `e7210a8`, `b87396d` and the fork
[shivrajora/cyw43-driver@`picodroid`](https://github.com/shivrajora/cyw43-driver/tree/picodroid).

What remains is **follow-up work, not blockers**. Each item below is
self-contained: evidence, impact, and where to start. None of them prevents
the demos from passing today.

## NET-1: `apsta` / `ampdu_rx_factor` iovars fail with BCME -5 (NOTDOWN)

During `cyw43_ll_bus_init`, two of the fire-and-forget config iovars are
rejected by the firmware:

```text
cyw43: ioctl cmd 263 error status -5 payload 617073746100010000000000   ("apsta\0" 1)
cyw43: ioctl cmd 263 error status -5 payload 616d7064755f72785f666163   ("ampdu_rx_fac…")
```

BCME -5 is `NOTDOWN` — the WL core claims to be up when these are set, even
though they run before the explicit `WLC_UP`. Only visible at all because the
fork now logs firmware error statuses (upstream silently discards them, so
stock Pico W setups likely hit this too). Joins, DHCP, TCP, and HTTP all work
regardless; `apsta` matters only for concurrent AP+STA mode and
`ampdu_rx_factor` is a throughput tunable.

Start by comparing against pico-sdk's boot on the same firmware blob (does it
get -5 too?), then test moving the two iovars after an explicit `WLC_DOWN` or
simply before the 150 ms post-boot settle. Re-confirmed still present at
every boot on 2026-08-15; queued with cost/tradeoff notes in
`docs/quality-roadmap.md` § Networking follow-ups.

## NET-2: link-status flapping while a join is retrying — DONE 2026-08-15

Fixed in the in-repo glue, no fork change. The suggested 0x0e01 join-state
mask turned out not to work: on full join the driver *resets*
`wifi_join_state` to bare `ACTIVE` (`cyw43_ctrl.c`, the
`WIFI_JOIN_STATE_ALL` collapse), so post-join the mask is indistinguishable
from join-in-progress. Instead `NetworkInterface_CYW43.c` now gates on
`xInterfaceUp` — which the driver's own `cyw43_cb_tcpip_set_link_up`
callback sets only when the join fully completes (assoc + link + keys) —
ANDed with `link_status >= CYW43_LINK_JOIN` to catch failure kinds that
deliver no EV_LINK down event. DHCP therefore starts at full join rather
than 1–2 s earlier during association; join→lease latency re-checked on HW
(see validation notes).

## NET-3: upstream the `bsscfg:event_msgs` fix — PR PREPARED 2026-08-15

Branch `upstream-bsscfg-event-msgs` in `third_party/cyw43-driver` carries the
isolated, marker-free fix rebased onto the fork's upstream base; the full
handover (pre-flight refresh against upstream main, submission commands,
PR body, post-merge rebase guidance) is `docs/upstream-cyw43-bsscfg-pr.md`.
Submission is deliberately left manual.
The error-status logging patch is not bundled (log-volume change on every
port; propose separately if the first PR lands).

## NET-4: PIO gSPI transport — DONE 2026-08-14

Implemented in Rust as `platforms/rp/src/hal/rp/pio_spi.rs` (PIO0 SM0 + DMA
channels 4/5, pico-sdk `spi_gap01_sample0` semantics), replacing the deleted
bit-bang `cyw43_bus_spi.c`. The bus now runs at 37.5 MHz (150 MHz / clkdiv 2
/ 2 cycles-per-bit) and the per-transfer PRIMASK guard is gone entirely —
frames complete autonomously in hardware, which is what allowed the cyw43
task to move to core 1 (`boot_tasks.rs`). Validated on HW: blinky-loaded
core 0 + DHCP, http_get end-to-end TCP, 10/10 pdb-install soak. See
`docs/designs/cyw43-pio-transport.md` for the full history and debug
recipes. With atomic sections gone, NET-5 is now unblocked.

## Vendored FreeRTOS+TCP is now a fork (2026-08-15)

`third_party/freertos-plus-tcp` points at the `picodroid` branch of
`shivrajora/FreeRTOS-Plus-TCP` (V4.4.1 + `e43e446f`), mirroring the
cyw43-driver arrangement, with the same `PICODROID`-marker build assertion
in `build_support/network.rs`. The carried fix: an RST received in SYN-SENT
(peer refuses the connection) transitioned the socket to `eCLOSED`, but
`vTCPStateChange()` only wakes a task blocked in `FreeRTOS_connect()` on the
`eCONNECT_SYN → eCLOSE_WAIT` transition — so with our (default, infinite)
socket block time, `Socket.connect` to a reachable host with a closed port
**hung forever** instead of failing in one RTT. Diagnosed by tcpdump (SYN →
RST in 24 µs, no SYN retransmission, no app wake). The bug is present on
upstream `main` as of 2026-08-15 (last change to `FreeRTOS_TCP_IP.c` is the
v4.4.1 release itself) — but upstream has it in flight: **open PR #1355**
(issue #1301) fixes the same wake-gate defect the RFC-793 way (gate accepts
`eCLOSED`, SYN-retry exhaustion moves to `eCLOSED`; its unit test names the
RST-while-connecting case verbatim). Same app-visible outcome as our patch.
**Rebase guidance:** once #1355 is in a release, drop `e43e446f` and take
upstream — do not carry both (our patch reroutes RST to `eCLOSE_WAIT`;
theirs makes `eCLOSED` wake correctly; combining is harmless but ours
becomes dead weight). **Error-mapping consequence (HW-verified
2026-08-15):** with an infinite socket block time, `FreeRTOS_connect`
returns `-ENOTCONN` (-128) for *every* aborted connect — peer RST,
ARP-resolution give-up (`prvTCPPrepareConnect_IPV4` counts each 500 ms
cache miss against the same `ucRepCount`, so an unresolvable host aborts
in ≈1.5 s), and SYN-retransmission exhaustion (≥9 s) all converge on
`eCLOSE_WAIT`; `-ETIMEDOUT` (-116) only appears when a finite block time
expires first. The HAL classifies -128 by elapsed time (`tcp_connect` in
`picodroid-core/src/hal/freertos_tcp/mod.rs` (was `platforms/rp/src/hal/rp/net.rs`): <1 s → Refused, ≤6 s → Unreachable
(NoRouteToHostException), else TimedOut — the stack's timing ladder keeps
the causes far apart). If the upstream rebase changes which state an
aborted connect lands in, re-verify all three netdemo failure cases on
HW.

## NET-5: host-wake GPIO interrupt instead of 100 ms polling — DONE 2026-08-15

GP24 now raises a **level-high** IO_IRQ_BANK0 interrupt
(`hal/rp/gpio.rs::hostwake`; the correct polarity — the wake line is
ACTIVE-HIGH despite the vendored hook's "irq_falling" name). pico-sdk
discipline: a level interrupt cannot be acked while the line is high, so
the ISR masks it and notifies the cyw43 task
(`picodroid_cyw43_hostwake_notify_from_isr`; PROC1 routing, so arm/ISR/
re-arm all run on core 1 — the banked NVIC makes core-0 routing
undeliverable from a core-1 init, the first attempt's HW-caught bug), and
`CYW43_POST_POLL_HOOK` re-arms it after every poll. Data toggling on the
shared PIO DATA pad can fire it spuriously mid-transfer, but mask-on-fire
bounds that to one extra workless poll. The poll timeout is now a 1 s
safety net (was the sole 100 ms RX path);
`instr_hostwake_irqs` (cyw43_port.c) counts IRQ-path wakes for gdb.
The `cyw43_hal_pin_config_irq_falling` stub stays a no-op — the driver
only calls it on the SDIO path.

## NET-6: real entropy for TCP ISNs and DHCP xids — DONE 2026-08-15

`hal/rp/trng.rs` drives the RP2350 TRNG via the rp235x-pac register block
(sw-reset, conservative 50k-cycle sample period, health-test recovery) and
buffers each 192-bit EHR harvest as six words. `xApplicationGetRandomNumber`
consumes them via `picodroid_trng_random_u32` — non-blocking: while a
harvest is still sampling the timer-seeded LCG fills in, and every TRNG
word XOR-mixes into the LCG state so even the fallback stream stops being
predictable after the first harvest.

## NET-7: HIL coverage for networking — DONE 2026-09-04

**2026-09-04: the device half landed.** `scripts/hil-tests.conf` has a `net`
category with two rows (`netdemo`, `http_get`) built as `testbench_rp2350w`
firmware. `hil-run.sh` reads `.wifi-creds.env` into the firmware build,
bakes the host's LAN IP into the app, and runs the echo (7000) and HTTP
(8000) servers itself (`lib.sh::start_net_listeners`); `sim-run.sh` runs the
same rows against loopback. Details and the pattern correction are in
`docs/nightly-networking-handover.md`. The 2026-08-15 status follows.


Landed:

- **`netexception` roster row** (`sim` category, new): deterministic typed-
  exception assertions run in the nightly sim suite in both shrink modes,
  with a board-override column selecting the network-enabled W-board sim
  build. `sim` rows are skipped by `hil-run` (the HIL testbench board has
  no network stack).
- **Build-time target-IP injection**: `picodroidNetTest { enabled = true }`
  in an example's build.gradle.kts generates `NetTestConfig.java`; the host
  comes from `-PpicodroidNetTestHost` / `PICODROID_NET_TEST_HOST` (default
  loopback). netdemo and http_get consume it, so pointing them at a real
  host is `PICODROID_NET_TEST_HOST=<ip> ./scripts/build-apk.sh --app
  netdemo` — no source edit.

Still open for on-device nightly rows: WiFi creds supplied to the nightly
via environment (never checked in), listeners on the HIL host, and ~~hil-run
board parameterization~~. Full execution plan (with the load-bearing fact
that the attached HIL board is physically a Pico 2 W, verified
2026-08-15): `docs/nightly-networking-handover.md`.

**2026-08-28 (`40411ec`): the board-parameterization third is closed.**
`hil-run.sh` takes `--board` (`:47`, help at `:63`) and the 2026-08-30 bug bash
used it in anger (`--board pico_enviro_mon_w`). What remains of NET-7 is the
creds-from-environment plumbing, the HIL-host listeners, and a `net` category —
`scripts/hil-tests.conf` still has none, and its only networking row
(`netexception`) is category `sim`, which hil-run skips.

## NET-8: WPA3 — DONE 2026-08-15 (needs a WPA3 AP to validate)

`drivers/cyw43.rs` exposes `WPA3_SAE_AES` / `WPA3_WPA2_AES` and
`wifi_join` takes an auth override; `PICODROID_WIFI_AUTH`
(`open|wpa2|wpa3|wpa2wpa3`, unset = historical automatic choice) selects it
at build time in `hal/rp/cyw43/link.rs` (was `wifi_task.rs`). `platforms/rp/build.rs` now emits
`rerun-if-env-changed` for SSID/PASS/AUTH — previously a credential change
was a cargo no-op. Untested against a real WPA3 AP (none on the bench);
WPA2 verified unaffected on HW.

## NET-9: latent sockets-HAL leftovers (from the original audit)

- **Handle tables — FIXED 2026-08-15.** `socket_table`/`http_table` now
  share a slot-reusing `net/ptr_table.rs` on every pointer width. This
  closed two defects at once: the 64-bit tables never reused slots (a
  create/close loop exhausted them), and the 32-bit arms handed the raw
  pointer to Java with a no-op `remove`, making close-then-use a dangling
  dereference into FreeRTOS+TCP (device-only, sim-invisible — the
  pre-generational handle_table hazard class). A stale handle now resolves
  to null → catchable `SocketException("Socket is closed")`.
- Socket I/O is chunked at 256 bytes per native call — correctness-fine,
  throughput-poor, **still open by choice**: the chunk buffers live on the
  JVM task stack, so raising them should follow a measurement, not
  precede one. NET-4's 37.5 MHz bus makes this the remaining throughput
  bottleneck if anyone cares to measure. Queued with cost/tradeoff notes
  in `docs/quality-roadmap.md` § Networking follow-ups.
- **Typed exceptions — DONE 2026-08-15.** `docs/designs/net-typed-exceptions.md`
  executed in full: semantic `NetErrorKind` across the HAL (sim + device +
  test platform), the normalized `tcp_recv` contract (`Ok(0)` = EOF,
  timeout throws — fixing the device/sim inversion), typed
  `java.net` exceptions with Android wording across Socket/ServerSocket/
  DatagramSocket/HTTP, `InetAddress.getByName`, `ServerSocket.setSoTimeout`,
  SDK throws clauses, the `netexception` sim-roster example, and the
  exception-taxonomy section in `website/.../api/networking.md`.

## Validation environment (for whoever picks these up)

Flash + RTT recipe, chip-state readback tricks (GET_SSID/GET_BSSID/clmver/
country), the MicroPython-over-probe hardware exonerator, and the tcpdump
gate are written up in the auto-memory
(`reference_pico2w_wifi_debug_recipes`). Hard config invariants (do not
lower `CYW43_IOCTL_TIMEOUT_US` below 500 ms, do not override
`ipconfigBUFFER_PADDING`, keep `ipconfigINCLUDE_FULL_INET_ADDR=1`) are
commented at their definition sites in
`platforms/rp/src/hal/rp/port/cyw43_configport.h` and
`picodroid-core/net-freertos-tcp/FreeRTOSIPConfig.h` (shared since the
network-seam work; host tests pin them).
