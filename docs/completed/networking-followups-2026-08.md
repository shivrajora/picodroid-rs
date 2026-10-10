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

## NET-12: the board sometimes never joins WiFi after a power cycle — FIXED 2026-10-09

*As opened (2026-09-19).* Seen while chasing NET-11 on `pico_enviro_mon_w` (main f717b80b,
debug build): 8 of about 250 power cycles never answered a ping within 90 s. The log showed
`net: down` a few seconds after `wifi: join ... requested`, then the app's `net: still no
network after 30s`, and no `net: up` afterwards; one such boot stayed down 11 minutes until a
probe reset brought it back. The next power cycle always joined.

*Cause, in two halves.* The link driver (`platforms/rp/src/hal/rp/cyw43/link.rs`) issued
exactly one join per request and then only mirrored the chip's verdict, and the vendored
cyw43 driver retries nothing after a verdict of its own. So any join that ended without the
station associated — NONET because the access point missed the probe, a deauth, a link lost
during the handshake, or no verdict at all — left the board off the network until the next
boot. Which of those it was could not be told from `net: down` (that line is only the IP
stack's first 3-second initialisation retry finding the link not up; it says nothing about
the join), so the fix started with evidence: the chip's async events are now logged on every
build (`cyw43: [<ms>] ASYNC(<flags>,<NAME>,<status>,<reason>,<itf>)`, the driver's own
`CYW43_TRACE_ASYNC_EV` dump through a now line-buffered log shim), and the supervisor below
logs the driver's join-state word when it retries.

The boot that failed on the bench (cycle 13 of the first instrumented run) read:

```text
cyw43: [  8766] ASYNC(…,PSK_SUP,4,15,0)     supplicant AUTHENTICATED, reason WPA_PSK_TMO:
                                               the 4-way handshake timed out (M1 never came)
cyw43: [  9705] ASYNC(…,AUTH,2,0,0)         the driver's own rejoin: an auth timeout, ignored
cyw43: [  9705] ASYNC(…,PSK_SUP,4,0,0)      AUTHENTICATED again, reason OTHER
wifi: join failed: bad password
wifi: rejoin "…" (bad password; attempt 1, join state 0x4)
wifi: join "…" requested (retry)
cyw43: …ASSOC, LINK, PSK_SUP,6 (KEYED)…
wifi: associated
net: up, ip 192.168.1.121                      6 s after the verdict
```

`cyw43_ctrl.c`'s `EV_PSK_SUP` handler treats status 4/8/10 with reason 15 as a timeout and
schedules its internal rejoin, but files every other status/reason pair — including the
plain "authenticated, reason other" progress event the rejoin produces — as
`WIFI_JOIN_STATE_BADAUTH`. Nothing retries after BADAUTH, so the join state sat at 0x4 for
ever. The handshake timeout itself is the access point under load (the bench AP serves the
house); the chip is 2.4 GHz only and the AP is shared, so a few percent of boots catching an
EAPOL timeout is unsurprising. The "11 minutes until a probe reset" boot is the same state:
only a re-init of the chip ever issued another join.

*Fix.* `crates/picodroid-core/src/hal/wifi_join.rs` — `JoinSupervisor`, a pure policy with
host tests, fed the mirrored station status by any link driver: NoNet / Fail / Down (lost
after being up, or dropped during the join) / no verdict within 15 s (then a leave first, so
a stale join-state word cannot swallow the new attempt) → rejoin after 3 s, doubling to 60 s
(NoNet, a missed probe: after 1 s, doubling), the ladder reset by a successful join; BadAuth → the whole ladder (six tries, 3 s to 60 s),
then one every 5 minutes (Android disables a network for 5 minutes after three
authentication failures, but the chip reports a handshake that timed out under load with
the same verdict as a wrong password — three times in a row on run 3b's cycle 74 — so the
verdict alone cannot be trusted that early); an explicit leave or Forget clears the wanted network. `link.rs` keeps the wanted
credentials (boot's configured network, or the app's last join) and executes the retries
(`wifi: rejoin "<ssid>" (<why>; attempt N, join state 0x…)`). `NetworkInterface_CYW43.c`
also calls `FreeRTOS_NetworkDown` when a link that was up goes down, so the stack runs its
network-down path (`net: down`, sockets failing fast, `ConnectivityManager.onLost`) and
brings DHCP back the moment the station is re-associated — a lost link used to leave the
stack believing it was up.

*Bench.* `pico_enviro_mon_w` slot, the AP "ATT69vkRev", instrumented debug firmware,
power cycle → RTT attach → wait for `net: up` with a 120 s limit (the RTT ring keeps the
whole boot log, so no `probe-rs run` reset is needed). First run, 28 cycles: one boot hit
the sequence above and the first retry joined it (`net: up` 6 s after the verdict). Second
run, 125 cycles: 4 boots (3.2 %, the rate NET-12 was opened with) hit the same handshake
timeout, every one of them rejoined — `net: up` 6, 7 and 9 s after the verdict on three,
and 55 s on the fourth, which is the second finding. No boot stayed down.

*The second half: a self-join the driver never counts.* On that fourth boot (cycle 116) the
retry got `SET_SSID` status 3 — NONET, the AP did not answer the probe — three times over
35 s, the AP being out of reach. The chip's firmware keeps trying the SSID it was given,
and at 58 s it joined by itself: `ASSOC_REQ_IE, AUTH 0, ASSOC_RESP_IE, LINK (up), JOIN,
PSK_SUP 6 (KEYED)`. The driver's join-state word then read **0xe03**: authenticated,
linked and keyed, with the verdict nibble still NONET, because `EV_AUTH` status 0 resets
the verdict only after BADAUTH. `WIFI_JOIN_STATE_ALL` is 0x0e01, so the collapse to
link-up never happened, `cyw43_wifi_link_status` kept answering NONET, and the station was
on the network with nothing above it knowing. That is the "11 minutes until a probe reset"
boot of the original report: a self-join after a verdict, invisible for ever. The
supervisor's next retry (24 s later) re-issued the join and `net: up` followed at 62 s;
`NetworkInterface_CYW43.c` now also performs the collapse the driver skipped
(`vCollapseSelfJoin`, in `picodroid_cyw43_sta_status` on the link task: all three progress
bits set → the word back to bare ACTIVE and the interface up, the driver's own step), so a
self-join counts the moment it completes. Third run, the firmware with that in, 77 cycles
past midnight with the access point visibly busier: 9 boots needed a rejoin (one to three
retries each; `net: up` 6 to 31 s after attach), none stayed down. Cycle 42 showed a retry
landing on a self-join the chip had just begun (`join state 0x203`, authenticated), the
two joins racing to the same end; a due retry now waits up to 5 s while the word shows
progress bits over a failure verdict, since the collapse above cancels it once the station
keys. Cycle 74 took three consecutive `bad password` verdicts, one short of the ladder's
old give-up; the ladder now runs to its cap (six tries) before the 5-minute holdoff. Fourth
run, with those two in, 10 cycles before it was cut short for the next finding: 3 boots
needed a rejoin (cycle 10 took an auth timeout, a NONET and another auth timeout before the
fourth attempt joined at 33 s), none stayed down. Its cycle 8 showed the verdict-wipes-progress shape of
the self-join: `AUTH 0`, then `PSK_SUP` status 8 reason 0 filed as BADAUTH (an assignment,
which erased the AUTH bit), then `PSK_SUP 6` — the handshake completed — leaving the word at
**0x804**, keyed over a bad-password verdict, which the all-three-bits collapse did not
match; the retry re-issued the join and `net: up` took 19 s instead of 3. KEYED is only ever
set by the supplicant reporting the handshake complete (association and link included), and
cannot be the open-network preset once a verdict has overwritten the word, so the collapse
now fires on KEYED over any failure verdict. Fifth run, the final firmware, 150 cycles
(00:34–01:26): 149 joined, 7 of them after a rejoin (one retry on five, two on one, four on
cycle 45 where the AP refused for 55 s; `net: up` 6–52 s after attach), one by that
collapse alone (cycle 147: the verdict, KEYED 56 ms later, `associated` with no retry), and
one cycle that is a harness artifact (its log holds no boot line, only the display doze
60 s after the previous boot: the port cycle did not reset the board). No boot stayed down.
Across the five runs, 437 power cycles with the supervisor in: 0 boots stuck, 30 boots that
would have been (6.9 %; the first run's 3.2 % matched the report, the later ones ran past
midnight on a busier access point).

*An episode that was not the firmware.* The first attempt at the third run (23:35–23:52) had
11 clean joins and then six boots in a row that got NONET on every join and retry for the
whole 120 s window, from a fresh power cycle each time, while the bench host on the same
access point reached the gateway and saw the SSID on 2.4 GHz (channel 6), and
`pico_touch_kit` flashed with the same credentials over its own probe joined in 4 s. Fifteen
minutes later the same board joined again with firmware 3 and with firmware 4, within
seconds of each other, so neither the port change nor the supervisor was involved. The
board was invisible to the access point, or the access point to it, for a quarter of an
hour; a client-side lockout after the wrong-password boot's twenty failed handshakes is the
candidate, not shown. What the firmware did through it is the point: `wifi: rejoin … (no
such network; attempt 5)` every 60 s, and a join as soon as the network answered.

*A genuine wrong password* (`PICODROID_WIFI_PASS=definitely-wrong-password`, same trace)
looks different from the transient: the association completes (`AUTH 0, ASSOC, LINK up,
JOIN, SET_SSID 0`), then 4 s later `PSK_SUP` status 8 reason 14 (waiting for M1, reason
DEAUTH) and `DEAUTH_IND` reason 15 (the AP's 4-way-handshake timeout: our M2 carried a
wrong MIC) — `bad password`, every time. The chip re-associates by itself and fails the
same way every 5 s; the supervisor's retries ran at 3, 6 and 12 s (`join state 0x601`:
authenticated and linked, never keyed, so the self-join collapse above does not fire)
and then stepped back to one every 5 minutes, the status staying *Wrong password*. The
transient of the first finding never shows a `DEAUTH_IND`: its `PSK_SUP` status 4 reason
0 follows the driver's own key-timeout rejoin, which is the line a fork fix would draw.

*Not done.* The two classification gaps stay in the vendored driver (`EV_PSK_SUP`'s
catch-all, `EV_AUTH` resetting only BADAUTH); the port and the supervisor absorb both,
and a fork change would need the wrong-password signature above folded into its handler
first. A chip re-init as a last resort was not added: every retry seen recovered.
