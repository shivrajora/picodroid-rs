// SPDX-License-Identifier: GPL-3.0-only
package displaydemo;

import picodroid.widget.CompoundButton;
import picodroid.widget.TextView;

public class SwitchHandler implements CompoundButton.OnCheckedChangeListener {
  private TextView label;

  public SwitchHandler(TextView label) {
    this.label = label;
  }

  @Override
  public void onCheckedChanged(CompoundButton view, boolean isChecked) {
    if (isChecked) {
      label.setText("Switch: ON");
    } else {
      label.setText("Switch: OFF");
    }
  }
}
