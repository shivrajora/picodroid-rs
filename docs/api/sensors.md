# Sensors

`picodroid.hardware.*` — Android-compatible `SensorManager` for environmental sensors declared in [`board.toml`](../porting-guide.md#boardtoml-reference). Today the only supported device is the Bosch **BME688** (temperature, humidity, pressure, gas resistance) over I2C. See [docs/README.md](../README.md) for the full API index.

Events are delivered on the main loop (no background task), so listeners run in the same thread as the Activity. Up to eight concurrent registrations are supported.

## Quick start

```java
import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.util.Log;

public class TempActivity extends Activity implements SensorEventListener {
    public void onCreate() {
        SensorManager mgr = (SensorManager) getSystemService(Context.SENSOR_SERVICE);
        Sensor temp = mgr.getDefaultSensor(Sensor.TYPE_AMBIENT_TEMPERATURE);
        mgr.registerListener(this, temp, SensorManager.SENSOR_DELAY_NORMAL);
    }

    public void onSensorChanged(SensorEvent event) {
        Log.i("TempDemo", "temp=" + event.values[0] + "C");
    }

    public void onAccuracyChanged(Sensor sensor, int accuracy) {}
}
```

See the full [`sensordemo`](../../examples/sensordemo/) example.

## `picodroid.hardware.SensorManager`

Retrieved via `Activity.getSystemService(Context.SENSOR_SERVICE)` (Android-style) or `SensorManager.getInstance()`.

| Method | Description |
|--------|-------------|
| `Sensor getDefaultSensor(int type)` | Returns the default sensor of a given type, or `null` if this board doesn't expose one. |
| `boolean registerListener(SensorEventListener l, Sensor s, int samplingPeriodUs)` | Registers `l` for events from `s`. Returns `false` if the 8-registration cap is hit. `samplingPeriodUs` takes one of the `SENSOR_DELAY_*` constants (below); raw microsecond values are not supported yet. |
| `void unregisterListener(SensorEventListener l)` | Removes every registration owned by `l`. Safe to call if `l` was never registered. |

### Sampling rate constants

The constants map to a tick count on the 16 ms main-loop period (so FASTEST ≈ 60 Hz, NORMAL ≈ 5 Hz):

| Constant | Value | Approximate rate |
|----------|-------|------------------|
| `SENSOR_DELAY_FASTEST` | 0 | every tick (~62 Hz) |
| `SENSOR_DELAY_GAME` | 1 | every 2 ticks (~31 Hz) |
| `SENSOR_DELAY_UI` | 2 | every 4 ticks (~15 Hz) |
| `SENSOR_DELAY_NORMAL` | 3 | every 12 ticks (~5 Hz) |

## `picodroid.hardware.Sensor`

Immutable metadata. Construct via `SensorManager.getDefaultSensor()`.

| Type constant | Value | Units (in `SensorEvent.values[0]`) |
|---------------|-------|------------------------------------|
| `TYPE_PRESSURE` | 6 | hPa |
| `TYPE_RELATIVE_HUMIDITY` | 12 | % RH |
| `TYPE_AMBIENT_TEMPERATURE` | 13 | °C |
| `TYPE_GAS_RESISTANCE` | 0x10001 | Ω (Picodroid extension — Android doesn't define this) |
| `TYPE_ALL` | -1 | sentinel, not a real sensor |

Getters: `getType()`, `getName()`, `getVendor()`, `getMaximumRange()`, `getResolution()`, `getMinDelay()`.

## `picodroid.hardware.SensorEvent`

Plain data class passed to `onSensorChanged`. All fields are public:

```java
public Sensor sensor;   // sensor that produced this event
public float[] values;  // values[0] is the primary reading; see table above
public int accuracy;    // 0 (unreliable) .. 3 (high)
public long timestamp;  // nanoseconds since boot (SystemClock.elapsedRealtimeNanos)
```

The framework reuses a single `SensorEvent` instance across callbacks, so copy any values you need to keep.

## `picodroid.hardware.SensorEventListener`

```java
public interface SensorEventListener {
    void onSensorChanged(SensorEvent event);
    void onAccuracyChanged(Sensor sensor, int accuracy);
}
```

## Hardware wiring

The sensor must be declared in `board.toml`:

```toml
[[sensor]]
kind = "bme688"     # only kind currently supported
bus  = "I2C0"       # "I2C0" or "I2C1"
addr = 0x77         # 7-bit I2C address
```

The BME688 driver ([src/drivers/bme688/](../../src/drivers/bme688/)) handles Bosch compensation. Read-only for now — calibration and heater-profile control are not exposed. See [porting-guide.md](../porting-guide.md#boardtoml-reference) for the full board.toml schema.

---

**See also:** [core.md](core.md) (Java language) · [system.md](system.md) (logging, clock, threads, executors) · [peripherals.md](peripherals.md) (direct I2C / SPI access) · [ui.md](ui.md) (display, widgets)
