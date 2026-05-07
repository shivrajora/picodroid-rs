---
title: "Networking: TCP, UDP, and HTTP"
description: "TCP, UDP, and HTTP/1.1 client APIs over the on-board Wi-Fi or simulator loopback."
---

`picodroid.net.*` — TCP (`Socket`, `ServerSocket`), UDP (`DatagramSocket`, `DatagramPacket`), and a minimal HTTP/1.1 client (`Url`, `HttpUrlConnection`), backed by FreeRTOS+TCP on hardware (Pico 2 W via the cyw43 WiFi chip) and the host network stack under the simulator. IPv4 only. See [Java API overview](/) for the full API index.

Networking is a board capability, not a Cargo feature — a board opts in by setting `has_network = true` and `network_type = "cyw43"` in its [`board.toml`](/reference/porting-guide/#boardtoml-reference). On boards without a network stack the `picodroid.net.*` classes are registered as stubs (`NetworkInfo.isConnected()` returns `false`) and attempting to open a socket throws.

`InetAddress` represents an address as a packed 32-bit int. Sockets accept the raw int (from `InetAddress.getRawAddress()`) rather than a string, to keep the native API allocation-free.

## Network status

```java
import picodroid.net.NetworkInfo;
import picodroid.net.InetAddress;

if (NetworkInfo.isConnected()) {
    InetAddress me = new InetAddress(NetworkInfo.getIpAddress());
    Log.i("Net", "IP: " + me.getHostAddress());   // "192.168.1.42"
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
int n = sock.recv(buf, 0, buf.length);            // -1 on error
sock.close();
```

## TCP server

```java
import picodroid.net.ServerSocket;

ServerSocket srv = new ServerSocket(8080);
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

`Url` + `HttpUrlConnection` — a small Android-style HTTP/1.1 client layered on the TCP socket API. DNS resolution happens at `connect()` time.

Constraints:

- HTTP/1.1 only. HTTPS URLs throw `UnsupportedOperationException` at `connect()` — TLS is not bundled.
- Methods: `GET`, `POST`, `PUT`.
- `Connection: close` is always sent — no keep-alive / connection pooling.
- Request bodies need a known length: call `setFixedLengthStreamingMode(n)` before `connect()` on any request that writes a body.

### GET

```java
import picodroid.net.HttpInputStream;
import picodroid.net.HttpUrlConnection;
import picodroid.net.Url;

HttpUrlConnection c = new Url("http://example.com/api/time").openConnection();
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

`HttpUrlConnection` implements `AutoCloseable`, so a `try`-with-resources block is equivalent:

```java
try (HttpUrlConnection c = new Url("http://example.com/").openConnection()) {
    c.connect();
    // ...
}
```

### POST

```java
import picodroid.net.HttpOutputStream;
import picodroid.net.HttpUrlConnection;
import picodroid.net.Url;

byte[] body = "hello".getBytes();
HttpUrlConnection c = new Url("http://example.com/ingest").openConnection();
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

### `Url`

```java
Url u = new Url("http://192.168.1.10:8080/status?id=42");
u.getProtocol();   // "http"
u.getHost();       // "192.168.1.10"
u.getPort();       // 8080 (80 if omitted, 443 for https)
u.getPath();       // "/status?id=42"  — query string is part of the path
```

See [`examples/http_get/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/http_get) for a full GET + POST worked example.

> **Hardware availability:** the networking stack is only built in for boards whose `board.toml` declares `has_network = true` with a supported `network_type`. Today that means `--board testbench_rp2350w` (Pico 2 W). On other boards the `picodroid.net.*` classes are stubbed and using them throws at runtime. Under `sim.sh`, networking always works against the host stack.

---

**See also:** [core.md](/api/core/) (Java language) · [system.md](/api/system/) (logging, clock, threads) · [peripherals.md](/api/peripherals/) (GPIO, UART, I2C, SPI, PWM, ADC) · [storage.md](/api/storage/) (files, preferences) · [sensors.md](/api/sensors/) (SensorManager) · [ui.md](/api/ui/) (display, widgets)
