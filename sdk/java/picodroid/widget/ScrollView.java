package picodroid.widget;

import picodroid.view.View;

public class ScrollView extends View {

  public ScrollView() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void addView(View child);
}
