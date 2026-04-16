package picodroid.hardware;

public final class SensorManager {
  public static final int SENSOR_DELAY_FASTEST = 0;
  public static final int SENSOR_DELAY_GAME = 1;
  public static final int SENSOR_DELAY_UI = 2;
  public static final int SENSOR_DELAY_NORMAL = 3;

  private static SensorManager instance;

  SensorManager() {}

  public static SensorManager getInstance() {
    if (instance == null) {
      instance = new SensorManager();
    }
    return instance;
  }

  public native Sensor getDefaultSensor(int type);

  public native boolean registerListener(
      SensorEventListener listener, Sensor sensor, int samplingPeriodUs);

  public native void unregisterListener(SensorEventListener listener);
}
