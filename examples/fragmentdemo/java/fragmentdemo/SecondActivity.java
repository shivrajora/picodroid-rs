// SPDX-License-Identifier: GPL-3.0-only
package fragmentdemo;

import picodroid.app.Activity;
import picodroid.os.Bundle;
import picodroid.widget.TextView;

/**
 * Covers the main Activity long enough for it to be reclaimed (the test.env turns "don't keep
 * activities" on), then finishes so the main Activity is re-created from its saved state.
 */
public class SecondActivity extends Activity {
  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    T.log("S.onCreate");
    TextView label = new TextView(this);
    label.setText("second");
    setContentView(label);
  }

  @Override
  public void onResume() {
    super.onResume();
    T.log("S.onResume");
    // Finishing here is enough: the main Activity is reclaimed at the end of this push, before
    // the pop this asks for runs (the reclaimdemo pattern).
    finish();
  }

  @Override
  public void onDestroy() {
    super.onDestroy();
    T.log("S.onDestroy");
  }
}
