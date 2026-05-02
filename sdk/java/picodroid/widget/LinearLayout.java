// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

public class LinearLayout extends View {
  public static final int HORIZONTAL = 0;
  public static final int VERTICAL = 1;

  private int orientation;

  public LinearLayout() {
    super(nativeCreate());
    this.orientation = VERTICAL;
  }

  private static native int nativeCreate();

  public native void addView(View child);

  public native void setOrientation(int orientation);
}
