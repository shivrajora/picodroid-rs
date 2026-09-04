# Handover: networking rows in the nightly HIL run (NET-7, device half)

The sim half of NET-7 landed 2026-08-15 (see "Already landed" below). This
doc is the plan for the remaining half: making the 4 AM on-device nightly
exercise the WiFi/network stack, so a regression there turns a nightly email
red instead of waiting for someone to flash a demo by hand. Written to be
executed cold; every seam named below was verified in the tree at handover
time.

## Status: landed 2026-09-04

Everything under "Work items" is in the tree:

- `net` category in `scripts/hil-tests.conf`, validated by
  `scripts/check-hil-conf.sh` (board column required, `has_network = true`).
- `scripts/lib.sh`: `host_lan_ip`, `start_net_listeners`,
  `stop_net_listeners` (kill by PID; shared by both runners).
- `scripts/hil-run.sh`: per-row board via the 5th column, creds read from
  `.wifi-creds.env` into the firmware build (never logged), LAN IP into the
  APK build, SKIP with a reason when creds are missing or the row's MCU is
  not the one on the probe, listeners started before the first `net` row
  and stopped on exit.
- `scripts/sim-run.sh`: runs `net` rows against 127.0.0.1 with the same
  listeners.

One correction to §6 below: the apps log `"  status="` with leading spaces,
so the device line is `HttpGet:   status=200` and the row uses
`HttpGet[]:] +status=200` (the pattern as written here never matched).


**The attached HIL board is physically a Pico 2 W.** Verified 2026-08-15:
`testbench_rp2350w` firmware was flashed onto the attached board and joined
WPA2 + ran TCP/HTTP end-to-end. The nightly currently drives the same board
as plain `testbench_rp2350` (no radio firmware). So no second board, probe,
or USB port is needed — hil-run just has to build some rows with the W board
feature.

## Already landed (don't redo)

- `netexception` runs nightly in the **sim** suite, both shrink modes, via
  the new `sim` category in `scripts/hil-tests.conf` (board-override column
  selects the W-board sim build; hil-run skips `sim` rows).
- Build-time target-IP injection: examples opting in via
  `picodroidNetTest { enabled = true }` get a generated `NetTestConfig.java`;
  the host comes from `-PpicodroidNetTestHost` (preferred, per-invocation)
  or `PICODROID_NET_TEST_HOST` (env fallback), default loopback.
  `scripts/build-apk.sh` forwards the env var as `-P` automatically. netdemo
  and http_get already consume it — this is how the 2026-08-15 HW
  validation was driven, zero source edits.
- WiFi creds flow: `PICODROID_WIFI_SSID`/`_PASS` (and optional
  `PICODROID_WIFI_AUTH`) are `option_env!` at firmware-build time;
  `platforms/rp/build.rs` has `rerun-if-env-changed` for all three; local
  values live in the gitignored `.wifi-creds.env` at the repo root.

## Work items

### 1. Per-row board override in hil-run.sh

`scripts/hil-run.sh` hardcodes `BOARD="testbench_rp2350"` (~line 28) and
resolves it once. Give `term`/`loop` rows an optional board column (the
5th column, same position `sim` rows already use for their board and `pdb`
rows use for the command — `check-hil-conf.sh` already parses five fields).
When present, call `resolve_board "$row_board"` before the row's build
(`resolve_board` in `scripts/lib.sh` sets `BOARD_FEATURE`/`TARGET`/… as
globals) and restore the default afterwards.

Firmware is rebuilt per row (`cargo build` at ~line 496); switching board
features forces a full rebuild, so **group the W rows contiguously at the
end of the conf** — that's one rebuild per shrink mode instead of one per
row-transition.

### 2. A `net` category, not bare `term`

Recommended: a new category `net` = behaves like `term` on hil-run, is run
by sim-run like `term` too (sim networking works against the host stack),
and — crucially — is **skipped with a logged reason when
`.wifi-creds.env` is absent**. That keeps the suite green on any checkout
without bench creds, and gives an escape hatch when the AP is down
(`skip`-edit one line). Add `net` to the category cases in `hil-run.sh`,
`sim-run.sh`, and `check-hil-conf.sh` (which should also validate the board
column the way it already does for `sim` rows).

The alternative — plain `term` rows with a board column — works but makes
the nightly hard-depend on the home AP with no graceful degradation.

### 3. Creds into the row's firmware build

In `run_test`'s firmware build (~line 493, the `cargo_env` array): when the
row's board resolves to a network board, source `.wifi-creds.env` and append
`PICODROID_WIFI_SSID=… PICODROID_WIFI_PASS=…` to `cargo_env`. Never echo the
values into the log. Nothing else is needed — the cron job runs from the
repo root, and the file is already gitignored.

### 4. Test-host auto-detection

At hil-run start, detect the machine's LAN IP once, e.g.
`ip -4 route get 1.1.1.1 | grep -oP 'src \K[\d.]+'`, and export it as
`PICODROID_NET_TEST_HOST` before the APK build step — `build-apk.sh`
forwards it per-invocation. Do not hardcode: this machine's address is DHCP-
assigned (192.168.1.215 at handover time, not guaranteed stable).

### 5. Listeners, owned by hil-run

Have hil-run start the listeners itself just before the first `net` row and
kill them after the last — self-contained, survives reboots, no systemd
unit to maintain:

```bash
socat TCP-LISTEN:7000,fork,reuseaddr EXEC:cat &   # echo server for netdemo
ECHO_PID=$!
python3 -m http.server 8000 --directory "$(mktemp -d)" &   # for http_get
HTTP_PID=$!
# ... rows ...
kill $ECHO_PID $HTTP_PID
```

Kill by PID, never by pattern — `pkill -f`/`pgrep -f` with a literal that
appears in the caller's own command line kills the caller (this bit twice
during the 2026-08-15 session; exit code 144 is the tell). For probe-rs
specifically, `./scripts/device-lock.sh release` does the kill by exact
process name. A persistent
systemd user service (+ `loginctl enable-linger`) is the fallback if
always-on listeners turn out to be wanted for manual testing too.

### 6. The rows

```text
netdemo|net|90|NetDemo[]:] Sent 5 bytes;NetDemo[]:] Received 5 bytes;NetDemo[]:] Done.|testbench_rp2350w
http_get|net|90|HttpGet[]:] status=200;HttpGet[]:] read [0-9]+ body bytes;HttpGet[]:] status=501|testbench_rp2350w
```

- 90 s timeout: join ~6 s + DHCP lease by ~6–10 s + demo seconds, with
  margin for AP mood (the apps themselves poll `NetworkInfo.isConnected()`
  for up to 30 s).
- The `Tag[]:]` pattern form already matches both log shapes (device RTT
  `Tag: msg`, sim `[Tag] msg`).
- `status=501` is python http.server's correct answer to POST — asserting
  it proves the POST path end-to-end.
- **Do not add `netexception` as a device row.** Its cases assume host-OS
  loopback semantics (127.0.0.1 refusal, self-connect); FreeRTOS+TCP has no
  loopback interface. Device-side failure taxonomy is covered by the manual
  recipe in `docs/networking-followups-2026-08.md` instead.

## Costs and follow-through

- Nightly duration: 2 rows × 2 shrink modes ≈ +6–12 min (flash ~40 s +
  run ≤90 s each, plus one board-feature rebuild per mode).
- The nightly becomes AP-dependent for these rows; the `net`-category skip
  is the mitigation. Watch the first week's emails — the new-vs-known
  diffing will flag any join flakiness as "new" failures until a baseline
  forms.
- Validate the whole thing once by hand before trusting cron:
  `./scripts/hil-run.sh --app netdemo --no-email`, then check
  `build/hil/logs/<run>/netdemo.*.log` for the three patterns.
- Cron context (for reference): `0 4 * * *` runs
  `./scripts/hil-run.sh >> build/hil/cron.log` from the repo working tree;
  sim-run at 3 AM. Since the device lock (`scripts/device-lock.sh`) the
  4 AM run queues behind whoever holds the board for up to `HIL_LOCK_WAIT`
  (1 h) and otherwise records `SKIP hil-run (device busy: …)` — a manual
  session overlapping 4 AM no longer collides on the probe, it just delays
  or skips the nightly. Concurrent papk builds in the *same* checkout still
  race (`build/apks/<app>.papk` is shared); worktrees have their own.
