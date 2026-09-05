---
title: "WiFi & networking setup"
description: "Get a Pico 2 W on your WiFi network: the cyw43 submodule, build-time credentials, boot timing, and waiting for the network in your app."
---

Networking runs on the **Raspberry Pi Pico 2 W** (`testbench_rp2350w` board) over its CYW43439 WiFi chip. Once the board has joined your network, the full [`picodroid.net` API](/api/networking/) — TCP/UDP sockets and `HttpURLConnection` — works against real hosts.

In the **simulator** none of this page applies: the sim routes `picodroid.net` through the host's network stack, so there are no credentials to configure and the network is up immediately.

## One-time setup: the cyw43 driver fork

The WiFi firmware driver is a git submodule, and it must be the **picodroid fork** of `cyw43-driver` (the upstream driver misses fixes the RP2350 port needs). Fresh clones with `--recurse-submodules` get the right one automatically. Checkouts that predate the fork switch must re-sync:

```bash
git submodule sync
git submodule update --init third_party/cyw43-driver
```

If the submodule is the unpatched upstream, the device build stops early with exactly this instruction — it does not build a broken image.

## WiFi credentials are baked in at build time

There is no runtime provisioning: the SSID and password are compiled into the firmware from two environment variables read at build time.

```bash
PICODROID_WIFI_SSID='MyAP' PICODROID_WIFI_PASS='secret' \
  ./scripts/flash.sh --board testbench_rp2350w --app netdemo --release
```

- No `PICODROID_WIFI_SSID` at build time → the firmware logs `wifi: no SSID configured (PICODROID_WIFI_SSID) — not joining` and the network stack stays offline.
- An empty `PICODROID_WIFI_PASS` means an open network.
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

Each line is printed once per change of state: a join that keeps failing logs one `net: down`, not one per retry.

## Wait for the network in your app

An app's `onCreate` runs long before the join finishes, so a one-shot `NetworkInfo.isConnected()` check will almost always read `false` on hardware. Poll with a deadline instead:

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

## Try it: netdemo and http_get

Two example apps exercise the stack end-to-end, and both already contain the wait loop above:

- **`netdemo`** — connects to a TCP echo server on port 7000, sends a message, logs the echo. Run an echo server on a machine the Pico can reach (`socat TCP-LISTEN:7000,fork EXEC:cat`), and point the server address in `NetDemo.java` at that machine.
- **`http_get`** — issues HTTP GET/POST requests. It ships pointing at `http://127.0.0.1:8000/` for the simulator; **edit `BASE_URL`** to a host reachable from your LAN before flashing (`python3 -m http.server 8000` on your dev machine works).

```bash
env $(grep -v '^#' .wifi-creds.env | xargs) \
  ./scripts/flash.sh --board testbench_rp2350w --app http_get --release
```

## Limits

The current stack supports open and WPA2-AES networks, IPv4 only, and no TLS — the full list lives on the [known issues](/reference/known-issues/) page. The API surface itself is documented in the [networking API reference](/api/networking/).
