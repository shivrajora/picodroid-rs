# WiFi provisioning: Settings → Wi-Fi, a stored network, build-time override

**Status: built 2026-09-26.** Amendments at the bottom record where execution diverged.

## 0. Why

Until now the SSID and password were compiled into the firmware
(`PICODROID_WIFI_SSID` / `PICODROID_WIFI_PASS`, read by `option_env!`). A device could
only ever join the network it was built for, and every user of a flashed image had to
rebuild to move it. Android provisions WiFi from Settings: scan, pick a network, type
the password, and the device remembers it across reboots. This doc adds that, with the
Android API shape (`android.net.wifi.WifiManager`), on every board that has WiFi.

## 1. What a user sees

- **Settings → Wi-Fi** exists only on boards with a WiFi link
  (`PackageManager.hasSystemFeature(FEATURE_WIFI)`; the root row is not built otherwise).
- The Wi-Fi screen shows the connection status, the saved network (with "Saved" or
  "Build" as its source), a "Scan" row and one row per network found, strongest first,
  with signal bars and the security type. Tapping an open network joins it; tapping a
  secured one opens the password screen: one `EditText` (masked) and a Connect button.
- The join outcome comes back on the Wi-Fi screen: Connecting…, Obtaining IP address,
  Connected, Wrong password, Network not found.
- **Password entry per board.** The system keyboard is the same LVGL keyboard on every
  board; what differs is how it is driven. On a touch panel the user taps keys. On a
  four-button board (Enviro+, Display Pack) ENTER on the field opens the keyboard and
  PREV/NEXT walk the keys one by one (held, they auto-repeat), ENTER types the selected
  key, ESC closes the keyboard; the OK key is Connect.
- The saved network is joined at every boot, before the JVM starts an app.
- **Build-time credentials still win.** A firmware built with `PICODROID_WIFI_SSID`
  joins that network at boot whatever is stored, and the Wi-Fi screen says so
  ("Build" as the source; Forget is refused). Saving from Settings still writes the
  store, so an image built without credentials picks it up.

## 2. Pieces

| Where | What |
|---|---|
| `crates/picodroid-core/src/hal/wifi.rs` | Family-neutral state: the credential store (`/system/wifi`, CRC-checked), the build-time override, the request mailbox (join / scan / leave) the Java side fills and the link driver drains, the scan table and the STA status, and the `WIFI_EVENTS` generation the event loop watches. |
| `platforms/rp/src/hal/rp/cyw43/link.rs` | `Cyw43Link::bring_up` joins the configured network (build first, else stored); `service` drains the mailbox on the link task, feeds the scan callback into the table and mirrors the driver's STA status. |
| `platforms/rp/src/hal/rp/port/net/NetworkInterface_CYW43.c` | Two C helpers: `picodroid_cyw43_sta_status` (failure kinds, down, associating, associated) and `picodroid_cyw43_scan_active`. |
| `crates/picodroid-core/src/hal/sim/wifi.rs` | The simulator's fake: canned scan results (`PICODROID_SIM_WIFI_NETWORKS`), a join that succeeds for a known SSID with the right password (`PICODROID_SIM_WIFI_PASS`, default `picodroid`) and takes the link up, or fails with BADAUTH / NONET. |
| `sdk/java/picodroid/net/wifi/` | `WifiManager`, `ScanResult`, `WifiInfo`, `WifiConfiguration`, `SupplicantState` — the Android names and semantics, one saved network (`networkId` 0). |
| `crates/picodroid-core/src/net/wifi_manager.rs` | The natives behind `WifiManager`; `net_stub.rs` answers "no WiFi" on boards without one. |
| `crates/picodroid-core/src/lifecycle/net_events.rs` | `dispatch_wifi_events`: when `WIFI_EVENTS` moved, `WifiManager.fireEvent()` on the main thread, which fans out `ScanResultsCallback`s. |
| `crates/picodroid-core/src/graphics/lvgl/widgets/keyboard.rs` + `events/keypad.rs` | The system keyboard sized to the display, joined to the Activity's focus group while shown, PREV/NEXT remapped to LEFT/RIGHT while it holds the focus; `EditText.setInputType(TYPE_TEXT_VARIATION_PASSWORD)` masks the field. |
| `system-apps/settings/java/settings/WifiActivity.java`, `WifiPasswordActivity.java` | The two screens. |
| `crates/picodroid-core/src/hal/sim/display.rs` | `input text <string>` on the control channel types into the keyboard's field, so a scripted test need not tap keys. |

## 3. Decisions

- **Store outside `/data`.** `/data/<package>` is the app sandbox and is swept when the
  package goes; the network belongs to the device, not to the Settings app, so it lives
  at `/system/wifi`, written only by the native side (`hal::fs`, tmp + rename). Format:
  `PDWF`, version, security, ssid length, password length, ssid, password, CRC-32.
- **One saved network.** Android keeps a list; an embedded board keeps the one it is
  on. `addNetwork` replaces it and returns 0; `getConfiguredNetworks` has at most one
  entry; `removeNetwork(0)` forgets it (and leaves the AP).
- **Every driver call on the link task.** The cyw43 driver state is unsynchronised
  across tasks, so the JVM never calls it: a request goes through the mailbox and a
  task notification, and the link task's `service` pass runs it. The scan callback also
  runs there (inside `cyw43_poll`).
- **Security from the scan.** `WifiConfiguration` carries no `allowedKeyManagement`
  (no `BitSet`); the join uses the security the last scan reported for that SSID, else
  WPA2/WPA3 with a password and open without one. `PICODROID_WIFI_AUTH` keeps its
  meaning for the build-time network.
- **No broadcast.** picodroid has no `BroadcastReceiver`, so the join outcome is read,
  not pushed: `WifiInfo.getSupplicantState()` and `WifiManager.getLastError()`
  (`ERROR_AUTHENTICATING` is Android's constant). The Settings screen polls them on a
  `ScheduledExecutorService` while a join is in flight; link up/down still arrives
  through `ConnectivityManager`. Scan completion is pushed
  (`registerScanResultsCallback`, Android 11's shape).
- **Keyboard on four buttons.** LVGL's keypad indev turns PREV/NEXT into focus moves
  before the widget sees them, and its button matrix moves its selection only on
  LEFT/RIGHT/UP/DOWN. While the system keyboard is the focused object, `keypad_read_cb`
  reports PREV as LEFT and NEXT as RIGHT (and does not forward them to Java, as an IME
  consumes keys); ENTER presses the selected key through LVGL's own path; ESC is BACK,
  which already hides the keyboard. No new widget, no per-board input code.

## 4. Verification

- `./scripts/sim.sh --app helloworld` (smoke) and `./scripts/pre-commit`.
- `scripts/sim-run.sh` settings lanes: the existing one on `testbench_rp2350` (no Wi-Fi
  row), and `settings-wifi` on `testbench_rp2350w` (touch: scan, pick, `input text`,
  connect) and on `pico_display2_w` (buttons: walk to the field, open the keyboard, walk
  and press keys, connect), both against the simulator's fake AP list.
- Device: flash `pico_display2_w` or `pico_touch_kit` without credentials, provision
  from Settings, power-cycle, see `wifi: join "<ssid>" (stored)` then `net: up`.

## 5. Amendments

### 2026-09-26 — verified on pico_display2_w

Release firmware, launcher boot, driven over `pdb input keyevent` (the ten-digit bench password
typed through the keyboard's `1#` layout). Scan found 5 networks; pick → password → Connect gave
`wifi: join … requested (app)`, `wifi: associated`, `net: up`, and the screen read Connected. A
power cycle rejoined from the store (`requested (stored)`). A wrong password saved from the UI
read Wrong password (`join failed: bad password`); a firmware built with `.wifi-creds.env` then
joined at boot anyway (`requested (build)`), and one built without fell back to the stored wrong
password and failed as expected. The board was left provisioned with the right password.
