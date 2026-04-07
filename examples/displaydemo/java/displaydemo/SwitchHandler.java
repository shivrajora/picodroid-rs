package displaydemo;

import picodroid.widget.Switch;
import picodroid.widget.TextView;

public class SwitchHandler implements Runnable {
  private TextView label;
  private Switch sw;

  public SwitchHandler(TextView label, Switch sw) {
    this.label = label;
    this.sw = sw;
  }

  public void run() {
    if (sw.isChecked()) {
      label.setText("Switch: ON");
    } else {
      label.setText("Switch: OFF");
    }
  }
}
