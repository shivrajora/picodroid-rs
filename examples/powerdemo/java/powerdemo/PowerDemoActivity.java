// SPDX-License-Identifier: GPL-3.0-only
package powerdemo;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.TimeUnit;
import picodroid.content.Context;
import picodroid.os.Bundle;
import picodroid.os.PowerManager;
import picodroid.provider.Settings;
import picodroid.util.Log;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;

/**
 * Display power, driven from the sim's control channel (test.ctrl): the panel dozes on {@code
 * KEYCODE_SLEEP}, wakes on a tap that lands as no click, toggles on {@code KEYCODE_POWER}, dozes on
 * the idle timeout the app shortens through {@code Settings.System.SCREEN_OFF_TIMEOUT}, and stays
 * lit under {@code View.setKeepScreenOn(true)}. One button fills the window so any tap is a click;
 * each click is numbered in the log, and the first two change the app's power asks.
 */
public class PowerDemoActivity extends Activity {
  private static final String TAG = "PowerDemo";

  private int clicks;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    Button button = new Button(this);
    button.setText("Tap");
    button.setLayoutParams(
        new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
    button.setOnClickListener(v -> onClick(root));
    root.addView(button);
    setContentView(root);

    PowerManager pm = (PowerManager) getSystemService(Context.POWER_SERVICE);
    int timeout =
        Settings.System.getInt(getContentResolver(), Settings.System.SCREEN_OFF_TIMEOUT, -1);
    Log.i(TAG, "ready interactive=" + pm.isInteractive() + " timeout=" + timeout);
  }

  private void onClick(LinearLayout root) {
    clicks++;
    Log.i(TAG, "click " + clicks);
    if (clicks == 1) {
      // Shorten the idle timeout: the next doze comes from the timer, not a key.
      boolean ok =
          Settings.System.putInt(getContentResolver(), Settings.System.SCREEN_OFF_TIMEOUT, 2500);
      Log.i(TAG, "timeout 2500 stored=" + ok);
    } else if (clicks == 2) {
      // Hold the panel on past that timeout, and say so once it would have dozed.
      root.setKeepScreenOn(true);
      PowerManager pm = (PowerManager) getSystemService(Context.POWER_SERVICE);
      Executors.mainScheduledExecutor()
          .schedule(
              () -> {
                Log.i(TAG, "held interactive=" + pm.isInteractive());
                // The simulator's volume outlives this run: put the board default back so
                // the next app does not inherit a 2.5 s timeout.
                Settings.System.putInt(
                    getContentResolver(), Settings.System.SCREEN_OFF_TIMEOUT, 60000);
                finish();
              },
              3,
              TimeUnit.SECONDS);
    }
  }
}
