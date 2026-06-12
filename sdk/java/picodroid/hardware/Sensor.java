// SPDX-License-Identifier: GPL-3.0-only
package picodroid.hardware;

public final class Sensor {
  public static final int TYPE_ALL = -1;

  /**
   * Motion/field sensor types, defined with Android's values for source compatibility. No current
   * picodroid board ships these sensors, so {@link SensorManager#getDefaultSensor(int)} returns
   * {@code null} for them — the same contract as an Android device without that sensor.
   */
  public static final int TYPE_ACCELEROMETER = 1;

  public static final int TYPE_MAGNETIC_FIELD = 2;
  public static final int TYPE_GYROSCOPE = 4;

  public static final int TYPE_LIGHT = 5;
  public static final int TYPE_PRESSURE = 6;
  public static final int TYPE_PROXIMITY = 8;
  public static final int TYPE_RELATIVE_HUMIDITY = 12;
  public static final int TYPE_AMBIENT_TEMPERATURE = 13;

  /** Picodroid extension (BME688 gas sensor); value sits outside Android's reserved range. */
  public static final int TYPE_GAS_RESISTANCE = 0x10001;

  int type;
  String name;
  String vendor;
  float maxRange;
  float resolution;
  int minDelay;

  Sensor() {}

  public int getType() {
    return type;
  }

  public String getName() {
    return name;
  }

  public String getVendor() {
    return vendor;
  }

  public float getMaximumRange() {
    return maxRange;
  }

  public float getResolution() {
    return resolution;
  }

  public int getMinDelay() {
    return minDelay;
  }
}
