package adcdemo;

import picodroid.app.Application;
import picodroid.pio.Adc;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;

/**
 * ADC read demo.
 *
 * <p>Opens the ADC on GP26 (ADC channel 0) and takes 5 readings, logging each voltage value. On
 * hardware, connect a potentiometer or voltage source (0–3.3 V) to GP26. In simulation, a fixed
 * mid-scale voltage (1.65 V) is returned.
 */
public class AdcDemo extends Application {
  public void onCreate() {
    PeripheralManager pm = PeripheralManager.getInstance();
    Adc adc = pm.openAdcPin("GP26");

    for (int i = 0; i < 5; i++) {
      double voltage = adc.readValue();
      Log.i("ADC", "GP26 = " + voltage + " V");
    }

    adc.close();
    Log.i("ADC", "done");
  }
}
