// SPDX-License-Identifier: GPL-3.0-only
package reclaimdemo;

/** Top of the three-deep stack: a result code and no Intent. */
public class LeafActivity extends TracedActivity {
  private static int instances;

  @Override
  String tag() {
    return "L";
  }

  @Override
  int nextId() {
    return ++instances;
  }

  @Override
  public void onResume() {
    mark("resume");
    setResult(RESULT_FIRST_USER);
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
