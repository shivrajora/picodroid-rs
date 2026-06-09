// SPDX-License-Identifier: GPL-3.0-only
package sensordemo;

import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.graphics.Color;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.util.Log;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class SensorDemoActivity extends Activity implements SensorEventListener {
  private static final String TAG = "SensorDemo";
  private int eventCount = 0;
  private TextView tempLabel;

  @Override
  public void onCreate() {
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Sensor Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    tempLabel = new TextView();
    tempLabel.setText("Temp: --");
    tempLabel.setTextColor(Color.CYAN);
    root.addView(tempLabel);

    SensorManager mgr = (SensorManager) getSystemService(Context.SENSOR_SERVICE);
    if (mgr != null) {
      Sensor temp = mgr.getDefaultSensor(Sensor.TYPE_AMBIENT_TEMPERATURE);
      if (temp != null) {
        Log.i(TAG, "Registering for " + temp.getName());
        mgr.registerListener(this, temp, SensorManager.SENSOR_DELAY_NORMAL);
      } else {
        tempLabel.setText("No temp sensor");
      }
    } else {
      tempLabel.setText("No SensorManager");
    }

    setContentView(root);
  }

  @Override
  public void onSensorChanged(SensorEvent event) {
    eventCount++;
    float temp = event.values[0];
    tempLabel.setText("Temp: " + temp + " C");
    Log.i(TAG, "temp=" + temp + "C (event #" + eventCount + ")");
  }

  @Override
  public void onAccuracyChanged(Sensor sensor, int accuracy) {}
}
