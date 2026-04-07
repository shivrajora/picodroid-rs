package displaydemo;

import picodroid.pio.Gpio;
import picodroid.pio.PeripheralManager;
import picodroid.widget.TextView;
import picodroid.widget.ToggleButton;

public class ToggleHandler implements Runnable {
  private TextView label;
  private ToggleButton toggle;
  private Gpio led;

  public ToggleHandler(TextView label, ToggleButton toggle) {
    this.label = label;
    this.toggle = toggle;
    PeripheralManager manager = PeripheralManager.getInstance();
    led = manager.openGpio("GP25");
    led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
  }

  public void run() {
    if (toggle.isChecked()) {
      label.setText("LED: ON");
      led.setValue(true);
    } else {
      label.setText("LED: OFF");
      led.setValue(false);
    }
  }
}
