// SPDX-License-Identifier: GPL-3.0-only
package alarmdemo;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.Intent;
import picodroid.content.pm.PackageManager;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.view.Gravity;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * What the user sees while the alarm is pending, and the thing that walks the app out of itself so
 * the alarm has something to bring it back from.
 */
public class MainActivity extends Activity {
  /** The launcher's package: where an app goes when it leaves. */
  private static final String LAUNCHER = "picodroid.launcher";

  /** Long enough for this Activity to be up and settled before leaving. */
  private static final int LEAVE_AFTER_MS = 1500;

  @Override
  public void onCreate() {
    LinearLayout page = new LinearLayout();
    page.setOrientation(LinearLayout.VERTICAL);
    page.setGravity(Gravity.CENTER);
    TextView label = new TextView();
    label.setText("Alarm armed. Leaving...");
    page.addView(label);
    setContentView(page);

    // A thread that sleeps and posts, because the main loop is a single
    // queue: a Runnable that re-posted itself would spin instead of waiting.
    new Thread(this::leaveWhenSettled).start();
  }

  @Override
  public void onResume() {
    // Back from the ring: nothing left to wait for.
    if (AlarmDemoApp.woken) {
      finish();
    }
  }

  /** Go to the launcher, if this board has one to go to. */
  private void leaveWhenSettled() {
    SystemClock.sleep(LEAVE_AFTER_MS);
    Executors.mainExecutor().execute(this::leave);
  }

  private void leave() {
    if (AlarmDemoApp.woken) {
      return;
    }
    PackageManager pm = getPackageManager();
    Intent home = pm.getLaunchIntentForPackage(LAUNCHER);
    if (home == null) {
      // One app on its own: leaving would end the run before the alarm
      // could fire, so stay up and let it arrive in this run instead.
      Log.i(AlarmDemoApp.TAG, "no launcher, waiting here");
      return;
    }
    Log.i(AlarmDemoApp.TAG, "leaving for the launcher");
    startActivity(home);
  }
}
