// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.service;

import picodroid.app.IBinder;
import picodroid.app.Notification;
import picodroid.app.Service;
import picodroid.content.Intent;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.util.Log;
import picoenvmon.data.SensorRingBuffer;
import picoenvmon.data.ThresholdConfig;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.hardware.RgbLed;
import picoenvmon.util.Formatter;

/**
 * Foreground Service that streams every sensor reading into a per-type ring buffer, drives the
 * Pack's RGB LED from gas IAQ, and logs threshold breaches. Bind via {@code bindService} to read
 * the ring buffers from a UI activity.
 */
public class SensorLoggerService extends Service implements SensorEventListener {
  private static final String TAG = "SensorLogger";
  private static final int NOTIFICATION_ID = 1;
  public static final int RING_CAPACITY = 60;

  /** Stable indices into the ring-buffer table — match {@link #ringFor(int)}. */
  public static final int IDX_TEMPERATURE = 0;

  public static final int IDX_HUMIDITY = 1;
  public static final int IDX_PRESSURE = 2;
  public static final int IDX_GAS = 3;
  public static final int IDX_LIGHT = 4;

  public static class LocalBinder implements IBinder {
    public SensorLoggerService service;
  }

  private final LocalBinder binder = new LocalBinder();
  private final SensorRingBuffer[] rings = {
    new SensorRingBuffer(RING_CAPACITY),
    new SensorRingBuffer(RING_CAPACITY),
    new SensorRingBuffer(RING_CAPACITY),
    new SensorRingBuffer(RING_CAPACITY),
    new SensorRingBuffer(RING_CAPACITY),
  };

  private SensorManager sensorManager;
  private RgbLed rgbLed;
  private ThresholdConfig thresholds;
  private float lastGas = -1f;
  private boolean started;

  public void onCreate() {
    binder.service = this;
    EnvAppComponent app = (EnvAppComponent) EnvAppComponent.current();
    rgbLed = app.rgbLed();
    thresholds = app.thresholds();

    sensorManager = SensorManager.getInstance();
    registerAll(sensorManager);
    Log.i(TAG, "onCreate");
  }

  private void registerAll(SensorManager mgr) {
    int[] types = {
      Sensor.TYPE_AMBIENT_TEMPERATURE,
      Sensor.TYPE_RELATIVE_HUMIDITY,
      Sensor.TYPE_PRESSURE,
      Sensor.TYPE_GAS_RESISTANCE,
      Sensor.TYPE_LIGHT,
    };
    for (int t : types) {
      Sensor s = mgr.getDefaultSensor(t);
      if (s != null) {
        mgr.registerListener(this, s, SensorManager.SENSOR_DELAY_NORMAL);
      } else {
        Log.i(TAG, "no default sensor for type=" + t);
      }
    }
  }

  public int onStartCommand(Intent intent, int startId) {
    if (!started) {
      started = true;
      Notification n =
          new Notification.Builder()
              .setContentTitle("PicoEnvMon")
              .setContentText("Logging sensors")
              .build();
      startForeground(NOTIFICATION_ID, n);
      Log.i(TAG, "foreground started");
    }
    return START_STICKY;
  }

  public IBinder onBind(Intent intent) {
    return binder;
  }

  public void onDestroy() {
    Log.i(TAG, "onDestroy");
    if (sensorManager != null) {
      sensorManager.unregisterListener(this);
    }
    if (rgbLed != null) {
      rgbLed.off();
    }
    stopForeground(true);
  }

  @Override
  public void onSensorChanged(SensorEvent event) {
    int type = event.sensor.getType();
    float v = event.values[0];
    SensorRingBuffer ring = ringFor(type);
    if (ring != null) {
      ring.add(v);
    }

    switch (type) {
      case Sensor.TYPE_GAS_RESISTANCE:
        lastGas = v;
        applyLedFromIaq(v);
        break;
      case Sensor.TYPE_AMBIENT_TEMPERATURE:
        if (thresholds.tempBreached(v)) {
          Log.i(TAG, "ALERT: temperature breach: " + v + " C");
        }
        break;
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        if (thresholds.humidityBreached(v)) {
          Log.i(TAG, "ALERT: humidity below threshold: " + v + " m%");
        }
        break;
      case Sensor.TYPE_LIGHT:
        if (thresholds.luxBreached(v)) {
          Log.i(TAG, "ALERT: light below threshold: " + v + " lx");
        }
        break;
      default:
        break;
    }
  }

  @Override
  public void onAccuracyChanged(Sensor sensor, int accuracy) {}

  private void applyLedFromIaq(float gasOhm) {
    if (rgbLed == null) {
      return;
    }
    int iaq = Formatter.iaqFromGas(gasOhm);
    // 0 (clean) → green; 250 → yellow; 500 → red.
    int r;
    int g;
    if (iaq < 250) {
      r = (int) (iaq * 255f / 250f);
      g = 255;
    } else {
      r = 255;
      g = (int) ((500 - iaq) * 255f / 250f);
      if (g < 0) {
        g = 0;
      }
    }
    rgbLed.setColor(r, g, 0);
  }

  private SensorRingBuffer ringFor(int type) {
    switch (type) {
      case Sensor.TYPE_AMBIENT_TEMPERATURE:
        return rings[IDX_TEMPERATURE];
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        return rings[IDX_HUMIDITY];
      case Sensor.TYPE_PRESSURE:
        return rings[IDX_PRESSURE];
      case Sensor.TYPE_GAS_RESISTANCE:
        return rings[IDX_GAS];
      case Sensor.TYPE_LIGHT:
        return rings[IDX_LIGHT];
      default:
        return null;
    }
  }

  /** Snapshot of one ring buffer. {@code idx} = one of {@link #IDX_TEMPERATURE} … */
  public int snapshot(int idx, float[] out) {
    if (idx < 0 || idx >= rings.length) {
      return 0;
    }
    return rings[idx].snapshot(out);
  }

  public float lastGas() {
    return lastGas;
  }
}
