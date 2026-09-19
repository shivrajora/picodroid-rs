// SPDX-License-Identifier: GPL-3.0-only
package reclaimdemo;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * Boot Activity, three instances. The first launches Child for a result and is reclaimed under it;
 * the second gets that result, launches Mid and is reclaimed again; the third checks the trace.
 */
public class HomeActivity extends TracedActivity {
  static final int REQ = 7;

  private static final String EXPECTED =
      "H1:create(null) H1:start H1:resume H1:pause C1:create(null) C1:start C1:resume "
          + "H1:stop H1:save H1:destroy C1:pause C1:stop C1:destroy "
          + "H2:create(saved) H2:start H2:restore H2:result H2:resume "
          + "H2:pause M1:create(null) M1:start M1:resume H2:stop H2:save H2:destroy "
          + "M1:pause L1:create(null) L1:start L1:resume M1:stop M1:save M1:destroy "
          + "L1:pause L1:stop L1:destroy "
          + "M2:create(saved) M2:start M2:restore M2:result M2:resume "
          + "M2:pause M2:stop M2:destroy H3:create(saved) H3:start H3:restore H3:resume ";

  /** The first instance, kept past its destruction to check the framework ignores it. */
  private static Activity stale;

  private boolean gotResult;
  private static int instances;

  @Override
  String tag() {
    return "H";
  }

  @Override
  int nextId() {
    return ++instances;
  }

  @Override
  public void onResume() {
    mark("resume");
    if (id == 1) {
      stale = this;
      count++;
      startActivityForResult(new Intent(ChildActivity.class), REQ);
    } else if (id == 2) {
      ReclaimDemoApp.check("home restored before resume", restored);
      ReclaimDemoApp.check("home count restored", count == 6);
      ReclaimDemoApp.check("result before resume", gotResult);
      // A destroyed instance is off the stack: neither call may touch the entry it used to own.
      stale.finish();
      stale.recreate();
      startActivity(new Intent(MidActivity.class));
    } else {
      ReclaimDemoApp.check("home count restored twice", restored && count == 6);
      // Mid's result was for Mid: nothing may arrive here.
      ReclaimDemoApp.check("no result for home", !gotResult);
      String trace = ReclaimDemoApp.trace.toString();
      Log.i(ReclaimDemoApp.TAG, "trace: " + trace);
      ReclaimDemoApp.check("trace", EXPECTED.equals(trace));
      if (ReclaimDemoApp.failures == 0) {
        Log.i(ReclaimDemoApp.TAG, "=== PASSED ===");
      } else {
        Log.e(ReclaimDemoApp.TAG, "=== FAILED: " + ReclaimDemoApp.failures + " ===");
      }
    }
  }

  @Override
  protected void onActivityResult(int requestCode, int resultCode, Intent data) {
    mark("result");
    gotResult = true;
    // The caller was a different instance when it asked; the stack entry is what was remembered.
    ReclaimDemoApp.check("home request code", requestCode == REQ);
    ReclaimDemoApp.check("home result code", resultCode == RESULT_OK);
    ReclaimDemoApp.check("home result data", data != null && data.getIntExtra("answer", 0) == 42);
  }

  @Override
  public void onStart() {
    mark("start");
  }

  @Override
  public void onPause() {
    mark("pause");
  }

  @Override
  public void onStop() {
    mark("stop");
  }

  @Override
  public void onDestroy() {
    mark("destroy");
  }
}
