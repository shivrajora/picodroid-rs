---
title: "WiFi & networking setup"
description: "Get a Pico 2 W on your WiFi network: the cyw43 submodule, Settings → Wi-Fi or build-time credentials, boot timing, and waiting for the network in your app."
---

Networking runs on the boards built around a **Raspberry Pi Pico 2 W** or a Pimoroni Pico Plus 2 W — `testbench_rp2350w`, `pico_enviro_mon_w`, `pico_display2_w` and `pico_touch_kit` — over the module's CYW43439 WiFi chip. Once the board has joined your network, the full [`picodroid.net` API](/api/networking/) — TCP/UDP sockets, `HttpURLConnection` and [HTTPS](#https) — works against real hosts.

In the **simulator** the host's network stack stands in for the link, with nothing to configure — but simulate one of those boards (`./scripts/sim.sh --board testbench_rp2350w --app netdemo`): the default `testbench_rp2350` has no network, in the simulator as on the bench. Settings → Wi-Fi still works there, against a canned list of access points (see [the simulator's WiFi](#the-simulators-wifi)).

## One-time setup: the cyw43 driver fork

The WiFi firmware driver is a git submodule, and it must be the **picodroid fork** of `cyw43-driver` (the upstream driver misses fixes the RP2350 port needs). Fresh clones with `--recurse-submodules` get the right one automatically. Checkouts that predate the fork switch must re-sync:

```bash
git submodule sync
git submodule update --init third_party/cyw43-driver
```

If the submodule is the unpatched upstream, the device build stops early with exactly this instruction — it does not build a broken image.

## Joining a network from Settings

On a multi-app board (every WiFi board is one) the Settings app has a **Wi-Fi** screen, as Android does: it shows the connection status and the saved network, **Scan for networks** lists what is in range strongest first — signal bars, and `*` for a network that needs a password — and a tap on a network joins it. A secured network opens a password screen: one masked field and a Connect button. The outcome comes back on the Wi-Fi screen: *Connecting*, *Obtaining IP address*, *Connected*, *Wrong password* or *Not found*.

The device remembers the network: it is written to the storage volume (`/system/wifi`, outside any app's directory) and joined at every boot before any app runs, so provisioning is a one-time step per device. The saved network's row opens a dialog to connect again or to **Forget** it.

How the password is typed depends on the board's input:

- **Touch panel** (`testbench_rp2350w`, `pico_touch_kit`): tap the field and the on-screen keyboard slides up; type, then tap its OK key or the Connect button.
- **Four buttons** (`pico_enviro_mon_w`, `pico_display2_w`): NEXT moves the focus to the field, ENTER opens the keyboard, PREV and NEXT walk its keys (hold to repeat), ENTER types the highlighted key, ESC closes the keyboard; then NEXT to the Connect button and ENTER. The keyboard's OK key connects too.

Apps get the same through [`WifiManager`](/api/networking/#wifi) (`Context.WIFI_SERVICE`).

## Build-time credentials override the saved network

A firmware can still carry its network, which is what the bench and the `net` test rows do: the SSID and password are compiled in from environment variables read at build time, and such a firmware joins that network at boot **whatever is saved on the device**. The Wi-Fi screen shows it with "Build" as the source and refuses to forget it; saving another network from Settings still writes the volume, so an image built without credentials picks it up.

```bash
PICODROID_WIFI_SSID='MyAP' PICODROID_WIFI_PASS='secret' \
  ./scripts/flash.sh --board testbench_rp2350w --app netdemo --release
```

- No `PICODROID_WIFI_SSID` at build time and nothing saved → the firmware logs `wifi: no network configured (Settings > Wi-Fi, or PICODROID_WIFI_SSID) — not joining` and the network stack stays offline until a network is saved.
- An empty `PICODROID_WIFI_PASS` means an open network. `PICODROID_WIFI_AUTH` (`open`, `wpa2`, `wpa3`, `wpa2wpa3`) pins the security; unset, a password means WPA2.
- **Never commit or distribute an image built with real credentials** — they are recoverable from the binary.

To avoid retyping (and accidentally shell-history-ing) credentials, keep them in `.wifi-creds.env` at the repo root — it is gitignored:

```bash
# .wifi-creds.env
PICODROID_WIFI_SSID='MyAP'
PICODROID_WIFI_PASS='secret'
```

```bash
env $(grep -v '^#' .wifi-creds.env | xargs) \
  ./scripts/flash.sh --board testbench_rp2350w --app netdemo --release
```

## What to expect at boot

Joining is not instant. On a typical WPA2 network the join completes in about 6 seconds and the DHCP lease lands a few seconds after that — plan for **up to ~10 seconds** between reset and a usable network.

Watch the RTT log for the state lines:

```text
net: up, ip 192.168.1.42     ← joined + DHCP lease acquired
net: down                    ← link lost, or the join has not succeeded yet
```

Each line is printed once per change of state: a join that keeps failing logs one `net: down`, not one per retry. The join itself logs `wifi: join "MyAP" requested (stored)` — or `(build)` — then `wifi: associated`, or `wifi: join failed: bad password` / `no such network`. Between them the chip's own events appear as `cyw43: [<ms>] ASYNC(…,SET_SSID,…)`, `AUTH`, `LINK`, `PSK_SUP`: the sequence a join goes through, and where it stopped when it did not complete.

The firmware keeps the network joined. A join that ends without the station associated — the access point missed it, the link dropped during the handshake, the chip never gave a verdict — is retried after 3 s, then 6, 12, 24, 48 and every 60 s, each one logged as `wifi: rejoin "MyAP" (no such network; attempt 2, join state 0x3)`; a link lost later (the access point rebooted, the board moved out of range) is rejoined the same way, with `net: down` at the loss and `net: up` once DHCP has a lease again. A wrong password rides the same ladder (six tries over about two and a half minutes, since an access point under load can fail the handshake in a way that reads the same) and is then tried once every 5 minutes: the status stays *Wrong password* until a new one is saved. The retries stop on `disconnect()` or **Forget**.

## The simulator's WiFi

The sim's link is the host's network, up from the start, and Settings → Wi-Fi is faked on top of it so the screens can be exercised: a scan finds the access points in `PICODROID_SIM_WIFI_NETWORKS` (`ssid:security:rssi` triples, comma-separated; default `picodroid-lab:wpa2:-45,Cafe Guest:open:-70,Neighbour:wpa2wpa3:-82`), a join succeeds for a listed SSID with the password in `PICODROID_SIM_WIFI_PASS` (default `picodroid`) and takes the simulated link up, and fails — *Wrong password*, *Not found* — otherwise, taking it down. The saved network persists in the sim's filesystem image and is joined at the next start, as on a device. `PICODROID_SIM_NET=down` starts with the link down, so a join is what brings it up. The control channel's `input text <string>` types into the open keyboard's field, so a script need not tap keys.

The link itself can be dropped and restored while an app runs, to rehearse what it does when WiFi goes away. Type the verb into the terminal the simulator runs in (its control channel reads stdin), or send it with `./scripts/sim-ctrl.sh` to a simulator started by `sim-remote.sh`:

```text
net down
net up
```

While the link is down `NetworkInfo.isConnected()` is false, new connects, sends and lookups fail, and registered `NetworkCallback`s hear `onLost`; `net up` brings `onAvailable` with a new `Network`. Sockets that are already connected keep flowing.

## Wait for the network in your app

An app's `onCreate` runs long before the join finishes, so a one-shot `NetworkInfo.isConnected()` check will almost always read `false` on hardware. Do what an Android app does: ask `ConnectivityManager` to tell you when the link is there.

```java
import picodroid.content.Context;
import picodroid.net.ConnectivityManager;
import picodroid.net.Network;

ConnectivityManager cm = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
cm.registerDefaultNetworkCallback(new ConnectivityManager.NetworkCallback() {
  @Override public void onAvailable(Network network) {
    // Up with an address: open your sockets here (or wake the thread that does).
  }
  @Override public void onLost(Network network) {
    // Dropped: show it and wait for the next onAvailable.
  }
});
```

Callbacks arrive on the main thread, delivered by the Activity's event loop; a callback registered while the link is already up hears `onAvailable` right after `onCreate` returns. Unregister in `onDestroy`. The [networking API reference](/api/networking/#network-status) has the whole surface (`Network`, `NetworkCapabilities`, `NetworkRequest`). The `connectivity` example is the pattern end to end, including what the app sees when the link drops and returns.

An app with no Activity has no event loop to deliver callbacks, so it polls `NetworkInfo.isConnected()` against a deadline instead:

```java
import picodroid.net.NetworkInfo;
import picodroid.os.SystemClock;

// Wait up to 30 s for WiFi join + DHCP before the first socket call.
int waited = 0;
while (!NetworkInfo.isConnected() && waited < 30000) {
  SystemClock.sleep(500);
  waited += 500;
}
if (!NetworkInfo.isConnected()) {
  Log.i(TAG, "Network not available.");
  return;
}
```

## HTTPS

An `https` URL works on every board with `has_tls = true` in its `board.toml`, which is each of the four WiFi boards: `URL.openConnection()` returns an `HttpsURLConnection`, and `connect()` runs a TLS 1.3 handshake that checks the server's name and verifies its chain against a root store compiled into the firmware.

The certificate's validity is checked against the wall clock, and a board has no battery-backed clock: the platform's [time service](/api/networking/#wall-clock-the-time-service-and-sntpclient) sets it from the network seconds after the link comes up, and a handshake that gets there first waits for it (up to 8 s) before refusing with `SSLHandshakeException`. Nothing for the app to do. In the simulator `PICODROID_SIM_WALL_CLOCK=1` anchors the clock to the host's at boot as well. See [HTTPS](/api/networking/#https) for what is checked, what it costs and what is not there.

## Try it: netdemo and http_get

Two Application-only example apps exercise the stack end-to-end, and both contain the poll above:

- **`netdemo`** — connects to a TCP echo server on port 7000, sends a message, logs the echo. Run an echo server on a machine the Pico can reach (`socat TCP-LISTEN:7000,fork EXEC:cat`).
- **`http_get`** — issues HTTP GET/POST requests against port 8000 (`python3 -m http.server 8000` on your dev machine works).

Both talk to loopback by default, which is right for the simulator. For a board, name the machine that runs the server at build time with `PICODROID_NET_TEST_HOST` — no source edit:

```bash
env $(grep -v '^#' .wifi-creds.env | xargs) PICODROID_NET_TEST_HOST=192.168.1.10 \
  ./scripts/flash.sh --board testbench_rp2350w --app http_get --release
```

`https_get` does the same over TLS and `connectivity` is the `NetworkCallback` pattern; the [examples index](/examples/#networking) lists them all.

## Limits

The current stack supports open, WPA2-AES and WPA3-SAE personal networks (no enterprise authentication) and IPv4 only; TLS is 1.3 with one cipher suite and no client certificates — the full list lives on the [known issues](/reference/known-issues/) page. The API surface itself is documented in the [networking API reference](/api/networking/).
