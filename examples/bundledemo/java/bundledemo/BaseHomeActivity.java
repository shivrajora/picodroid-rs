// SPDX-License-Identifier: GPL-3.0-only
package bundledemo;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * Declares {@code onActivityResult} and {@code onRestart} for {@link HomeActivity}, which does not:
 * like {@link BaseStateActivity}, callbacks the framework must find one class up from the leaf.
 */
public abstract class BaseHomeActivity extends Activity {
  static final int REQ = 9;

  /** What StateActivity's two instances must go through, in order. */
  private static final String EXPECTED =
      "1:create(null) 1:start 1:resume 1:pause 1:stop 1:save 1:destroy "
          + "2:create(saved) 2:start 2:restore 2:resume 2:pause 2:stop 2:destroy ";

  boolean gotResult;
  boolean restarted;

  @Override
  protected void onActivityResult(int requestCode, int resultCode, Intent data) {
    gotResult = true;
    String trace = BaseStateActivity.trace.toString();
    Log.i(BundleDemoApp.TAG, "trace: " + trace);
    BundleDemoApp.check("trace", EXPECTED.equals(trace));
    // The for-result launch survived the re-creation: the second instance's result arrives here.
    BundleDemoApp.check("request code", requestCode == REQ);
    BundleDemoApp.check("result code", resultCode == RESULT_OK);
    BundleDemoApp.check("result data", data != null && data.getIntExtra("count", -1) == 42);
    BundleDemoApp.check("result before restart", !restarted);
  }

  @Override
  public void onRestart() {
    restarted = true;
  }
}
