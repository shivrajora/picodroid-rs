// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.LinearLayout;

/**
 * The settings app's root (multi-app M3): About, Apps and Storage, one row each. The header row is
 * Home: a tap on it, or BACK on a keypad board, finishes the app and the launcher comes back. Rows
 * are {@link Screens#ROW_HEIGHT} pixels from the top of the screen, the header first, so the bench
 * taps About at y = 60, Apps at 100, Storage at 140.
 */
public class SettingsActivity extends Activity {
  static final String TAG = "Settings";

  /** Held so the rows stay reachable while their click listeners are live. */
  private View[] rows;

  @Override
  public void onCreate() {
    LinearLayout root = Screens.column(this);
    root.addView(Screens.header(this, "Settings", v -> finish()));
    rows =
        new View[] {
          Screens.row(this, "About", v -> startActivity(new Intent(AboutActivity.class))),
          Screens.row(this, "Apps", v -> startActivity(new Intent(AppsActivity.class))),
          Screens.row(this, "Storage", v -> startActivity(new Intent(StorageActivity.class))),
        };
    for (int i = 0; i < rows.length; i++) {
      root.addView(rows[i]);
    }
    rows[0].requestFocus();
    setContentView(Screens.scrollable(this, root, 1 + rows.length));
  }

  /** Once per showing of the root — the first one and every return from a screen. */
  @Override
  public void onResume() {
    super.onResume();
    Log.i(TAG, "ready");
  }
}
