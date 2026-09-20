// SPDX-License-Identifier: GPL-3.0-only
package bundledemo;

import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.util.Log;

/** Boot Activity: launches StateActivity for a result and judges what comes back. */
public class HomeActivity extends BaseHomeActivity {
  private boolean launched;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(BundleDemoApp.TAG, "Home.onCreate saved=" + (savedInstanceState != null));
    Bundle extras = new Bundle();
    extras.putString("who", "home");
    extras.putInt("seed", 40);
    startActivityForResult(new Intent(StateActivity.class).putExtras(extras), REQ);
  }

  @Override
  public void onResume() {
    if (!launched) {
      launched = true; // the first resume, before StateActivity covers this one
      return;
    }
    // Both arrive through callbacks only BaseHomeActivity declares.
    BundleDemoApp.check("onActivityResult on the base class", gotResult);
    BundleDemoApp.check("onRestart on the base class", restarted);
    if (BundleDemoApp.failures == 0) {
      Log.i(BundleDemoApp.TAG, "=== PASSED ===");
    } else {
      Log.e(BundleDemoApp.TAG, "=== FAILED: " + BundleDemoApp.failures + " ===");
    }
  }
}
