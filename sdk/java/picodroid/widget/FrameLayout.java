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
   * Mirrors {@code android.widget.FrameLayout.LayoutParams}: {@code gravity} names the edge or
   * centre the child is placed against on each axis ({@code TOP | LEFT} when it names none), and
   * the margins move it in from there, so {@code leftMargin} and {@code topMargin} alone are the
   * child's position. A child added without params is positioned with {@link
   * picodroid.view.View#setPosition setPosition}.
   */
  public static class LayoutParams extends ViewGroup.MarginLayoutParams {
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
