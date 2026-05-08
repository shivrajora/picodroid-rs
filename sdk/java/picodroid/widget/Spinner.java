// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

public class Spinner extends View {
  private OnItemSelectedListener onItemSelectedListener;

  public Spinner() {
    super(nativeCreate());
  }

  public Spinner(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  /**
   * Set the dropdown items. Accepts a newline-separated string for now; an {@code Adapter}-based
   * variant is planned (see {@code project_future_milestones.md} resource-system milestone).
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

  void fireItemSelected() {
    if (onItemSelectedListener != null) {
      onItemSelectedListener.onItemSelected(this, getSelectedItemPosition());
    }
  }

  /**
   * Picodroid's lighter-weight equivalent of {@code AdapterView.OnItemSelectedListener}. Until an
   * adapter pattern lands the {@code position} is read straight from the underlying roller; the
   * full Android signature ({@code View, long id}) will be reintroduced when the adapter milestone
   * ships.
   */
  public interface OnItemSelectedListener {
    void onItemSelected(Spinner parent, int position);
  }
}
