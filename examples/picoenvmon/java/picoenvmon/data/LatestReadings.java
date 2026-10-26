// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.data;

import javax.inject.Inject;
import javax.inject.Singleton;
import picodroid.hardware.Sensor;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.MutableLiveData;

/**
 * Latest 1 Hz smoothed value per sensor type: the app's readings repository. Written by {@code
 * SensorLoggerService}'s smoothing emit; the UI observes {@link #readings()}, and the HTTP
 * dashboard (network thread) reads the plain fields through {@link #get} without allocating. Those
 * fields are unsynchronized: float slots are written whole, and FreeRTOS scheduling on this
 * single-core target makes the cross-thread reads benign (a reader sees either the previous or the
 * current sample, never a torn one — floats occupy one 32-bit slot).
 */
@Singleton
public class LatestReadings {
  public static final int IDX_TEMPERATURE = 0;
  public static final int IDX_HUMIDITY = 1;
  public static final int IDX_PRESSURE = 2;
  public static final int IDX_GAS = 3;
  public static final int IDX_LIGHT = 4;
  public static final int COUNT = 5;

  private final float[] values = new float[COUNT];
  private int validMask;
  private final MutableLiveData<Readings> readings = new MutableLiveData<>();

  @Inject
  public LatestReadings() {}

  /** Map a {@link Sensor} type to an index, or -1. */
  public static int indexForType(int sensorType) {
    switch (sensorType) {
      case Sensor.TYPE_AMBIENT_TEMPERATURE:
        return IDX_TEMPERATURE;
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        return IDX_HUMIDITY;
      case Sensor.TYPE_PRESSURE:
        return IDX_PRESSURE;
      case Sensor.TYPE_GAS_RESISTANCE:
        return IDX_GAS;
      case Sensor.TYPE_LIGHT:
        return IDX_LIGHT;
      default:
        return -1;
    }
  }

  public void updateByType(int sensorType, float value) {
    int idx = indexForType(sensorType);
    if (idx >= 0) {
      values[idx] = value;
      validMask |= (1 << idx);
    }
  }

  /**
   * Publish the values {@link #updateByType} has written since the last call, as one immutable
   * {@link Readings}. Main thread (sensor callbacks run there).
   */
  public void publish() {
    float[] copy = new float[COUNT];
    System.arraycopy(values, 0, copy, 0, COUNT);
    readings.setValue(new Readings(copy, validMask));
  }

  /** The latest smoothed values, once a second while the logging service runs. */
  public LiveData<Readings> readings() {
    return readings;
  }

  /** Whether {@code idx} has received at least one sample. */
  public boolean isValid(int idx) {
    return (validMask & (1 << idx)) != 0;
  }

  public float get(int idx) {
    return values[idx];
  }
}
