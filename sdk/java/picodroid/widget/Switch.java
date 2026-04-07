package picodroid.widget;

import picodroid.view.View;

public class Switch extends View {
  private Runnable onCheckedChangeListener;

  public Switch() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native boolean isChecked();

  public native void setChecked(boolean checked);

  public native void toggle();

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
