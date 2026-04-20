package picodroid.widget;

import picodroid.view.View;

public class CheckBox extends View {
  private Runnable onCheckedChangeListener;

  public CheckBox() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void setText(String text);

  public native boolean isChecked();

  public native void setChecked(boolean checked);

  /**
   * Synthetically toggle and fire a checked-change event. Registered OnCheckedChangeListener runs
   * on the next main-loop dispatch tick.
   */
  public native void performCheckedChange();

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
