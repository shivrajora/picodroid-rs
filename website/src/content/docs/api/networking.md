---
title: "Networking: TCP, UDP, and HTTP"
description: "TCP, UDP, and HTTP/1.1 client APIs over the on-board Wi-Fi or simulator loopback."
---

`picodroid.net.*` — TCP (`Socket`, `ServerSocket`), UDP (`DatagramSocket`, `DatagramPacket`), and a minimal HTTP/1.1 client (`URL`, `HttpURLConnection`), backed by FreeRTOS+TCP on hardware (Pico 2 W via the cyw43 WiFi chip) and the host network stack under the simulator. IPv4 only. See [Java API overview](/api/) for the full API index.

Networking is a board capability, not a Cargo feature — a board opts in by setting `has_network = true` and `network_type = "cyw43"` in its [`board.toml`](/reference/porting-guide/#boardtoml-reference). On boards without a network stack the `picodroid.net.*` classes are registered as stubs (`NetworkInfo.isConnected()` returns `false`) and attempting to open a socket throws.

`InetAddress` represents an address as a packed 32-bit int. Sockets accept the raw int (from `InetAddress.getRawAddress()`) rather than a string, to keep the native API allocation-free. `InetAddress.getByName("host")` resolves a hostname (or parses a dotted-quad literal without touching the network) and throws `java.net.UnknownHostException` on failure.

## Network status

On hardware the WiFi join takes ~6 s and DHCP completes around 10 s after boot, so an app that opens a socket in `onCreate()` races the link. Poll `NetworkInfo.isConnected()` against a deadline instead of checking it once:

```java
import picodroid.net.NetworkInfo;
import picodroid.net.InetAddress;
import picodroid.os.SystemClock;

// Wait up to 30 s for WiFi join + DHCP (instant under the simulator).
long deadline = SystemClock.elapsedRealtimeNanos() + 30_000_000_000L;
while (!NetworkInfo.isConnected() && SystemClock.elapsedRealtimeNanos() < deadline) {
    SystemClock.sleep(500);
}

if (NetworkInfo.isConnected()) {
    InetAddress me = new InetAddress(NetworkInfo.getIpAddress());
    Log.i("Net", "IP: " + me.getHostAddress());   // "192.168.1.42"
} else {
    Log.w("Net", "network not up after 30 s");
}
```

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

DatagramSocket s = new DatagramSocket(0);         // 0 = any free local port
byte[] data = "ping".getBytes();
DatagramPacket out = new DatagramPacket(data, data.length,
                                        InetAddress.getByAddress(192,168,1,10).getRawAddress(),
                                        9000);
s.send(out);

byte[] inBuf = new byte[1500];
DatagramPacket in = new DatagramPacket(inBuf, inBuf.length);
s.setTimeout(2000);
s.receive(in);                                    // fills data, length, address, port
Log.i("Net", "got " + in.getLength() + " bytes");
s.close();
```

## HTTP client

`URL` + `HttpURLConnection` — a small Android-style HTTP/1.1 client layered on the TCP socket API. DNS resolution happens at `connect()` time.

Constraints:

- HTTP/1.1 only. HTTPS URLs throw `UnsupportedOperationException` at `connect()` — TLS is not bundled.
- Methods: `GET`, `POST`, `PUT`.
- `Connection: close` is always sent — no keep-alive / connection pooling.
- Request bodies need a known length: call `setFixedLengthStreamingMode(n)` before `connect()` on any request that writes a body.

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

`Host:` is set automatically from the URL (including port if non-standard). To add your own headers, the current API only accepts the method, path, and content-length — no per-request header map yet.

### `URL`

```java
URL u = new URL("http://192.168.1.10:8080/status?id=42");
u.getProtocol();   // "http"
u.getHost();       // "192.168.1.10"
u.getPort();       // 8080 (80 if omitted, 443 for https)
u.getPath();       // "/status?id=42"  — query string is part of the path
```

See [`examples/http_get/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/http_get) for a full GET + POST worked example.

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
| Anything else | `java.io.IOException` | `<op> failed (err N)` |

`Socket.recv` and `HttpInputStream.read` return `-1` **only** at orderly end-of-stream — timeouts and transport errors always throw, so a stalled-but-alive server no longer reads as a clean EOF. The hierarchy matches real Java: `ConnectException` and `BindException` extend `SocketException`; `SocketTimeoutException` extends `InterruptedIOException`, *not* `SocketException`.

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

> **Hardware availability:** the networking stack is only built in for boards whose `board.toml` declares `has_network = true` with a supported `network_type`. Today that means `--board testbench_rp2350w` (Pico 2 W). On other boards the `picodroid.net.*` classes are stubbed and using them throws at runtime. Under `sim.sh`, networking always works against the host stack.
>
> Network builds require the `vendor/cyw43-driver` submodule to be the patched picodroid fork — existing checkouts must run `git submodule sync && git submodule update --init vendor/cyw43-driver` after the fork switch, or the build fails early. Full setup: [WiFi & networking setup](/get-started/networking/). On the device, the WiFi task runs on core 1 over a PIO+DMA gSPI transport.

> **WiFi credentials:** on hardware, the firmware joins the network named by the `PICODROID_WIFI_SSID` and `PICODROID_WIFI_PASS` environment variables at **build time** (automatic auth: open without a password, WPA2 with one; set `PICODROID_WIFI_AUTH` to `open`, `wpa2`, `wpa3`, or `wpa2wpa3` to pin a mode — `wpa2wpa3` is WPA3-SAE with WPA2-PSK fallback for mixed-mode APs). They are baked into the image, so rebuild after changing them and never commit images built with real credentials. Without an SSID the stack still starts but stays offline. Example: `PICODROID_WIFI_SSID='MyAP' PICODROID_WIFI_PASS='secret' ./scripts/flash.sh --board testbench_rp2350w --app netdemo --release`. Expect the `net: up, ip …` RTT log line once DHCP completes (typically 5–15 s after boot); example apps poll `NetworkInfo.isConnected()` for up to 30 s to bridge this window.

## Current limits

- Open, WPA2-AES, and WPA3-SAE personal networks — no enterprise auth.
- No TLS: HTTPS URLs throw at `connect()`.
- Socket I/O is chunked at 256 bytes per native call; larger reads/writes loop internally.

See [Known issues & current limits](/reference/known-issues/) for the live list.

---

**See also:** [core.md](/api/core/) (Java language) · [system.md](/api/system/) (logging, clock, threads) · [peripherals.md](/api/peripherals/) (GPIO, UART, I2C, SPI, PWM, ADC) · [storage.md](/api/storage/) (files, preferences) · [sensors.md](/api/sensors/) (SensorManager) · [ui.md](/api/ui/) (display, widgets)
