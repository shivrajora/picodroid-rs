// SPDX-License-Identifier: GPL-3.0-only
package reclaimdemo;

import picodroid.content.Intent;

/** Answers Home with a result Intent, which has to outlive Home's re-creation. */
public class ChildActivity extends TracedActivity {
  private static int instances;

  @Override
  String tag() {
    return "C";
  }

  @Override
  int nextId() {
    return ++instances;
  }

  @Override
  public void onResume() {
    mark("resume");
    setResult(RESULT_OK, new Intent(HomeActivity.class).putExtra("answer", 42));
    finish();
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
