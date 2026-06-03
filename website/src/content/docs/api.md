---
title: "Java System API"
description: "Java system APIs grouped by area: peripherals, storage, networking, sensors, UI, and more."
---

Java system APIs live under `sdk/java/picodroid/` and mirror the Android API surface. Native implementations are in `platforms/rp/src/system/picodroid/`.

The reference is split by package family. Pick the area you need.

| Area | Packages | Covers |
|------|----------|--------|
| [Core language](/api/core/) | `java.lang`, `java.util` | `String` (incl. `String.format`), `StringBuilder`, `Math`, wrapper classes, exceptions, `Random`, `ArrayList`, `HashMap` / `HashSet`, `Iterator` / for-each, enums, `Arrays` / `Collections` / `List` / `Comparable`, `Class`, `AutoCloseable` |
| [System & concurrency](/api/system/) | `picodroid.util`, `picodroid.os`, `picodroid.concurrent` | `Log`, `SystemClock`, `System.currentTimeMillis`, `Runtime` (GC stats), `Thread`, `Executors` (main-thread FIFO + background pool) |
| [Services & DI](/api/services/) (Preview) | `picodroid.app`, `picodroid.content`, `picodroid.di` | `Service` / `IBinder` / `Notification`, `bindService` / `startService`, `ServiceConnection`, manual DI components (`ApplicationComponent`, `ActivitySingletonComponent`) |
| [Peripherals](/api/peripherals/) | `picodroid.pio` | `PeripheralManager`, `Gpio`, `UartDevice`, `I2cDevice`, `SpiDevice`, `Pwm`, `Adc`, `AutoCloseable` idiom |
| [Storage](/api/storage/) | `picodroid.io`, `picodroid.content` | `File` / `FileInputStream` / `FileOutputStream` (LittleFS), `Preferences` / `Editor` |
| [Networking](/api/networking/) | `picodroid.net` | `Socket`, `ServerSocket`, `DatagramSocket`, `DatagramPacket`, `InetAddress`, `NetworkInfo`, `HttpUrlConnection` + `Url` (Pico 2 W on hardware; sim always works) |
| [Sensors](/api/sensors/) | `picodroid.hardware` | `SensorManager`, `Sensor`, `SensorEvent`, `SensorEventListener` — BME688 (temperature / humidity / pressure / gas), LTR559 (light / proximity) |
| [Graphics & UI](/api/ui/) | `picodroid.app`, `picodroid.graphics`, `picodroid.view`, `picodroid.widget`, `picodroid.debug` | `Application` / `Activity` full lifecycle + back stack, `Display` / `DisplayDebug`, `Color`, `Theme`, `GradientDrawable`, `View` (incl. `animate()`, per-View touch, focus nav), `ViewGroup`, `MotionEvent`, `GestureDetector`, `ViewPropertyAnimator`, `KeyEvent` / `OnKeyListener`, `OnSwipeListener`, typed listener interfaces, the `Adapter` / `ArrayAdapter` pattern, 20+ widgets including `Toast`, `AlertDialog`, `Keyboard`, `DatePicker`, `TimePicker`, `Snackbar`, `SwipeRefreshLayout`, `ImageView` |

## Quick example

A complete mini-app that opens a GPIO pin, blinks it, and logs the result. See [Peripherals](/api/peripherals/) for the full PIO surface and [System & concurrency](/api/system/) for `Log` and `SystemClock`.

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
