package sensordemo;

import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.util.Log;

public class SensorDemoActivity extends Activity implements SensorEventListener {
  private static final String TAG = "SensorDemo";
  private int eventCount = 0;

  public void onCreate() {
    SensorManager mgr = (SensorManager) getSystemService(Context.SENSOR_SERVICE);
    if (mgr == null) {
      Log.i(TAG, "SensorManager not available");
      return;
    }

    Sensor temp = mgr.getDefaultSensor(Sensor.TYPE_AMBIENT_TEMPERATURE);
    if (temp == null) {
      Log.i(TAG, "No temperature sensor on this board");
      return;
    }

    Log.i(TAG, "Registering for " + temp.getName());
    mgr.registerListener(this, temp, SensorManager.SENSOR_DELAY_NORMAL);
  }

  public void onSensorChanged(SensorEvent event) {
    eventCount++;
    Log.i(TAG, "temp=" + event.values[0] + "C (event #" + eventCount + ")");
  }

  public void onAccuracyChanged(Sensor sensor, int accuracy) {}
}
