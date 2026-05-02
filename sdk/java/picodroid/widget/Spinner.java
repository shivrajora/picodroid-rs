// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

public class Spinner extends View {
  private Runnable onItemSelectedListener;

  public Spinner() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void setItems(String items);

  public native int getSelectedItemPosition();

  /**
   * Synthetically fire an item-selected event. Registered OnItemSelectedListener runs on the next
   * main-loop dispatch tick.
   */
  public native void performItemSelected();

  public void setOnItemSelectedListener(Runnable listener) {
    this.onItemSelectedListener = listener;
    nativeRegisterItemSelectedListener();
  }

  private native void nativeRegisterItemSelectedListener();

  void fireItemSelected() {
    if (onItemSelectedListener != null) {
      onItemSelectedListener.run();
    }
  }
}
