// SPDX-License-Identifier: GPL-3.0-only
package bundledemo;

import picodroid.content.Intent;
import picodroid.os.Bundle;

/**
 * First instance: bumps its counter and asks to be re-created. Second instance: checks what came
 * back, reports the counter as its result, and finishes. Declares only some of its callbacks; the
 * rest are on {@link BaseStateActivity}.
 */
public class StateActivity extends BaseStateActivity {
  private boolean restored;

  @Override
  public void onResume() {
    mark("resume");
    if (id == 1) {
      BundleDemoApp.check("seed from extras", count == 40);
      count += 2;
      recreate();
      recreate(); // idempotent while one is pending, like finish()
      return;
    }
    BundleDemoApp.check("restored before resume", restored);
    BundleDemoApp.check("count restored", count == 42);
    BundleDemoApp.check(
        "intent carried over", "home".equals(getIntent().getExtras().getString("who")));
    setResult(RESULT_OK, new Intent(StateActivity.class).putExtra("count", count));
    finish();
  }

  @Override
  protected void onSaveInstanceState(Bundle outState) {
    mark("save");
    BundleDemoApp.check("outState starts empty", outState != null && outState.isEmpty());
    outState.putInt("count", count);
    Bundle nested = new Bundle();
    nested.putString("note", "kept");
    outState.putBundle("nested", nested);
  }

  @Override
  protected void onRestoreInstanceState(Bundle savedInstanceState) {
    mark("restore");
    Bundle nested = savedInstanceState.getBundle("nested");
    BundleDemoApp.check("nested state", nested != null && "kept".equals(nested.getString("note")));
    BundleDemoApp.check("same state as onCreate", savedInstanceState.getInt("count") == count);
    restored = true;
  }
}
