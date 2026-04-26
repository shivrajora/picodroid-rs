# Java System API

Java system APIs live under `sdk/java/picodroid/` and mirror the Android API surface. Native implementations are in `src/system/picodroid/`.

The reference is split by package family. Pick the area you need, or browse [docs/README.md](README.md) for a task-oriented index.

| Area | Packages | Covers |
|------|----------|--------|
| [Core language](api/core.md) | `java.lang`, `java.util` | `String`, `StringBuilder`, `Math`, `ArrayList`, `HashMap` / `HashSet`, `Iterator` / for-each, enums, `Arrays` / `Collections` / `List` / `Comparable` |
| [System services](api/system.md) | `picodroid.util`, `picodroid.os`, `picodroid.concurrent` | `Log`, `SystemClock`, `System.currentTimeMillis`, `Runtime` (GC stats), `Thread`, `Executors` (main-thread FIFO + background pool) |
| [Peripherals](api/peripherals.md) | `picodroid.pio` | `PeripheralManager`, `Gpio`, `UartDevice`, `I2cDevice`, `SpiDevice`, `Pwm`, `Adc`, `AutoCloseable` idiom |
| [Storage](api/storage.md) | `picodroid.io`, `picodroid.content` | `File` / `FileInputStream` / `FileOutputStream` (LittleFS), `Preferences` / `Editor` |
| [Networking](api/networking.md) | `picodroid.net` | `Socket`, `ServerSocket`, `DatagramSocket`, `DatagramPacket`, `InetAddress`, `NetworkInfo`, `HttpUrlConnection` + `Url` (Pico 2 W on hardware; sim always works) |
| [Sensors](api/sensors.md) | `picodroid.hardware` | `SensorManager`, `Sensor`, `SensorEvent`, `SensorEventListener` — BME688 (temperature / humidity / pressure / gas) |
| [Graphics & UI](api/ui.md) | `picodroid.app`, `picodroid.graphics`, `picodroid.view`, `picodroid.widget` | `Application` / `Activity` full lifecycle + back stack, `Display`, `Color`, `Theme`, `GradientDrawable`, `View` (incl. `animate()` + per-View touch), `MotionEvent`, `GestureDetector`, `ViewPropertyAnimator`, `KeyEvent` / `OnKeyListener`, 17 widgets including `Toast`, `AlertDialog`, `Keyboard` |

## Quick example

A complete mini-app that opens a GPIO pin, blinks it, and logs the result. See [api/peripherals.md](api/peripherals.md) for the full PIO surface and [api/system.md](api/system.md) for `Log` and `SystemClock`.

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
