// SPDX-License-Identifier: GPL-3.0-only
package displaydemo;

import picodroid.view.View;
import picodroid.widget.TextView;

public class TapHandler implements View.OnClickListener {
  private TextView label;
  private int taps;

  public TapHandler(TextView label) {
    this.label = label;
    this.taps = 0;
  }

  @Override
  public void onClick(View v) {
    taps = taps + 1;
    label.setText("Taps: " + taps);
  }
}
