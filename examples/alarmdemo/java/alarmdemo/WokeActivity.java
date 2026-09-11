// SPDX-License-Identifier: GPL-3.0-only
package alarmdemo;

import picodroid.app.Activity;
import picodroid.util.Log;
import picodroid.view.Gravity;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Where a fired alarm lands. The framework pushes this on top of whatever the app is showing —
 * after starting the app first, if it was not running.
 */
public class WokeActivity extends Activity {
  @Override
  public void onCreate() {
    AlarmDemoApp.woken = true;
    int id = getIntent().getIntExtra(AlarmDemoApp.EXTRA_ID, -1);

    LinearLayout page = new LinearLayout();
    page.setOrientation(LinearLayout.VERTICAL);
    page.setGravity(Gravity.CENTER);
    TextView label = new TextView();
    label.setText("Woke for alarm " + id);
    page.addView(label);
    setContentView(page);

    Log.i(AlarmDemoApp.TAG, "woke id=" + id);

    // Starting the app again re-ran onCreate, which armed a fresh alarm for
    // three seconds out. Nothing is waiting for that one.
    AlarmDemoApp.operation(this).cancel();
    finish();
  }
}
