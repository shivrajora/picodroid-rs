// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.app.AlertDialog;
import picodroid.os.Bundle;
import picodroid.provider.Settings;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.TextView;

/**
 * Display: the screen timeout, as Android's Display settings carry it. One row shows the current
 * value; a tap (or SELECT) opens a list of choices and the pick is stored through {@link
 * Settings.System#putInt} with {@link Settings.System#SCREEN_OFF_TIMEOUT}, which the display's idle
 * timer reads at once. "Never" keeps the panel on; an app that must stay lit regardless calls
 * {@code View.setKeepScreenOn(true)} itself. The pick is logged as {@code display timeout <ms>}.
 */
public class DisplayActivity extends Activity {
  private static final String TAG = SettingsActivity.TAG;

  private static final int[] CHOICE_MS = {15_000, 30_000, 60_000, 120_000, 300_000, 0};
  private static final String[] CHOICE_LABELS = {
    "15 seconds", "30 seconds", "1 minute", "2 minutes", "5 minutes", "Never"
  };

  private Column column;
  private View timeoutRow;
  private TextView timeoutTail;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    column = new Column(this, "Display", v -> finish());
    column.fill(null, i -> row(i), () -> timeoutRow.requestFocus());
  }

  private View row(int i) {
    if (i != 0) {
      return null;
    }
    timeoutTail = Screens.tail(this, label(currentMs()));
    timeoutRow = Screens.info(this, "Screen timeout", timeoutTail);
    timeoutRow.setOnClickListener(v -> pick());
    return timeoutRow;
  }

  private int currentMs() {
    return Settings.System.getInt(getContentResolver(), Settings.System.SCREEN_OFF_TIMEOUT, 60_000);
  }

  private static String label(int ms) {
    for (int i = 0; i < CHOICE_MS.length; i++) {
      if (CHOICE_MS[i] == ms) {
        return CHOICE_LABELS[i];
      }
    }
    return (ms / 1000) + " seconds";
  }

  private void pick() {
    new AlertDialog.Builder()
        .setTitle("Screen timeout")
        .setItems(
            CHOICE_LABELS,
            (dialog, which) -> {
              int ms = CHOICE_MS[which];
              boolean ok =
                  Settings.System.putInt(
                      getContentResolver(), Settings.System.SCREEN_OFF_TIMEOUT, ms);
              Log.i(TAG, "display timeout " + ms + (ok ? "" : " (not stored)"));
              timeoutTail.setText(label(ms));
            })
        .show();
  }

  @Override
  public void onDestroy() {
    column.stop();
    super.onDestroy();
  }
}
