// SPDX-License-Identifier: GPL-3.0-only
package reclaimdemo;

import picodroid.app.Activity;
import picodroid.os.Bundle;
import picodroid.widget.TextView;

/**
 * Records the Bundle callbacks and carries one saved int. The other lifecycle callbacks are
 * declared on each leaf class: the framework still finds those by name on the leaf alone.
 */
public abstract class TracedActivity extends Activity {
  int id;
  int count;
  boolean restored;

  abstract String tag();

  abstract int nextId();

  void mark(String event) {
    ReclaimDemoApp.trace.append(tag()).append(id).append(':').append(event).append(' ');
  }

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    id = nextId();
    mark(savedInstanceState == null ? "create(null)" : "create(saved)");
    count = savedInstanceState == null ? 5 : savedInstanceState.getInt("count");
    // A view tree for the reclaim to free, and for the new instance to build again.
    TextView label = new TextView();
    label.setText(tag() + id);
    setContentView(label);
  }

  @Override
  protected void onSaveInstanceState(Bundle outState) {
    mark("save");
    outState.putInt("count", count);
  }

  @Override
  protected void onRestoreInstanceState(Bundle savedInstanceState) {
    mark("restore");
    ReclaimDemoApp.check("same state as onCreate", savedInstanceState.getInt("count") == count);
    restored = true;
  }
}
