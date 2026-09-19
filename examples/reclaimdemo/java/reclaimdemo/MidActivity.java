// SPDX-License-Identifier: GPL-3.0-only
package reclaimdemo;

import picodroid.content.Intent;

/**
 * Launched without a result, launches Leaf for one: with Home below it, two reclaimed entries are
 * on the stack at once, and Leaf's result must reach Mid and stop there.
 */
public class MidActivity extends TracedActivity {
  static final int REQ = 11;

  private boolean gotResult;
  private static int instances;

  @Override
  String tag() {
    return "M";
  }

  @Override
  int nextId() {
    return ++instances;
  }

  @Override
  public void onResume() {
    mark("resume");
    if (id == 1) {
      count = 77;
      startActivityForResult(new Intent(LeafActivity.class), REQ);
    } else {
      ReclaimDemoApp.check("mid restored", restored && count == 77);
      ReclaimDemoApp.check("mid result before resume", gotResult);
      finish();
    }
  }

  @Override
  protected void onActivityResult(int requestCode, int resultCode, Intent data) {
    mark("result");
    gotResult = true;
    ReclaimDemoApp.check("mid request code", requestCode == REQ);
    ReclaimDemoApp.check("mid result", resultCode == RESULT_FIRST_USER && data == null);
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
