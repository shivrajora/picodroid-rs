// SPDX-License-Identifier: GPL-3.0-only
package trywithresourcesdemo;

import picodroid.app.Application;
import picodroid.pio.Adc;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;

public class TryWithResourcesDemo extends Application {
  public void onCreate() {
    run();
  }

  public static void run() {
    PeripheralManager pm = PeripheralManager.getInstance();

    try (Adc adc = pm.openAdcPin("GP26")) {
      double v = adc.readValue();
      Log.i("TWR", "voltage=" + v);
    }

    Log.i("TWR", "done");
  }
}
