package sensordemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class SensorDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(SensorDemoActivity.class));
  }
}
