---
title: "Networking: TCP, UDP, and HTTP"
description: "TCP, UDP, and HTTP/1.1 client APIs over the on-board Wi-Fi or simulator loopback."
---

`picodroid.net.*` — TCP (`Socket`, `ServerSocket`), UDP (`DatagramSocket`, `DatagramPacket`), a minimal HTTP/1.1 client (`URL`, `HttpURLConnection`) with HTTPS over TLS 1.3 (`picodroid.net.ssl.HttpsURLConnection`), an SNTP client, and Android's `ConnectivityManager` and `WifiManager`, backed by FreeRTOS+TCP on hardware (the Pico 2 W boards, via the cyw43 WiFi chip) and the host network stack under the simulator. IPv4 only. See [Java API overview](/api/) for the full API index.

Networking is a board capability, not a Cargo feature — a board opts in by setting `has_network = true` and a `network_type` (one of the known link types — `"cyw43"` today) in its [`board.toml`](/reference/porting-guide/#boardtoml-reference). On boards without a network stack the `picodroid.net.*` classes are registered as stubs: `NetworkInfo.isConnected()` returns `false`, and anything that would touch the network throws `UnsupportedOperationException`. Probe with `NetworkInfo.isConnected()` (or `PackageManager.hasSystemFeature(FEATURE_WIFI)`) and degrade, rather than assuming a socket will open.

A flash-constrained board may go further and leave the unusable classes out of its firmware entirely — `testbench_rp2040` ships only `NetworkInfo`, since networking will never be supported there (see `framework_class_excludes` in [limits](/reference/limits/#runtime-limits)). On such a board the missing classes fail to resolve instead of throwing, so the probe-and-degrade path is the portable one.

`InetAddress` represents an address as a packed 32-bit int. Sockets accept the raw int (from `InetAddress.getRawAddress()`) rather than a string, to keep the native API allocation-free. `InetAddress.getByName("host")` resolves a hostname (or parses a dotted-quad literal without touching the network) and throws `java.net.UnknownHostException` on failure.

## Network status

Which kind of link the board has is a build-time fact. `NetworkInfo.getType()` returns
`ConnectivityManager.TYPE_WIFI` (1) or `ConnectivityManager.TYPE_ETHERNET` (9), and
`ConnectivityManager.TYPE_NONE` (-1) on a board without networking. `PackageManager`'s
`FEATURE_WIFI` and `FEATURE_ETHERNET` answer the same question through
`hasSystemFeature`; an app that only needs *a* network should accept either.

On hardware the WiFi join takes ~6 s and DHCP completes around 10 s after boot, so an app that opens a socket in `onCreate()` races the link. Android's answer is `ConnectivityManager` with a `NetworkCallback`, and it is picodroid's too:

```java
import picodroid.content.Context;
import picodroid.net.ConnectivityManager;
import picodroid.net.Network;
import picodroid.net.NetworkCapabilities;

ConnectivityManager cm = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
cm.registerDefaultNetworkCallback(new ConnectivityManager.NetworkCallback() {
    @Override public void onAvailable(Network network) {
        // The link is up with an address: fetch, connect, listen.
    }
    @Override public void onCapabilitiesChanged(Network network, NetworkCapabilities caps) {
        boolean wifi = caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI);
    }
    @Override public void onLost(Network network) {
        // The link dropped: show it, stop retrying until onAvailable.
    }
});
```

The shape is Android's: one `ConnectivityManager` per app (`getSystemService`), a `NetworkCallback` subclass with the methods you need overridden, `registerDefaultNetworkCallback` / `registerNetworkCallback(NetworkRequest, cb)` / `requestNetwork` to start hearing and `unregisterNetworkCallback` to stop (in `onDestroy`; registering the same callback twice, or unregistering one that is not registered, throws `IllegalArgumentException` as on Android). `onAvailable` arrives when the link comes up with an address and `onLost` when it drops; a callback registered while the link is already up hears `onAvailable` shortly after `register` returns, never from inside it, so an Activity that registers in `onCreate` has its views by then. Callbacks run on the main thread between frames, like every other framework callback, and it is the Activity event loop that delivers them: an app with no Activity does not receive them (the same rule as a posted `Runnable`). Register from any thread.

Since a board has one link there is one `Network` at a time — `getActiveNetwork()` returns it, or null while the link is down — and each time the link comes back it is a new `Network`, as on Android, so `onLost` names the one `onAvailable` did. `getNetworkCapabilities(network)` and the `NetworkCapabilities` passed to `onCapabilitiesChanged` carry the link's transport (`TRANSPORT_WIFI` or `TRANSPORT_ETHERNET`, matching `NetworkInfo.getType()`) and the capabilities a home network shows on Android: `NET_CAPABILITY_INTERNET`, `NET_CAPABILITY_VALIDATED`, `NET_CAPABILITY_NOT_METERED` and the `NOT_*` set. `Network.openConnection(URL)` is `url.openConnection()`, since the board has one network, and `getNetworkHandle()` is Android's handle for it. picodroid does not probe the internet — `VALIDATED` means the link is up with an address. A `NetworkRequest` built with `NetworkRequest.Builder` (`addTransportType`, `removeTransportType`, `addCapability`, `removeCapability`, `clearCapabilities`, `build`) is satisfied by a network that has every capability it asks for and, if it names transports, one of them; a request for `TRANSPORT_CELLULAR` on a WiFi board never fires.

Not there: `LinkProperties` and `onLinkPropertiesChanged` — read the address with `NetworkInfo.getIpAddress()`; a DHCP renewal that changes it keeps the same `Network` and fires nothing. `onLosing` and `onUnavailable` exist so overrides compile but are never called (nothing loses a network gracefully or times a request out), and the deprecated `getActiveNetworkInfo()` is absent: `NetworkInfo`'s methods are static. A link that drops and returns within one frame (16 ms) is not reported. At most eight callbacks may be registered at once.

The static probes stay for code that has no Activity, or just wants an answer now:

```java
import picodroid.net.NetworkInfo;
import picodroid.net.InetAddress;

if (NetworkInfo.isConnected()) {
    InetAddress me = new InetAddress(NetworkInfo.getIpAddress());
    Log.i("Net", "IP: " + me.getHostAddress());   // "192.168.1.42"
}
```

An Application-only app (no Activity) that needs the link before its first socket call still polls `NetworkInfo.isConnected()` against a deadline, as `netdemo` and `http_get` do; under the simulator the link is up at boot, so the poll returns at once.

## WiFi

`picodroid.net.wifi.*` mirrors `android.net.wifi`: `WifiManager` (`getSystemService(Context.WIFI_SERVICE)`), `ScanResult`, `WifiInfo`, `WifiConfiguration` and `SupplicantState`. It is what Settings → Wi-Fi is built on ([WiFi setup](/get-started/networking/#joining-a-network-from-settings)); an app can do the same.

```java
import picodroid.content.Context;
import picodroid.concurrent.Executors;
import picodroid.net.wifi.ScanResult;
import picodroid.net.wifi.WifiConfiguration;
import picodroid.net.wifi.WifiManager;

WifiManager wm = (WifiManager) getSystemService(Context.WIFI_SERVICE);
wm.registerScanResultsCallback(Executors.mainExecutor(), new WifiManager.ScanResultsCallback() {
  @Override public void onScanResultsAvailable() {
    for (ScanResult r : wm.getScanResults()) {          // strongest first, one per SSID
      Log.i(TAG, r.SSID + " " + r.level + " dBm " + r.capabilities);
    }
  }
});
wm.startScan();
// ...
WifiConfiguration c = new WifiConfiguration();
c.SSID = "\"MyAP\"";                                    // quoted, as on Android
c.preSharedKey = "\"secret\"";                           // null for an open network
int id = wm.addNetwork(c);                              // saves it; 0, the one networkId
wm.enableNetwork(id, true);                             // joins it now
```

What differs from Android, and why:

- **One saved network.** `addNetwork` replaces the saved network and returns 0; `getConfiguredNetworks()` has at most one entry; `removeNetwork(0)` forgets it and leaves. A network compiled into the firmware (`PICODROID_WIFI_SSID`) shows as `WifiConfiguration.Status.CURRENT` and cannot be removed.
- **No key-management set.** `WifiConfiguration` has no `allowedKeyManagement`: the join uses the security the last scan reported for that SSID (open, WPA, WPA2, WPA3 or mixed), else WPA2/WPA3 with a password and open without one.
- **The outcome is read, not broadcast.** `getConnectionInfo()` gives a `WifiInfo`: `getSSID()` (quoted, or `WifiManager.UNKNOWN_SSID`), `getSupplicantState()` (`DISCONNECTED`, `ASSOCIATING`, `COMPLETED`), `getRssi()` from the last scan. After a join that did not complete, `getLastError()` is `ERROR_AUTHENTICATING` (Android's value), `ERROR_NETWORK_NOT_FOUND` or `ERROR_GENERIC`. The address arriving is `ConnectivityManager`'s `onAvailable`. Scan completion is pushed, through `registerScanResultsCallback` (Android 11's shape); nothing else is.
- **Always on.** `isWifiEnabled()` says whether the board has a WiFi link, and `getWifiState()` is `WIFI_STATE_ENABLED` or `WIFI_STATE_DISABLED` accordingly; `setWifiEnabled` is accepted and ignored (it returns `false`).

The rest of the surface:

| Member | Description |
|---|---|
| `startScan()` | `false` when the board has no WiFi or a request is already waiting; the results arrive through the callbacks a few seconds later. |
| `registerScanResultsCallback(Executor, ScanResultsCallback)` / `unregisterScanResultsCallback` | At most four at once (`IllegalStateException` past that; `IllegalArgumentException` for one already registered). Delivered between frames, so an app with no Activity does not receive them. |
| `ScanResult` | Public fields, as on Android: `SSID` (unquoted), `BSSID`, `level` (dBm), `frequency` (MHz), `capabilities` (`[ESS]` for an open network, `[WPA2-PSK-CCMP][ESS]`, `[WPA3-SAE-CCMP][ESS]`, both for a mixed-mode access point), `timestamp` (always 0); `isSecured()`. |
| `addNetwork(WifiConfiguration)` / `updateNetwork(WifiConfiguration)` | Save the network; 0, or -1 when the SSID is empty, a field is too long or the board has no WiFi. Neither connects. |
| `enableNetwork(int netId, boolean attemptConnect)` | With `attemptConnect`, join the saved network now. `false` when nothing is saved, `netId` is not 0, or a request is already waiting. |
| `disconnect()` / `reconnect()` / `reassociate()` | Leave the current network (the saved one is kept and rejoined at the next boot); join the saved one again; the same as `reconnect()`. |
| `WifiInfo` | A snapshot: `getSSID()`, `getSupplicantState()`, `getRssi()` (`WifiInfo.UNKNOWN_RSSI`, -127, for a network no scan has seen), `getIpAddress()` (packed as `NetworkInfo.getIpAddress()`; 0 until the link is up), `getNetworkId()` (0 when the current network is the saved one, else -1), `getBSSID()` (always `02:00:00:00:00:00`). |
| `calculateSignalLevel(int rssi, int numLevels)` (static) / `calculateSignalLevel(int rssi)` / `compareSignalLevel(int rssiA, int rssiB)` (static) | Android's helpers: a level in `0..numLevels-1`, linear between -100 and -55 dBm; the one-argument form uses five levels. |

On a board without WiFi the class is a stub: no networks, nothing saved, every request refused — check `hasSystemFeature(FEATURE_WIFI)` first. `testbench_rp2040` leaves the package out of its firmware.

## TCP client

```java
import picodroid.net.Socket;

InetAddress server = InetAddress.getByAddress(192, 168, 1, 10);
Socket sock = new Socket();
sock.connect(server.getRawAddress(), 7000);
sock.setTimeout(5000);                            // 5 s recv timeout (0 = infinite)

byte[] msg = "Hello".getBytes();
sock.send(msg, 0, msg.length);

byte[] buf = new byte[64];
int n = sock.recv(buf, 0, buf.length);            // -1 = end of stream; errors throw
sock.close();
```

`connect`, `send`, and `recv` throw `IOException` subtypes on failure — see [Error handling](#error-handling).

## TCP server

```java
import picodroid.net.ServerSocket;

ServerSocket srv = new ServerSocket(8080);      // BindException if the port is taken
srv.setSoTimeout(5000);                           // optional: accept() throws SocketTimeoutException after 5 s
Socket client = srv.accept();                     // blocking
// ... use client.send / client.recv ...
client.close();
srv.close();
```

## UDP

```java
import picodroid.net.DatagramSocket;
import picodroid.net.DatagramPacket;

DatagramSocket s = new DatagramSocket();          // any free local port; or new DatagramSocket(9000)
byte[] data = "ping".getBytes();
DatagramPacket out = new DatagramPacket(data, data.length,
                                        InetAddress.getByAddress(192,168,1,10), 9000);
s.send(out);

byte[] inBuf = new byte[1500];
DatagramPacket in = new DatagramPacket(inBuf, inBuf.length);
s.setSoTimeout(2000);                             // receive() past this throws SocketTimeoutException
s.receive(in);                                    // fills data, length, address, port
Log.i("Net", "got " + in.getLength() + " bytes");
s.close();
```

A datagram to `255.255.255.255` (or the subnet's broadcast address) reaches every host on the LAN.
`setBroadcast(true)` is the default, as in Java, and picodroid's stacks never refuse a broadcast,
so the flag is recorded rather than enforced. That is how a device can find its peer without being
given an address: `examples/claudeusage` broadcasts one query and takes the source address of the
reply (`BridgeDiscovery`). Under the simulator `PICODROID_SIM_NET_BROADCAST=0` makes broadcast sends
fail, to rehearse an access point that isolates its clients.

## HTTP client

`URL` + `HttpURLConnection` — a small Android-style HTTP/1.1 client layered on the TCP socket API. DNS resolution happens at `connect()` time.

Constraints:

- HTTP/1.1 only. `https` URLs work on boards built with TLS (`has_tls = true`: every RP2350
  WiFi board); elsewhere they throw `UnsupportedOperationException` at `connect()`. See
  [HTTPS](#https) below.
- Methods: `GET`, `POST`, `PUT`.
- `Connection: close` is always sent — no keep-alive / connection pooling.
- Request bodies need a known length: call `setFixedLengthStreamingMode(n)` before `connect()` on any request that writes a body.
- At most 16 request headers per connection.

The `HTTP_*` status constants (`HTTP_OK`, `HTTP_NOT_FOUND`, `HTTP_INTERNAL_ERROR`, …) match `java.net.HttpURLConnection`. `setRequestMethod` throws `UnsupportedOperationException` for any other method.

### Timeouts

```java
HttpURLConnection c = new URL("http://example.com/api").openConnection();
c.setConnectTimeout(10000);   // connect() throws SocketTimeoutException past this
c.setReadTimeout(10000);      // so does each blocking read of the response
c.connect();
```

Both are in milliseconds and both default to 0, which waits forever, so set them. The read timeout bounds `getResponseCode()` and every `HttpInputStream.read`; it is applied to the socket at connect time, so set it before `connect()`. A negative value throws `IllegalArgumentException`. `getConnectTimeout()` and `getReadTimeout()` read them back.

### Request headers

Set headers before connecting; `setRequestProperty` replaces any previous value for that name, `addRequestProperty` adds another line.

```java
HttpURLConnection c = new URL("http://example.com/api").openConnection();
c.setRequestProperty("Accept", "application/json");
c.setRequestProperty("Authorization", "Bearer " + token);
c.connect();
```

`Host`, `Connection`, and `Content-Length` are managed by the connection — values set for them are ignored. Setting any header after `connect()` throws `IllegalStateException`, as does a seventeenth header, and a name or value containing CR or LF throws `IllegalArgumentException` (header injection). `getRequestProperty(name)` returns what was set (case-insensitive; the first value when a header was added more than once), or null.

### Response headers

```java
int status = c.getResponseCode();          // 200
String message = c.getResponseMessage();   // "OK"
String type = c.getHeaderField("Content-Type");   // case-insensitive, null if absent

// Index 0 is the status line and has a null key; real headers start at 1.
for (int i = 1; ; i++) {
    String key = c.getHeaderFieldKey(i);
    if (key == null) {
        break;
    }
    Log.i(TAG, key + "=" + c.getHeaderField(i));
}
```

When a header repeats, `getHeaderField(String)` returns the last value; the indexed accessors see every line. `getErrorStream()` returns the body stream for a status of 400 or above, and null otherwise. `getContentLength()` is the parsed `Content-Length`, or -1 when the server sent none or the connection is not open.

`getResponseCode()`, `getInputStream()`, `getOutputStream()` and the header accessors connect first when `connect()` has not been called, as on Android.

### GET

```java
import picodroid.net.HttpInputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.URL;

HttpURLConnection c = new URL("http://example.com/api/time").openConnection();
try {
    c.connect();
    if (c.getResponseCode() == 200) {
        HttpInputStream in = c.getInputStream();
        byte[] buf = new byte[256];
        int n;
        while ((n = in.read(buf)) > 0) {
            // ... consume buf[0..n] ...
        }
    }
} finally {
    c.disconnect();
}
```

`HttpURLConnection` implements `AutoCloseable`, so a `try`-with-resources block is equivalent:

```java
try (HttpURLConnection c = new URL("http://example.com/").openConnection()) {
    c.connect();
    // ...
}
```

### POST

```java
import picodroid.net.HttpOutputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.URL;

byte[] body = "hello".getBytes();
HttpURLConnection c = new URL("http://example.com/ingest").openConnection();
try {
    c.setRequestMethod("POST");
    c.setDoOutput(true);
    c.setFixedLengthStreamingMode(body.length);   // required
    c.connect();
    c.getOutputStream().write(body);

    int status = c.getResponseCode();
    // ...
} finally {
    c.disconnect();
}
```

`Host:` is set automatically from the URL (including port if non-standard). Add your own with [`setRequestProperty`](#request-headers).

### `URL`

```java
URL u = new URL("http://192.168.1.10:8080/status?id=42");
u.getProtocol();   // "http"
u.getHost();       // "192.168.1.10"
u.getPort();       // 8080 (80 if omitted, 443 for https)
u.getPath();       // "/status?id=42"  — query string is part of the path
```

See [`examples/http_get/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/http_get) for a full GET + POST worked example.

## HTTPS

`URL.openConnection()` returns a `picodroid.net.ssl.HttpsURLConnection` (the shape of
`javax.net.ssl.HttpsURLConnection`) for an `https` URL, and the usual cast works. Everything
above applies unchanged; the difference is what `connect()` does on the wire:

- TLS 1.3, one cipher suite (`TLS_AES_128_GCM_SHA256`, which `getCipherSuite()` reports),
  P-256 key exchange.
- The server name is sent (SNI) and checked against the certificate — an IP literal in the
  URL is checked against the certificate's IP entries instead, and sends no SNI.
- The chain is verified against the runtime's compiled-in root store (GTS, ISRG / Let's
  Encrypt, DigiCert, GlobalSign, Sectigo / USERTrust, Amazon; ECDSA and RSA), so any
  public host under those roots works: `api.anthropic.com`, `api.github.com`,
  `api.open-meteo.com`, …
- Validity is checked against the wall clock, which **must have been set**: the runtime
  refuses to handshake with the clock unset rather than skip the check. Anchor it once the
  network is up, with `SntpClient` below.

Failures throw `javax.net.ssl.SSLHandshakeException` (an `IOException`) whose message
names the reason: `wall clock not set`, `not issued by a known root`, `rejected
(signature, validity or host name)`; a handshake that runs into the read timeout throws
`SocketTimeoutException`. Not mirrored: `setSSLSocketFactory`, `setHostnameVerifier`,
`getServerCertificates` — an app cannot loosen the trust store.

```java
import javax.net.ssl.SSLHandshakeException;
import picodroid.net.ssl.HttpsURLConnection;

HttpsURLConnection c = (HttpsURLConnection) new URL("https://api.example.com/v1/x").openConnection();
c.setConnectTimeout(10000);
c.setReadTimeout(10000);
try {
    int status = c.getResponseCode();   // the handshake happens in connect()
    // ...
} catch (SSLHandshakeException e) {
    Log.w(TAG, "certificate rejected: " + e.getMessage());
} finally {
    c.disconnect();
}
```

Cost, on the RP2350: about 24 KB of arena for the life of a connection (a 16 KB record
buffer, a 4 KB write buffer, the session state), plus a 40 KB stack for the handshake's
own task, which exists only for the handshake. Any thread may call `connect()`; like every
`connect()`, it blocks the caller, so the main thread is the wrong place for it. Design and
measurements: `docs/designs/tls-2026-09.md`.

## Wall clock: `SntpClient`

`System.currentTimeMillis()` counts from boot until an app anchors it; there is no
battery-backed clock. `picodroid.net.SntpClient` is the shape of Android's
`android.net.SntpClient`: one request yields the server's time and the monotonic reference
it was read against, and the caller anchors the clock, as Android's network time service
does.

```java
import picodroid.net.SntpClient;
import picodroid.os.SystemClock;

SntpClient client = new SntpClient();
if (client.requestTime("pool.ntp.org", 3000)) {
    long now = client.getNtpTime() + SystemClock.elapsedRealtime() - client.getNtpTimeReference();
    SystemClock.setCurrentTimeMillis(now);
}
```

`requestTime(String host, int timeoutMs)` sends one request and returns `false` on any failure
(resolution, timeout, a malformed reply) without throwing; `getRoundTripTime()` is how long the
exchange took, in milliseconds. It blocks for up to the timeout, so call it off the main thread.
The anchor is lost at a reset. Do this once the network is up and before the first `https` request. Under the simulator,
`PICODROID_SIM_WALL_CLOCK=1` anchors the clock to the host's at boot instead, for tests that
should not depend on an NTP round trip.

## Error handling

Network failures throw the `java.net` exception types Android apps expect, with Android's message wording — catch them per-type or via their `IOException` superclass:

| Condition | Exception | Message |
|---|---|---|
| Connect actively refused (RST) | `java.net.ConnectException` | `Connection refused` |
| Connect timeout (incl. unreachable hosts) | `java.net.SocketTimeoutException` | `connect timed out` |
| Receive timeout (`setTimeout`) | `java.net.SocketTimeoutException` | `Read timed out` |
| Accept timeout (`setSoTimeout`) | `java.net.SocketTimeoutException` | `Accept timed out` |
| Bind conflict | `java.net.BindException` | `Address already in use` |
| Hostname resolution failure | `java.net.UnknownHostException` | `Unable to resolve host "…"` |
| Peer reset / operation on a closed socket | `java.net.SocketException` | `Connection reset` / `Socket is closed` |
| Malformed HTTP response | `java.net.ProtocolException` | `unexpected status line: …` |
| TLS handshake refused (see [HTTPS](#https)) | `javax.net.ssl.SSLHandshakeException` | `wall clock not set…`, `certificate chain of <host> is not issued by a known root`, `certificate of <host> rejected (signature, validity or host name)`, `malformed certificate chain from <host>`, `TLS handshake with <host> aborted: …` |
| TLS handshake ran into the read timeout | `java.net.SocketTimeoutException` | `TLS handshake timed out` |
| No memory or entropy for the TLS session | `javax.net.ssl.SSLException` | `out of memory for the TLS session…`, `no hardware entropy for the handshake` |
| Anything else | `java.io.IOException` | `<op> failed (err N)` |

`Socket.recv` and `HttpInputStream.read` return `-1` **only** at orderly end-of-stream — timeouts and transport errors always throw, so a stalled-but-alive server no longer reads as a clean EOF. The hierarchy matches real Java: `ConnectException` and `BindException` extend `SocketException`; `SocketTimeoutException` extends `InterruptedIOException`, *not* `SocketException`; `SSLHandshakeException` and `SSLPeerUnverifiedException` extend `SSLException`, an `IOException`.

```java
import java.io.IOException;
import java.net.ConnectException;
import java.net.SocketTimeoutException;

try {
    sock.connect(server.getRawAddress(), 7000);
} catch (ConnectException e) {
    Log.w("Net", "refused: " + e.getMessage());
} catch (SocketTimeoutException e) {
    Log.w("Net", "timed out: " + e.getMessage());
} catch (IOException e) {
    Log.w("Net", "I/O error: " + e.getMessage());
}
```

See [`examples/netexception/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/netexception) for runnable per-type assertions.

> **Hardware availability:** the networking stack is only built in for boards whose `board.toml` declares `has_network = true` with a supported `network_type`. Today that means the four Pico 2 W boards: `testbench_rp2350w`, `pico_display2_w`, `pico_enviro_mon_w` and `pico_touch_kit`. On other boards the `picodroid.net.*` classes are stubbed and using them throws at runtime. Under `sim.sh`, networking always works against the host stack.
>
> Network builds require the `third_party/cyw43-driver` submodule to be the patched picodroid fork — existing checkouts must run `git submodule sync && git submodule update --init third_party/cyw43-driver` after the fork switch, or the build fails early. Full setup: [WiFi & networking setup](/get-started/networking/). On the device, the WiFi task runs on core 1 over a PIO+DMA gSPI transport.

> **WiFi credentials:** on hardware, the firmware joins the network saved from Settings → Wi-Fi, or the one named by the `PICODROID_WIFI_SSID` and `PICODROID_WIFI_PASS` environment variables at **build time**, which override a saved network (automatic auth: open without a password, WPA2 with one; set `PICODROID_WIFI_AUTH` to `open`, `wpa2`, `wpa3`, or `wpa2wpa3` to pin a mode — `wpa2wpa3` is WPA3-SAE with WPA2-PSK fallback for mixed-mode APs). Build-time credentials are baked into the image, so rebuild after changing them and never commit images built with real credentials. With neither, the stack still starts but stays offline. Example: `PICODROID_WIFI_SSID='MyAP' PICODROID_WIFI_PASS='secret' ./scripts/flash.sh --board testbench_rp2350w --app netdemo --release`. Expect the `net: up, ip …` RTT log line once DHCP completes (typically 5–15 s after boot); example apps poll `NetworkInfo.isConnected()` for up to 30 s to bridge this window.

## Current limits

- Open, WPA2-AES, and WPA3-SAE personal networks — no enterprise auth.
- TLS 1.3 only, one cipher suite, no client certificates, no session resumption, and only on
  boards with `has_tls = true` (the RP2350 WiFi boards); elsewhere HTTPS URLs throw at `connect()`.
- Socket I/O is chunked at 256 bytes per native call; larger reads/writes loop internally.

See [Known issues & current limits](/reference/known-issues/) for the live list.

---

**See also:** [core.md](/api/core/) (Java language) · [system.md](/api/system/) (logging, clock, threads) · [peripherals.md](/api/peripherals/) (GPIO, UART, I2C, SPI, PWM, ADC) · [storage.md](/api/storage/) (files, preferences) · [sensors.md](/api/sensors/) (SensorManager) · [ui.md](/api/ui/) (display, widgets)
