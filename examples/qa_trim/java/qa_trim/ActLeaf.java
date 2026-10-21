// SPDX-License-Identifier: GPL-3.0-only
package qa_trim;

import picodroid.app.Activity;
import picodroid.os.Bundle;
import picodroid.widget.TextView;

/** Pushed over Fill once the cache is dropped, so the launch's reclaim pass has nothing to do. */
public class ActLeaf extends Activity {
  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    TextView label = new TextView(this);
    label.setText("qa_trim leaf");
    setContentView(label);
  }

  @Override
  public void onResume() {
    super.onResume();
    finish();
  }
}
