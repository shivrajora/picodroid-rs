// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.ViewGroup;

public class LinearLayout extends ViewGroup {
  public static final int HORIZONTAL = 0;
  public static final int VERTICAL = 1;

  private int orientation;

  public LinearLayout() {
    super(nativeCreate());
    this.orientation = VERTICAL;
  }

  public LinearLayout(Context ctx) {
    super(nativeCreate());
    this.orientation = VERTICAL;
  }

  private static native int nativeCreate();

  public native void setOrientation(int orientation);

  /** Gap in pixels between adjacent children. Default 0. */
  public native void setSpacing(int px);

  /**
   * Set the alignment of children along the main axis. {@code gravity} is currently a placeholder
   * for the Android constants; routing into LVGL flex alignment is part of the larger LayoutParams
   * milestone.
   */
  public native void setGravity(int gravity);

  /**
   * Mirrors {@code android.widget.LinearLayout.LayoutParams}. Adds {@code weight} (mapped to LVGL
   * {@code lv_obj_set_flex_grow}) and {@code gravity} (per-child alignment along the cross axis).
   */
  public static class LayoutParams extends ViewGroup.LayoutParams {
    public float weight;
    public int gravity;

    public LayoutParams(int width, int height) {
      super(width, height);
    }

    public LayoutParams(int width, int height, float weight) {
      super(width, height);
      this.weight = weight;
    }
  }
}
