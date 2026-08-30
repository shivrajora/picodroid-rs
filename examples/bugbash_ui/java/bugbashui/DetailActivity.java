// SPDX-License-Identifier: GPL-3.0-only
package bugbashui;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.util.Log;

public class DetailActivity extends Activity {
  static int creates = 0;

  @Override
  public void onCreate() {
    creates++;
    Log.i("BugBashUi", "Detail.onCreate #" + creates);
    Intent i = getIntent();
    if (i != null && i.getIntExtra("once", 0) == 1) {
      finish();
      return;
    }
    // F1: Android's finish() is idempotent; a second call must not also pop
    // the parent Activity.
    finish();
    finish();
  }
}
