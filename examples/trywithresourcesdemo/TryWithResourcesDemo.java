package trywithresourcesdemo;

import picodroid.app.Application;
import picodroid.pio.Adc;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;

public class TryWithResourcesDemo extends Application {
  public void onCreate() {
    PeripheralManager pm = PeripheralManager.getInstance();

    try (Adc adc = pm.openAdcPin("GP26")) {
      double v = adc.readValue();
      Log.i("TWR", "voltage=" + v);
    }

    Log.i("TWR", "done");
  }
}
