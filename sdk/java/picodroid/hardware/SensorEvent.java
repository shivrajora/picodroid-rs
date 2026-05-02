// SPDX-License-Identifier: GPL-3.0-only
package picodroid.hardware;

public final class SensorEvent {
  public Sensor sensor;
  public float[] values;
  public int accuracy;
  public long timestamp;
}
