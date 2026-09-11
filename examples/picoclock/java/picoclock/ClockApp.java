// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import picoclock.ui.ClockActivity;
import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * A clock and alarm app for the pico_touch_kit board: a 320x480 capacitive panel on a Pico Plus 2
 * W, with the carrier's buzzer on GP13 for the ring.
 *
 * <p>Entry point. It starts {@link AlarmService} before showing anything, so the alarms are being
 * watched from the moment the app is up and go on being watched while the user is three screens
 * deep; the Service is both started and bound, so it outlives the screens that bind it.
 */
public class ClockApp extends Application {
  /** The log tag every class in the app writes under. */
  public static final String TAG = "PicoClock";

  /** The preferences file behind {@link AlarmStore}. */
  public static final String PREFS_NAME = "picoclock";

  @Override
  public void onCreate() {
    // The date and schedule arithmetic is the part of this app that no amount
    // of tapping at the panel can exercise: an alarm that does not ring is
    // silent in both senses. Check it where a failure is visible.
    SelfTest.run();

    startService(new Intent(AlarmService.class));
    startActivity(new Intent(ClockActivity.class));
    Log.i(TAG, "started");
  }
}
