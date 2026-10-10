// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.content.pm.PackageManager;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.os.Bundle;

/**
 * The settings app's root (multi-app M3): About, Apps and Storage, one row each, then Wi-Fi on a
 * board that has it ({@code PackageManager.FEATURE_WIFI}), then Display and Date &amp; time. The
 * header row is Home: a tap on it, or BACK on a keypad board, finishes the app and the launcher
 * comes back. Rows are {@link Screens#ROW_HEIGHT} pixels from the top of the screen, the header
 * first, so the bench taps About at y = 60, Apps at 100, Storage at 140 and Wi-Fi at 180; Display
 * follows Wi-Fi where there is one and takes its place where there is not, and Date &amp; time
 * follows Display (y = 220 without Wi-Fi, 260 with it: past a 240-pixel panel, which the column
 * scrolls to).
 */
public class SettingsActivity extends Activity {
  static final String TAG = "Settings";

  private Column column;
  /** Held so the rows stay reachable while their click listeners are live. */
  private final View[] rows = new View[6];
  /** The screens, in row order: the Wi-Fi row is left out on a board with no link. */
  private String[] labels;
  private Class<?>[] screens;
  /** Whether the rows are on screen: "ready" is logged once they are, then on every return. */
  private boolean built;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    boolean wifi = getPackageManager().hasSystemFeature(PackageManager.FEATURE_WIFI);
    labels =
        wifi
            ? new String[] {"About", "Apps", "Storage", "Wi-Fi", "Display", "Date & time"}
            : new String[] {"About", "Apps", "Storage", "Display", "Date & time"};
    screens =
        wifi
            ? new Class<?>[] {
              AboutActivity.class,
              AppsActivity.class,
              StorageActivity.class,
              WifiActivity.class,
              DisplayActivity.class,
              DateTimeActivity.class
            }
            : new Class<?>[] {
              AboutActivity.class,
              AppsActivity.class,
              StorageActivity.class,
              DisplayActivity.class,
              DateTimeActivity.class
            };
    column = new Column(this, "Settings", v -> finish());
    column.fill(null, i -> row(i), () -> ready());
  }

  private View row(int i) {
    if (i >= labels.length) {
      return null;
    }
    final Class<?> screen = screens[i];
    rows[i] = Screens.row(this, labels[i], v -> startActivity(new Intent(screen)));
    return rows[i];
  }

  /** The rows are in: focus the first, and say so — the harness keys on this line. */
  private void ready() {
    rows[0].requestFocus();
    built = true;
    Log.i(TAG, "ready");
  }

  /** Once per showing of the root: when its rows are first in, then on every return from a screen. */
  @Override
  public void onResume() {
    super.onResume();
    if (built) {
      Log.i(TAG, "ready");
    }
  }

  @Override
  public void onDestroy() {
    column.stop();
    super.onDestroy();
  }
}
