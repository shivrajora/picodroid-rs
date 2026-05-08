// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.ViewGroup;

public class FrameLayout extends ViewGroup {

  public FrameLayout() {
    super(nativeCreate());
  }

  public FrameLayout(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  /**
   * Mirrors {@code android.widget.FrameLayout.LayoutParams}. Adds {@code gravity} for child
   * placement; FrameLayout otherwise positions children via {@link picodroid.view.View#setPosition
   * setPosition}.
   */
  public static class LayoutParams extends ViewGroup.LayoutParams {
    public int gravity;

    public LayoutParams(int width, int height) {
      super(width, height);
    }

    public LayoutParams(int width, int height, int gravity) {
      super(width, height);
      this.gravity = gravity;
    }
  }
}
