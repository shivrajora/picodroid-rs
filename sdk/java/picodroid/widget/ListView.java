// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

public class ListView extends AdapterView<Adapter> {
  public ListView() {
    super(nativeCreate());
  }

  public ListView(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  /** Append a single item. Convenience kept for parity with the pre-adapter API. */
  public native void addItem(String text);

  @Override
  protected void refreshFromAdapter() {
    removeAllViews();
    if (adapter == null) {
      return;
    }
    int n = adapter.getCount();
    for (int i = 0; i < n; i++) {
      Object item = adapter.getItem(i);
      addItem(item == null ? "" : item.toString());
    }
  }
}
