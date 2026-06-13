// SPDX-License-Identifier: GPL-3.0-only
package navdemo;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * Launched for-result by HomeActivity. Immediately reports a result and finishes, so the whole
 * startActivityForResult → setResult → onActivityResult round-trip runs deterministically (no
 * touch) for the HIL/sim check.
 */
public class ResultProbeActivity extends Activity {
  @Override
  public void onCreate() {
    Log.i("NavDemo", "Probe.onCreate");
    setResult(RESULT_OK, new Intent(ResultProbeActivity.class).putExtra("answer", 42));
    finish();
  }
}
