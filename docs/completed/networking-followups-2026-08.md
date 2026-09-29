# Completed: Follow-up backlog: Pico 2 W networking — 2026-08-12

Items closed out of [networking-followups-2026-08.md](../networking-followups-2026-08.md), moved here on 2026-09-28 so the
original lists only open work. Text is as it stood when moved; ids keep their meaning.

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

## NET-5: host-wake GPIO interrupt instead of 100 ms polling — DONE 2026-08-15

GP24 now raises a **level-high** IO_IRQ_BANK0 interrupt
(`hal/rp/gpio.rs::hostwake`; the correct polarity — the wake line is
ACTIVE-HIGH despite the vendored hook's "irq_falling" name). pico-sdk
discipline: a level interrupt cannot be acked while the line is high, so
the ISR masks it and notifies the cyw43 task
(`picodroid_cyw43_hostwake_notify_from_isr`; PROC1 routing, so arm/ISR/
re-arm all run on core 1 — the banked NVIC makes core-0 routing
undeliverable from a core-1 init, the first attempt's HW-caught bug), and
`CYW43_POST_POLL_HOOK` re-arms it after every poll. Since 2026-09-02 the
shared handler branches on SIO CPUID first, so on the button board the
host-wake block is core-1-only by construction and a core-0 button edge
never touches `PROC1_INTE` (bank0 entry in `docs/quality-roadmap.md`).
Data toggling on the
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
same rows against loopback (landed 2026-09-04; the executed handover doc was deleted
2026-09-16 — `git log --all -- docs/nightly-networking-handover.md` recovers it). The
2026-08-15 status follows.


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
2026-08-15) was the since-deleted `docs/nightly-networking-handover.md`.

**2026-08-28 (`40411ec`): the board-parameterization third is closed.**
`hil-run.sh` takes `--board` (`:47`, help at `:63`) and the 2026-08-30 bug bash
used it in anger (`--board pico_enviro_mon_w`). What remains of NET-7 is the
creds-from-environment plumbing, the HIL-host listeners, and a `net` category —
`scripts/hil-tests.conf` still has none, and its only networking row
(`netexception`) is category `sim`, which hil-run skips.

## NET-10: dashboard page loads hang after the first byte (RST-on-close) — FIXED 2026-09-16

*Fix:* fix candidate (1) below, in `crates/picodroid-core/src/hal/freertos_tcp/mod.rs`
(`HalNet::close`). `FreeRTOS_shutdown(SHUT_RDWR)` first; when it returns 0 (only an
established TCP socket does) the socket is drained with a 50 ms receive timeout
until `recv` answers `-ENOTCONN` — FreeRTOS+TCP parks a socket in `eCLOSE_WAIT`
once its FIN is sent, ACKed and answered — or a 3.5 s bound expires; then
`FreeRTOS_closesocket` as before. The bound follows +TCP's retransmit clock (initial
SRTT 500 ms, doubling per resend: first resend of a lost last segment at ~1 s, second
~2 s later), because a closed socket retransmits nothing. Listeners, UDP sockets and sockets the peer
already closed fail the `shutdown` and take the old path unchanged. The FIN is a
normal segment and is retransmitted; the RST never was. The `pdb sysmon` task cap
was raised to 24 in the same change (docs/quality-roadmap.md).

*Verified 2026-09-17 on `pico_enviro_mon_w` with the curl repro below, same
session, same AP.* Baseline (old close path, 90 s from the join): 68 loads, 66
ended in a RST (curl exit 56), 2 hung 25 s after delivering the full 756-byte
body (exit 28) — the lost-RST case. With the fix: 959 loads over four runs (45 s,
180 s, 300 s steady-state and 90 s from the join), every one exit 0 with a clean
FIN, slowest 0.64 s, no hangs — except the one described under NET-11. `pdb
sysmon` on the same board now lists all 15 tasks (pdb task 1181 of 2048 words
free with the larger table).

## NET-11: the first dashboard load after a reboot loses its body — CLOSED 2026-09-19, bench artifact

*Not a firmware bug, and not the housekeeping job.* Opened 2026-09-17 while verifying NET-10
as "a load during the boot-time NTP + weather job gets its headers and never its body".
Investigated from scratch 2026-09-19 on `pico_enviro_mon_w` (main f717b80b, debug build, about
250 power cycles).

*What the hang is.* A CPU halt over SWD in the middle of a hang (nothing instrumented before
it) shows the socket `eESTABLISHED` with `bUserShutdown` set, every body byte segmented and
transmitted, each segment already sent three times (original, +0.9 s, +2.6 s; SRTT 438 ms, the
next resend due at about 6.1 s), the ARP cache and the header template holding the client's
correct MAC, `instr_tx_fail` 0 and the heap never under 140 KB free. The 3.5 s close drain then
expires and `closesocket` drops the connection without a FIN or RST, which is why the client
waits out its own timeout. The app logs nothing because nothing failed on the device.

*Cause.* The bench host was attached to the dashboard LAN twice: `wlp82s0` (192.168.1.215) and
the wired `enp81s0` (10.0.0.1/24, the BACnet bench network) share one L2 segment, and with the
Linux defaults `arp_ignore=0` / `arp_announce=0` the host answers an ARP request for any of its
addresses on every interface (ARP flux). Only the first connection after a Pico reboot is
exposed, because only then is the Pico's ARP cache empty and it has to broadcast. A capture on
both NICs shows the sequence: the broadcast reaches the wired NIC first and is answered with the
wired MAC, so the ping reply, the SYN-ACK and the first ACK go to the wired NIC; 17 ms later the
same broadcast arrives over WiFi and is answered with the WiFi MAC, the Pico's entry flips, the
63-byte header goes to the WiFi MAC and arrives, and the three body segments with both
retransmissions of each arrive on neither NIC. One TCP flow that changes destination MAC
mid-stream is dropped inside the LAN (the gateway pins a flow to the port it first saw it on —
inferred, the gateway is a black box). Loads whose ARP replies arrive in the other order, or
after the entry has settled, are clean, which is why it looked random and why it looked tied to
the first seconds after `net: up`.

*Numbers.* Load at the first ping reply, host defaults: 22 hangs in 90 boots. First load 1.5 s
later, squarely inside the NTP + weather job: 0 in 26. Host with `arp_ignore=1`,
`arp_announce=2`: 0 in 29, one ARP reply per boot and every frame on `wlp82s0`; defaults
restored: hung again on the third boot. Probe-reset boots and every instrumented firmware (a few
microseconds in `close()`, RTT debug prints, an in-RAM event recorder) never hung: the window is
the millisecond ordering of two ARP replies against the page's segments, and any of those shifts
it.

*Bench rule.* A host that tests a board over the network must answer ARP on one interface only:
`sysctl net.ipv4.conf.all.arp_ignore=1 net.ipv4.conf.all.arp_announce=2` (persist it under
`/etc/sysctl.d/`), or keep its other NICs off the board's segment. Until then the first
connection after a board reboot is suspect in any curl loop run "from the join", NET-10's
included.

*Left as they are.* FreeRTOS+TCP taking the newest ARP reply is ordinary behaviour. The 3.5 s
drain does what NET-10 sized it for (two retransmits); a path that stays dead longer than that
ends as a silent close, and a RST sent into the same dead path would not arrive either.

*Seen on the side:* the join failures recorded as NET-12 below.

*Tooling that worked:* `probe-rs gdb` plus `gdb-multiarch -batch` (`set language c`; walk
`xBoundTCPSocketsList`, print `u.xTCP.bits`, the tx stream's head/mid/tail, each wait-queue
segment's `ucTransmitCount`, `xARPCache`) halts within a second and perturbs nothing beforehand;
host-side, `tcpdump -e` on each NIC separately (`-i any` hides the destination MAC) and the
per-interface `rx_packets` counters.

### NET-10, original report (2026-09-04)

Found while verifying the serve-loop fix (`fix/dashboard-stall`, 2026-09-04).
On the W board about 1 page load in 40 delivers its headers within 0.4 s and
then hangs until the client gives up (curl `-m 25`, exit 28): the rest of the
body and the close never arrive. It happens with no other socket active
(2 of 113 idle loads) and at a similar rate while the NTP/weather job runs
(3 of ~160), so the concurrent fetch is not the cause. Never seen in the sim
(host TCP stack).

Mechanism (consistent with every observation, not yet proven on the wire):
`tcp_close` calls `FreeRTOS_closesocket` on a connected socket with no
`FreeRTOS_shutdown`, so the stack aborts the connection with a RST — every
successful load already ends with curl exit 56 ("RST after the body",
docs/mem-session-2026-08.md). A RST is sent once and never retransmitted; if
it or the last data segment is lost, the client waits forever. Raising
`net_buffer_descriptors` 8 → 16 on the W board changed nothing, so it is not
descriptor exhaustion.

Fix candidates: (1) graceful close in the HAL — `FreeRTOS_shutdown(SHUT_RDWR)`,
drain `recv` to EOF/EINVAL under a short bound, then `closesocket`, so the FIN
is retransmitted like any segment; (2) if the RST close stays, wait for the TX
buffer to drain before closing. Repro: `curl -s -m 25 -o /dev/null -w
"%{http_code} %{exitcode} %{time_starttransfer} %{time_total}\n"
http://<board>:8080/` in a 0.3 s loop for 2 min; a hang reads `200 28 0.3 25.0`.
`pdb sysmon` cannot help on this board until its task cap is fixed
(docs/quality-roadmap.md, "`pdb sysmon` shows no task table on the W board").
