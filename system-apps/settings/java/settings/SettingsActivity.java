// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.util.Log;
import picodroid.view.View;

/**
 * The settings app's root (multi-app M3): About, Apps and Storage, one row each. The header row is
 * Home: a tap on it, or BACK on a keypad board, finishes the app and the launcher comes back. Rows
 * are {@link Screens#ROW_HEIGHT} pixels from the top of the screen, the header first, so the bench
 * taps About at y = 60, Apps at 100, Storage at 140.
 */
public class SettingsActivity extends Activity {
  static final String TAG = "Settings";

  private Column column;
  /** Held so the rows stay reachable while their click listeners are live. */
  private final View[] rows = new View[3];
  /** Whether the rows are on screen: "ready" is logged once they are, then on every return. */
  private boolean built;

  @Override
  public void onCreate() {
    column = new Column(this, "Settings", v -> finish());
    column.fill(null, i -> row(i), () -> ready());
  }

  private View row(int i) {
    switch (i) {
      case 0:
        rows[0] =
            Screens.row(this, "About", v -> startActivity(new Intent(AboutActivity.class)));
        return rows[0];
      case 1:
        rows[1] = Screens.row(this, "Apps", v -> startActivity(new Intent(AppsActivity.class)));
        return rows[1];
      case 2:
        rows[2] =
            Screens.row(this, "Storage", v -> startActivity(new Intent(StorageActivity.class)));
        return rows[2];
      default:
        return null;
    }
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
