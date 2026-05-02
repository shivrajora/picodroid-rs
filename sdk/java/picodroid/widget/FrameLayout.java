// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

public class FrameLayout extends View {

  public FrameLayout() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void addView(View child);
}
