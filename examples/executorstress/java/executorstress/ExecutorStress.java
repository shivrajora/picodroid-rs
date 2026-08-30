// SPDX-License-Identifier: GPL-3.0-only
package executorstress;

import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * GC-rooting stress for Runnables in flight in the executor queues (bugbash F2). The work happens
 * in {@link StressActivity} — an Application-only app never pumps the main queue, so the posted
 * Runnables would sit undelivered regardless of rooting.
 */
public class ExecutorStress extends Application {
  @Override
  public void onCreate() {
    Log.i("ExecutorStress", "=== ExecutorStress start ===");
    startActivity(new Intent(StressActivity.class));
  }
}
