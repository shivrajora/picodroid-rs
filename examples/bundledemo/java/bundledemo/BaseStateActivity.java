// SPDX-License-Identifier: GPL-3.0-only
package bundledemo;

import picodroid.app.Activity;
import picodroid.os.Bundle;

/**
 * Declares {@code onCreate(Bundle)}, {@code onStart}, {@code onPause}, {@code onStop} and {@code
 * onDestroy} for a subclass that does not: the framework must find an override on a base class,
 * which a by-name lookup on the leaf class alone would miss.
 */
public abstract class BaseStateActivity extends Activity {
  static final StringBuilder trace = new StringBuilder();
  private static int instances;

  final int id = ++instances;
  int count;

  void mark(String event) {
    trace.append(id).append(':').append(event).append(' ');
  }

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    mark(savedInstanceState == null ? "create(null)" : "create(saved)");
    if (savedInstanceState == null) {
      count = getIntent().getIntExtra("seed", 0);
    } else {
      count = savedInstanceState.getInt("count");
    }
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
