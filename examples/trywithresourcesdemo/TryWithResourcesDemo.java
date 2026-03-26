package trywithresourcesdemo;

import picodroid.pio.Adc;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;

public class TryWithResourcesDemo {
  public static void main() {
    PeripheralManager pm = PeripheralManager.getInstance();

    try (Adc adc = pm.openAdcPin("GP26")) {
      double v = adc.readValue();
      Log.i("TWR", "voltage=" + v);
    }

    Log.i("TWR", "done");
  }
}
