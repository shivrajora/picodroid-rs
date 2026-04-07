package displaydemo;

import picodroid.widget.TextView;
import picodroid.widget.ToggleButton;

public class ToggleHandler implements Runnable {
  private TextView label;
  private ToggleButton toggle;

  public ToggleHandler(TextView label, ToggleButton toggle) {
    this.label = label;
    this.toggle = toggle;
  }

  public void run() {
    if (toggle.isChecked()) {
      label.setText("LED: ON");
    } else {
      label.setText("LED: OFF");
    }
  }
}
