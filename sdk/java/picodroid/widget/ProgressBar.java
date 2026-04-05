package picodroid.widget;

import picodroid.view.View;

public class ProgressBar extends View {
  public ProgressBar() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void setProgress(int value);
}
