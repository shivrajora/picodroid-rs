// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

public class Spinner extends AdapterView<Adapter> {
  public Spinner(Context ctx) {
    super(ctx, nativeCreate());
    setFocusable(true);
  }

  private static native int nativeCreate();

  /** The rows, newline-separated: what {@link #refreshFromAdapter} pushes. */
  private native void setItems(String items);

  public native int getSelectedItemPosition();

  /**
   * Synthetically fire an item-selected event for headless testing. Registered listener runs on the
   * next main-loop dispatch tick.
   */
  public native void performItemSelected();

  @Override
  protected void registerNativeItemSelected() {
    nativeRegisterItemSelectedListener();
  }

  private native void nativeRegisterItemSelectedListener();

  @Override
  protected void refreshFromAdapter() {
    if (adapter == null) {
      setItems("");
      return;
    }
    StringBuilder sb = new StringBuilder();
    int n = adapter.getCount();
    for (int i = 0; i < n; i++) {
      if (i > 0) {
        sb.append('\n');
      }
      Object item = adapter.getItem(i);
      sb.append(item == null ? "" : item.toString());
    }
    setItems(sb.toString());
  }

  void fireItemSelected() {
    if (onItemSelectedListener != null) {
      int position = getSelectedItemPosition();
      long id = adapter != null ? adapter.getItemId(position) : position;
      onItemSelectedListener.onItemSelected(this, null, position, id);
    }
  }
}
