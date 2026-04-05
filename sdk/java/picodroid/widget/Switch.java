package picodroid.widget;

import picodroid.view.View;

public class Switch extends View {
  public Switch() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native boolean isChecked();

  public native void setChecked(boolean checked);
}
