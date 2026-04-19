package picodroid.app;

import picodroid.content.Context;
import picodroid.content.pm.PackageManager;
import picodroid.graphics.Display;
import picodroid.hardware.SensorManager;
import picodroid.view.View;

public class Activity {
  public void onCreate() {
    // Subclass overrides this
  }

  public Object getSystemService(String name) {
    if (Context.SENSOR_SERVICE.equals(name)) {
      return SensorManager.getInstance();
    }
    return null;
  }

  public PackageManager getPackageManager() {
    return PackageManager.getInstance();
  }

  public void setContentView(View root) {
    Display.getInstance().setContentView(root);
  }

  public Display getDisplay() {
    return Display.getInstance();
  }
}
