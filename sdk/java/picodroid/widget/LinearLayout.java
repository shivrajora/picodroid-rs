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
   * Set the alignment of the children. Mirrors {@code android.widget.LinearLayout#setGravity(int)}:
   * pass {@link picodroid.view.Gravity} constants, which may name both axes ({@code Gravity.BOTTOM
   * | Gravity.RIGHT}). The horizontal bits place the children of a {@link #HORIZONTAL} layout along
   * its length and a {@link #VERTICAL} one across it, and the vertical bits the other way round, as
   * on Android; call it after {@link #setOrientation}, which is what decides which is which.
   *
   * <p>Two divergences: an axis the gravity does not name keeps centring rather than falling back
   * to the start, and {@code FILL} places at the start instead of stretching the child. Per-child
   * {@code LayoutParams.gravity} is not yet applied — that is part of the LayoutParams milestone.
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
