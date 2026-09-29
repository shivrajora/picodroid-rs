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

Completed items: [completed/networking-followups-2026-08.md](completed/networking-followups-2026-08.md) — NET-2, NET-4, NET-5, NET-6, NET-7, NET-10, NET-11.

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

## NET-3: upstream the `bsscfg:event_msgs` fix — PR PREPARED 2026-08-15

Branch `upstream-bsscfg-event-msgs` in `third_party/cyw43-driver` carries the
isolated, marker-free fix rebased onto the fork's upstream base; the full
handover (pre-flight refresh against upstream main, submission commands,
PR body, post-merge rebase guidance) is `docs/upstream-cyw43-bsscfg-pr.md`.
Submission is deliberately left manual.
The error-status logging patch is not bundled (log-volume change on every
port; propose separately if the first PR lands).

*2026-09-16:* still unsubmitted. The fork now tracks upstream v2.0.0
(`cee4d9e0`, MIT-licensed), which does not contain the fix; the prepared
branch needs a rebase that `git merge-tree` reports as clean. Details in the
handover's amendment.

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

## NET-8: WPA3 — BUILT 2026-08-15, NEEDS VERIFICATION

**Needs verification:** join a real WPA3 (SAE) AP with `PICODROID_WIFI_AUTH=wpa3` and
`wpa2wpa3` before this moves to completed.

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

## NET-12: the board sometimes never joins WiFi after a power cycle — OPEN 2026-09-19

Seen while chasing NET-11 on `pico_enviro_mon_w` (main f717b80b, debug build): 8 of about 250
power cycles never answered a ping within 90 s. The log shows `net: down` a few seconds after
`wifi: join ... requested`, then the app's `net: still no network after 30s`, and no `net: up`
afterwards; one such boot stayed down 11 minutes until a probe reset brought it back, so the
retry path did not recover by itself. The next power cycle always joined. Nothing else is known:
whether the join request fails, the driver never retries, or the AP refuses the association has
not been looked at, and neither has whether a probe-reset boot (chip not power-cycled) shows it.
The ARP fix for NET-11 (bench rule in
[completed/networking-followups-2026-08.md](completed/networking-followups-2026-08.md)) does not touch it: 1 of the 30 boots run with that fix in place failed
the same way. First step: catch one with RTT attached at `DEFMT_LOG=debug` and read the cyw43
join-state word and link status (recipes in the validation notes below); `scratchpad`-style
harness = power cycle, ping with a 90 s limit, keep the RTT log of the boots that time out.

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
