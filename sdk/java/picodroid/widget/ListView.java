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
  protected void registerNativeItemClick() {
    nativeRegisterItemClickListener();
  }

  private native void nativeRegisterItemClickListener();

  /**
   * Invoked by the framework event loop when a row is activated (ENTER on the focused row, or a
   * touch tap). Resolves the row's stable {@code id} from the bound {@link Adapter} and delivers
   * the full Android {@code onItemClick(parent, view, position, id)} callback. {@code view} is
   * {@code null} — rows are LVGL-native and have no Java View wrapper.
   */
  void fireItemClick(int position) {
    if (onItemClickListener != null) {
      long id = adapter != null ? adapter.getItemId(position) : position;
      onItemClickListener.onItemClick(this, null, position, id);
    }
  }

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
