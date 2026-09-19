// SPDX-License-Identifier: GPL-3.0-only
package bundledemo;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.util.Log;

/**
 * Boot Activity. Deliberately the deprecated no-argument {@code onCreate()}: an Activity written
 * before {@code onCreate(Bundle)} existed must keep working through the bridge.
 */
public class HomeActivity extends Activity {
  static final int REQ = 9;

  /** What StateActivity's two instances must go through, in order. */
  private static final String EXPECTED =
      "1:create(null) 1:start 1:resume 1:pause 1:stop 1:save 1:destroy "
          + "2:create(saved) 2:start 2:restore 2:resume 2:pause 2:stop 2:destroy ";

  @Override
  @SuppressWarnings("deprecation")
  public void onCreate() {
    Log.i(BundleDemoApp.TAG, "Home.onCreate (legacy no-arg)");
    Bundle extras = new Bundle();
    extras.putString("who", "home");
    extras.putInt("seed", 40);
    startActivityForResult(new Intent(StateActivity.class).putExtras(extras), REQ);
  }

  @Override
  protected void onActivityResult(int requestCode, int resultCode, Intent data) {
    String trace = BaseStateActivity.trace.toString();
    Log.i(BundleDemoApp.TAG, "trace: " + trace);
    BundleDemoApp.check("trace", EXPECTED.equals(trace));
    // The for-result launch survived the re-creation: the second instance's result arrives here.
    BundleDemoApp.check("request code", requestCode == REQ);
    BundleDemoApp.check("result code", resultCode == RESULT_OK);
    BundleDemoApp.check("result data", data != null && data.getIntExtra("count", -1) == 42);
    if (BundleDemoApp.failures == 0) {
      Log.i(BundleDemoApp.TAG, "=== PASSED ===");
    } else {
      Log.e(BundleDemoApp.TAG, "=== FAILED: " + BundleDemoApp.failures + " ===");
    }
  }
}
