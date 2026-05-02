// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

public class Button extends View {
  private Runnable onClickListener;

  public Button(String text) {
    super(nativeCreate(text));
  }

  private static native int nativeCreate(String text);

  public native void setText(String text);

  public native boolean wasClicked();

  /**
   * Synthetically fire a click event, same as Android's View.performClick(). The registered
   * OnClickListener runs on the next main-loop dispatch tick. Useful for scripted UI flows,
   * accessibility, and headless end-to-end tests.
   */
  public native void performClick();

  public void setOnClickListener(Runnable listener) {
    this.onClickListener = listener;
    nativeRegisterClickListener();
  }

  private native void nativeRegisterClickListener();

  void fireClick() {
    if (onClickListener != null) {
      onClickListener.run();
    }
  }
}
