# Java System API

Java system APIs live under `sdk/java/picodroid/` and mirror the Android API surface. Native implementations are in `src/system/picodroid/`.

## Quick Example

A complete mini-app that opens a GPIO pin, blinks it, and logs the result:

```java
package myapp;

import picodroid.util.Log;
import picodroid.os.SystemClock;
import picodroid.pio.PeripheralManager;
import picodroid.pio.Gpio;

public class MyApp {
    public static void main(String[] args) {
        PeripheralManager pm = PeripheralManager.getInstance();
        try (Gpio led = pm.openGpio("GP25")) {
            led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
            for (int i = 0; i < 5; i++) {
                led.setValue(true);
                SystemClock.sleep(500);
                led.setValue(false);
                SystemClock.sleep(500);
                Log.i("MyApp", "Blink " + String.valueOf(i + 1));
            }
        }
    }
}
```

---

## `java.lang.String`

The JVM provides built-in support for `java.lang.String`. All methods work on ASCII strings; multi-byte UTF-8 sequences are passed through unchanged but byte-indexed (not character-indexed).

```java
String s = "Hello, Pico!";

// Length and access
int len   = s.length();          // 12
char ch   = s.charAt(7);         // 'P'
boolean e = s.isEmpty();         // false

// Comparison
boolean eq  = s.equals("Hello, Pico!");          // true
boolean eqi = s.equalsIgnoreCase("hello, pico!"); // true
int     cmp = s.compareTo("Hello, Pico!");        // 0

// Predicates
boolean sw = s.startsWith("Hello");  // true
boolean ew = s.endsWith("Pico!");    // true
boolean co = s.contains("Pico");     // true

// Search
int idx  = s.indexOf(',');         // 6
int idx2 = s.indexOf("Pico");      // 7
int li   = s.lastIndexOf('!');     // 11

// Transforms — return new String values
String sub   = s.substring(7, 11);  // "Pico"
String tail  = s.substring(7);      // "Pico!"
String tr    = "  hi  ".trim();     // "hi"
String upper = "pico".toUpperCase(); // "PICO"
String lower = "PICO".toLowerCase(); // "pico"

// Static factory
String vi = String.valueOf(42);       // "42"
String vl = String.valueOf(9000L);    // "9000"
String vb = String.valueOf(true);     // "true"
```

> **StringBuilder interaction:** `+` string concatenation compiles to a compiler-generated `StringBuilder` that shares the JVM's single internal buffer. If you build a `StringBuilder` manually and then log `"prefix=" + sb.toString()`, the compiler's `StringBuilder` will clear the buffer before `sb.toString()` is evaluated. Capture the result first:
> ```java
> String result = sb.toString();   // snapshot the buffer
> Log.i(TAG, "prefix=" + result);  // safe to concatenate now
> ```

## `java.lang.StringBuilder`

```java
StringBuilder sb = new StringBuilder();         // empty
StringBuilder sb = new StringBuilder("prefix="); // with initial content

sb.append("text");    // append String
sb.append(42);        // append int
sb.append(3.14f);     // append float  (formats as "3.14")
sb.append(100L);      // append long
sb.append(true);      // append "true" or "false"
sb.append('x');       // append char

int  len = sb.length();    // current content length
char ch  = (char) sb.charAt(2);  // byte at position 2

String s = sb.toString();  // intern result as a String
```

> **Single shared buffer:** all `StringBuilder` instances in the JVM share one underlying buffer. Creating a new `StringBuilder` (including the compiler-generated one for `+` concatenation) clears that buffer. Build one `StringBuilder` at a time and call `toString()` before starting another.

## `picodroid.util.Log`

```java
import picodroid.util.Log;

Log.i("TAG", "message");   // info log → defmt::info! over RTT
```

## `picodroid.os.SystemClock`

```java
import picodroid.os.SystemClock;

SystemClock.sleep(500);               // sleep for 500 ms
long t = SystemClock.elapsedRealtimeNanos();  // nanoseconds since boot (monotonic)
```

## `picodroid.os.Runtime`

GC introspection. All methods are static.

```java
import picodroid.os.Runtime;

long nanos  = Runtime.gcTimeNanos();  // total time spent in GC so far (ns)
int  count  = Runtime.gcCount();      // number of GC cycles run
int  freed  = Runtime.gcFreed();      // total heap entries freed across all cycles
Runtime.resetGcStats();               // reset all three counters to zero
```

## `picodroid.pio.PeripheralManager`

Singleton for opening hardware peripherals.

```java
import picodroid.pio.PeripheralManager;

PeripheralManager pm = PeripheralManager.getInstance();
Gpio gpio       = pm.openGpio("GP25");
UartDevice uart = pm.openUartDevice("UART0");
I2cDevice  i2c  = pm.openI2cDevice("I2C0");
SpiDevice  spi  = pm.openSpiDevice("SPI0");
Pwm pwm         = pm.openPwm("GP25");
Adc adc         = pm.openAdcPin("GP26");
```

## Resource management (`AutoCloseable`)

All peripheral classes implement `java.lang.AutoCloseable`, so they can be used in try-with-resources blocks. `close()` releases the hardware resource and is guaranteed to be called even if the body throws.

```java
try (Gpio gpio = pm.openGpio("GP25")) {
    gpio.setDirection(Gpio.DIRECTION_OUT_INITIALLY_HIGH);
    // gpio.close() is called automatically here
}

// Multiple resources (closed in reverse order)
try (Adc adc = pm.openAdcPin("GP26");
     Gpio cs  = pm.openGpio("GP17")) {
    double v = adc.readValue();
    cs.setValue(false);
}
```

## `picodroid.pio.Gpio`

```java
import picodroid.pio.Gpio;

gpio.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
gpio.setValue(true);    // drive high
gpio.setValue(false);   // drive low
gpio.close();           // or use try-with-resources
```

## `picodroid.pio.UartDevice`

```java
import picodroid.pio.UartDevice;

uart.setBaudrate(115200);
uart.setDataSize(8);
uart.setParity(UartDevice.PARITY_NONE);
uart.setStopBits(1);
int b = uart.readByte();    // non-blocking; returns -1 if RX FIFO empty
uart.writeByte(0x41);       // blocking write of single byte
```

## `picodroid.pio.I2cDevice`

Default pins: I2C0 → SDA=GP4, SCL=GP5; I2C1 → SDA=GP2, SCL=GP3.

```java
import picodroid.pio.I2cDevice;

i2c.setSpeed(I2cDevice.SPEED_FAST);      // 400 kHz (default: 100 kHz)

// Write 2 bytes to device at address 0x3C
byte[] cmd = new byte[]{ (byte)0x00, (byte)0xAF };
int written = i2c.write(0x3C, cmd, 2);  // returns bytes written, or -1 on NACK

// Read 2 bytes from device at address 0x48
byte[] buf = new byte[2];
int read = i2c.read(0x48, buf, 2);      // returns bytes read, or -1 on NACK

// Zero-byte write: probe for device presence (returns 0 if ACK, -1 if NACK)
byte[] empty = new byte[0];
int ack = i2c.write(0x48, empty, 0);
```

### I2C bus scan example

Probe every 7-bit address to discover connected devices:

```java
PeripheralManager pm = PeripheralManager.getInstance();
try (I2cDevice i2c = pm.openI2cDevice("I2C0")) {
    byte[] empty = new byte[0];
    for (int addr = 0x08; addr < 0x78; addr++) {
        if (i2c.write(addr, empty, 0) == 0) {
            Log.i("I2C", "Found device at 0x" + String.valueOf(addr));
        }
    }
}

## `picodroid.pio.SpiDevice`

Default pins (CS not driven by peripheral — use `Gpio` if needed):
SPI0 → SCK=GP2, MOSI=GP3, MISO=GP0; SPI1 → SCK=GP10, MOSI=GP11, MISO=GP8.

```java
import picodroid.pio.SpiDevice;

spi.setFrequency(4_000_000);           // 4 MHz (default: 1 MHz)
spi.setMode(SpiDevice.MODE_0);         // CPOL=0, CPHA=0 (default)

// Full-duplex: write tx, read back rx
byte[] tx = new byte[]{ (byte)0x9F, 0x00, 0x00 };
byte[] rx = new byte[3];
spi.transfer(tx, rx, 3);

// Write-only (RX discarded)
byte[] cmd = new byte[]{ (byte)0x02, (byte)0x00, (byte)0x00, (byte)0x00, (byte)0xAB };
spi.write(cmd, 5);
```

## `picodroid.pio.Pwm`

```java
import picodroid.pio.Pwm;

Pwm pwm = pm.openPwm("GP25");

pwm.setPwmFrequencyHz(1000.0);          // 1 kHz
pwm.setPwmDutyCycle(50.0);              // 50% duty cycle (0.0–100.0)
pwm.setEnabled(true);                   // start PWM output

pwm.setEnabled(false);                  // stop PWM output
pwm.close();                            // or use try-with-resources
```

## `picodroid.pio.Adc`

```java
import picodroid.pio.Adc;

Adc adc = pm.openAdcPin("GP26");

double voltage = adc.readValue();       // single blocking read, returns volts
adc.close();                            // or use try-with-resources
```

Pins are GPIO numbers (e.g. GP26–GP29 on RP2040). `readValue()` performs a single ADC conversion and returns the voltage.

## `picodroid.concurrent.Thread`

```java
import picodroid.concurrent.Thread;

Thread t = new Thread(new MyRunnable());
t.start();   // spawns a FreeRTOS task that calls MyRunnable.run()

// Priority (optional, must be set before start())
t.setPriority(Thread.MAX_PRIORITY);  // 1 (MIN) .. 5 (NORM, default) .. 10 (MAX)
int p = t.getPriority();
```

### Complete Runnable example

```java
import picodroid.concurrent.Thread;
import picodroid.util.Log;
import picodroid.os.SystemClock;

public class MyApp {
    public static void main(String[] args) {
        Thread worker = new Thread(new Runnable() {
            public void run() {
                for (int i = 0; i < 3; i++) {
                    Log.i("Worker", "tick " + String.valueOf(i));
                    SystemClock.sleep(1000);
                }
            }
        });
        worker.setPriority(Thread.MAX_PRIORITY);
        worker.start();

        Log.i("Main", "Worker started, main continues");
    }
}
```

### Priority

| Java constant | Value | FreeRTOS priority |
|---|---|---|
| `Thread.MIN_PRIORITY` | 1 | 11 |
| `Thread.NORM_PRIORITY` | 5 | 15 (default) |
| `Thread.MAX_PRIORITY` | 10 | 20 |

Priorities follow the Android `Thread` API (1–10). Internally they map to FreeRTOS priorities 11–20 (the JVM tier), which sit below real-time native tasks (21–30) and above background native services (1–10). `setPriority` must be called before `start()`; changing priority on a running thread is not supported.

Each call to `t.start()` creates a dedicated FreeRTOS task with a 4096-word stack. When `MyRunnable.run()` returns, the task self-deletes and its stack is reclaimed automatically.

All JVM child threads are pinned to **core 0**, the same core as the `jvm` task. This keeps the single-core safety assumption of `SharedJvmState` intact — no JVM state is ever accessed from core 1.

On hot-swap, any thread blocked inside `SystemClock.sleep()` is woken immediately so it can see the stop signal and exit cleanly before the new app starts.

## `java.lang.Math`

Standard math functions. All methods are static. `Math.PI` and `Math.E` are compile-time constants inlined by `javac`.

```java
// Constants (inlined by the compiler — no runtime cost)
double pi = Math.PI;   // 3.141592653589793
double e  = Math.E;    // 2.718281828459045

// abs — int, long, float, double
int    ai = Math.abs(-7);      // 7
long   al = Math.abs(-9000L);  // 9000
float  af = Math.abs(-3.14f);  // 3.14
double ad = Math.abs(-1.0);    // 1.0

// min / max — int, long, float, double
int    lo = Math.min(4, 9);    // 4
double hi = Math.max(1.5, 2.5); // 2.5

// Rounding
double fl = Math.floor(2.9);    // 2.0
double ce = Math.ceil(2.1);     // 3.0
int    ri = Math.round(2.6f);   // 3   (float → int)
long   rl = Math.round(2.5);    // 3   (double → long)

// Powers / roots
double sq = Math.sqrt(2.0);          // ≈ 1.4142135
double pw = Math.pow(2.0, 10.0);     // 1024.0

// Trigonometry (arguments in radians)
double s  = Math.sin(Math.PI / 2.0); // ≈ 1.0
double c  = Math.cos(0.0);           // 1.0
double t  = Math.tan(0.0);           // 0.0
double a2 = Math.atan2(1.0, 1.0);   // ≈ PI/4

// Angle conversion
double rad = Math.toRadians(90.0);   // ≈ PI/2
double deg = Math.toDegrees(Math.PI); // 180.0

// Logarithms / exponential
double ln  = Math.log(Math.E);       // ≈ 1.0
double lg  = Math.log10(100.0);      // ≈ 2.0
double ex  = Math.exp(1.0);          // ≈ 2.71828
```

## `java.util.ArrayList`

Dynamic list backed by a per-instance heap buffer.

```java
import java.util.ArrayList;

// Raw type (stores any Object — String, custom objects, null)
ArrayList list = new ArrayList();
list.add("alpha");
list.add("beta");
list.add("gamma");

int sz     = list.size();           // 3
boolean mt = list.isEmpty();        // false

String item    = (String) list.get(1);    // "beta"
String old     = (String) list.set(0, "ALPHA");  // returns "alpha"
String removed = (String) list.remove(2);        // returns "gamma"

boolean found = list.contains("ALPHA");   // true
list.clear();

// Indexed insert
list.add(0, "first");   // insert at position 0

// Generic type with autoboxing (Integer, Boolean, Long, Float, Double)
ArrayList<Integer> nums = new ArrayList<Integer>();
nums.add(10);    // autoboxes int → Integer
nums.add(20);
int n = nums.get(0);          // auto-unboxes Integer → int  (10)
boolean has = nums.contains(20);  // true — value equality for wrappers
```

> **Autoboxing:** `ArrayList<Integer>` works as expected — `add(42)` and `contains(42)` both box via `Integer.valueOf`. For raw `ArrayList`, store and retrieve Object references (String, custom class instances); do not store bare primitives without explicit boxing (`Integer.valueOf(42)`, etc.).
