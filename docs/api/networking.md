# Networking: TCP and UDP

`picodroid.net.*` — TCP (`Socket`, `ServerSocket`) and UDP (`DatagramSocket`, `DatagramPacket`) sockets backed by FreeRTOS+TCP on hardware (Pico 2 W via the cyw43 WiFi chip) and the host network stack under the simulator. IPv4 only. See [docs/README.md](../README.md) for the full API index.

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

> **Hardware availability:** the networking stack is only built in for boards with WiFi. Today that means `--board testbench_rp2350w` (Pico 2 W). On other boards the `picodroid.net.*` classes are not registered and using them throws at runtime. Under `sim.sh`, networking always works against the host stack.

---

**See also:** [core.md](core.md) (Java language) · [system.md](system.md) (logging, clock, threads) · [peripherals.md](peripherals.md) (GPIO, UART, I2C, SPI, PWM, ADC) · [storage.md](storage.md) (files, preferences) · [ui.md](ui.md) (display, widgets)
