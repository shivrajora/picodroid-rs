package sensordemo;

import picodroid.app.Application;

public class SensorDemoApp extends Application {
  public void onCreate() {
    startActivity(new SensorDemoActivity());
  }
}
