// SPDX-License-Identifier: GPL-3.0-only
package pagerdemo;

import picodroid.app.Activity;
import picodroid.os.Bundle;
import picodroid.widget.TextView;

/**
 * Covers the pager's Activity long enough for it to be reclaimed (test.env turns "don't keep
 * activities" on), then finishes so it is re-created from its saved state.
 */
public class CoverActivity extends Activity {
  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    TextView label = new TextView(this);
    label.setText("cover");
    setContentView(label);
  }

  @Override
  public void onResume() {
    super.onResume();
    // Finishing here is enough: the pager's Activity is reclaimed at the end of this push,
    // before the pop this asks for runs.
    finish();
  }
}
