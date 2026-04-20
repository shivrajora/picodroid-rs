package picodroid.widget;

import picodroid.view.View;

public class ToggleButton extends View {
  private Runnable onCheckedChangeListener;

  public ToggleButton() {
    super(nativeCreate());
  }

  public ToggleButton(String textOn, String textOff) {
    super(nativeCreateWithText(textOn, textOff));
  }

  private static native int nativeCreate();

  private static native int nativeCreateWithText(String textOn, String textOff);

  public native boolean isChecked();

  public native void setChecked(boolean checked);

  public native void toggle();

  /**
   * Synthetically toggle and fire a checked-change event. Registered OnCheckedChangeListener runs
   * on the next main-loop dispatch tick. Useful for scripted UI flows and headless end-to-end
   * tests.
   */
  public native void performCheckedChange();

  public native void setTextOn(String text);

  public native void setTextOff(String text);

  public void setOnCheckedChangeListener(Runnable listener) {
    this.onCheckedChangeListener = listener;
    nativeRegisterCheckedChangeListener();
  }

  private native void nativeRegisterCheckedChangeListener();

  void fireCheckedChanged() {
    if (onCheckedChangeListener != null) {
      onCheckedChangeListener.run();
    }
  }
}
