// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

public class Spinner extends AdapterView<Adapter> {
  private OnItemSelectedListener onItemSelectedListener;

  public Spinner() {
    super(nativeCreate());
  }

  public Spinner(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  /**
   * Direct items setter — accepts a newline-separated string. Convenience for apps that haven't
   * migrated to {@link #setAdapter}; programmatic callers should prefer constructing an {@link
   * ArrayAdapter}.
   */
  public native void setItems(String items);

  public native int getSelectedItemPosition();

  /**
   * Synthetically fire an item-selected event for headless testing. Registered listener runs on the
   * next main-loop dispatch tick.
   */
  public native void performItemSelected();

  public void setOnItemSelectedListener(OnItemSelectedListener listener) {
    this.onItemSelectedListener = listener;
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
      onItemSelectedListener.onItemSelected(this, getSelectedItemPosition());
    }
  }

  /**
   * Picodroid's lighter-weight equivalent of {@code AdapterView.OnItemSelectedListener}. Until the
   * adapter pattern is fully wired through native rendering the {@code position} is read straight
   * from the underlying roller; the full Android signature ({@code View, long id}) will be
   * reintroduced when the adapter milestone ships.
   */
  public interface OnItemSelectedListener {
    void onItemSelected(Spinner parent, int position);
  }
}
