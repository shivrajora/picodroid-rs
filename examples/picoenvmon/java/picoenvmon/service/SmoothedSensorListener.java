// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.service;

/**
 * Receives 1 Hz windowed-mean sensor values from {@link SensorLoggerService}. Lets a UI consumer
 * (e.g. {@code HomeActivity}) avoid registering its own 5 Hz {@code SensorEventListener} and
 * instead get one calm callback per sensor type per second.
 */
public interface SmoothedSensorListener {
  /** sensorType is one of {@code Sensor.TYPE_*} — same constants the raw API uses. */
  void onSmoothedSensor(int sensorType, float value);
}
