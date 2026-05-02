// SPDX-License-Identifier: GPL-3.0-only
package displaydemo;

import picodroid.widget.TextView;

public class TapHandler implements Runnable {
  private TextView label;
  private int taps;

  public TapHandler(TextView label) {
    this.label = label;
    this.taps = 0;
  }

  public void run() {
    taps = taps + 1;
    label.setText("Taps: " + taps);
  }
}
