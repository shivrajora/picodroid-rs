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
    nativeBindAdapter(adapter);
  }

  /**
   * Pulls every row from {@code adapter} in one native call, calling {@code getCount()}, {@code
   * getItem(int)} and {@code toString()} back into Java as it goes. Replaces the old Java-side loop
   * that made one {@link #addItem} call per row. A null adapter is a no-op; the caller has already
   * cleared the list.
   */
  private native void nativeBindAdapter(Adapter adapter);
}
