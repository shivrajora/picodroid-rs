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
simply before the 150 ms post-boot settle.

## NET-2: link-status flapping while a join is retrying

`cyw43_wifi_link_status` maps join-state `ACTIVE` (join *in progress*) to
`CYW43_LINK_JOIN`, and `NetworkInterface_CYW43.c` treats `>= LINK_JOIN` as
link-up. So while the chip is still associating — or looping through NONET
retries — +TCP's `pfInitialise` succeeds, DHCP starts, times out, the
interface drops, and the `net: down` hook fires repeatedly (dozens of lines
during a failed-join soak).

Once a join lands this settles and is purely cosmetic, but it wastes DHCP
traffic and makes RTT logs noisy. Fix direction: report link-up only when the
join state has all of `ACTIVE|AUTH|LINK|KEYED` (0x0e01) rather than bare
`ACTIVE`, either in the interface glue or as a fork patch to
`cyw43_wifi_link_status`. Watch out: DHCP currently *benefits* from starting
1–2 s early on successful joins, so re-check join→lease latency after.

## NET-3: upstream the `bsscfg:event_msgs` fix

The fork zeroes the bsscfg index in the event-mask iovar payload
(`cyw43_ll.c`, `cyw43_ll_bus_init`). Upstream leaves those 4 bytes as stale
buffer content, so whether async join events arrive at all depends on what
ioctl ran previously — on our boot sequence they never arrived. This is a
clean, universal bug worth a PR to georgerobotics/cyw43-driver; the
error-status logging patch may also be PR-worthy. Keeping the fork small
makes upstream rebases cheaper.

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

## NET-5: host-wake GPIO interrupt instead of 100 ms polling

`run_cyw43_task` polls every 100 ms (or on TX-side notifications). GP24 is
the chip's active-high host-wake line; routing it to a GPIO IRQ that fires
the existing task notification would cut RX latency from ~50 ms average to
near-zero and reduce idle wakeups. The hook stubs already exist
(`cyw43_hal_pin_config_irq_falling` is currently a no-op in `cyw43_port.c`).
Remember the polarity history: the wake line is ACTIVE-HIGH
(`INTERRUPT_POLARITY_HIGH`), so the "irq_falling" name from the vendored API
is misleading — configure a level-high or rising-edge interrupt.

## NET-6: real entropy for TCP ISNs and DHCP xids

`net_init.c` still uses a timer-seeded LCG for `xApplicationGetRandomNumber`
and `ulApplicationGetNextSequenceNumber`. Predictable ISNs are a real
(if LAN-scale) TCP-hijack concern. The RP2350 has a hardware TRNG
(0x400F0000); wire it up, with the LCG as fallback while the TRNG warms up.

## NET-7: HIL coverage for networking

Nightly `hil-run` has no networking row, so regressions in this
freshly-validated stack would go unnoticed until someone flashes a demo
manually. Needs: a netdemo (and ideally http_get) row in
`scripts/hil-tests.conf`, WiFi creds supplied to the nightly via environment
(never checked in), a persistent echo/HTTP listener on the HIL host, and the
example-target-IP question solved for CI (committed defaults are loopback;
the HIL flow must inject the host's LAN IP at build time, e.g. via a
`PICODROID_NET_TEST_HOST` env override in the examples).

## NET-8: WPA3

`cyw43_wifi_join` in the vendored driver already carries the SAE path
(`CYW43_AUTH_WPA3_SAE_AES_PSK`, `sae_password` iovar); our Rust wrapper only
exposes OPEN and WPA2_AES. Plumb an auth-mode selection through
`picodroid-core/src/drivers/cyw43.rs` (and decide how apps/boards express it
— likely another build-time env or board.toml key).

## NET-9: latent sockets-HAL leftovers (from the original audit)

Known, unfixed, low-priority:

- `socket_table.rs` 32-bit `remove()` is a no-op (leaks table slots on close
  in the 32-bit handle configuration).
- Socket I/O is chunked at 256 bytes per native call — correctness-fine,
  throughput-poor. NET-4 is done (bus now 37.5 MHz, ~30x faster), so this
  chunking is the remaining throughput bottleneck if anyone cares to measure.
- `http_connection.rs` maps every failure (DNS, connect, TLS-less refusal…)
  to `JvmError::InvalidReference`; worth distinct IOException messages now
  that the stack is real. During bring-up this cost a full debug cycle to
  see through. **Planned:** `Socket.connect` got a first catchable
  IOException in `a38d53c`; the full typed-exception design
  (ConnectException/SocketTimeoutException/UnknownHostException across the
  whole stack, plus the semantic NetError HAL rework it requires) is
  specced ready-to-execute in `docs/designs/net-typed-exceptions.md`.

## Validation environment (for whoever picks these up)

Flash + RTT recipe, chip-state readback tricks (GET_SSID/GET_BSSID/clmver/
country), the MicroPython-over-probe hardware exonerator, and the tcpdump
gate are written up in the auto-memory
(`reference_pico2w_wifi_debug_recipes`). Hard config invariants (do not
lower `CYW43_IOCTL_TIMEOUT_US` below 500 ms, do not override
`ipconfigBUFFER_PADDING`, keep `ipconfigINCLUDE_FULL_INET_ADDR=1`) are
commented at their definition sites in
`platforms/rp/src/hal/rp/port/cyw43_configport.h` and `FreeRTOSIPConfig.h`.
