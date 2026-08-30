// SPDX-License-Identifier: GPL-3.0-only
package bugbashui;

import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * Lifecycle half of the 2026-08-30 bug-bash regression app (see {@code examples/bugbash} for the
 * pure-logic half). Self-driving: {@link MainActivity} walks three phases across its own
 * onResume calls and finishes when done, so the app terminates like a {@code term} row.
 */
public class BugBashUiApp extends Application {
  @Override
  public void onCreate() {
    Log.i("BugBashUi", "=== BugBashUi start ===");
    startActivity(new Intent(MainActivity.class));
  }
}
