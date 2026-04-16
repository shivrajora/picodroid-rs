package picodroid.hardware;

public interface SensorEventListener {
  void onSensorChanged(SensorEvent event);

  void onAccuracyChanged(Sensor sensor, int accuracy);
}
