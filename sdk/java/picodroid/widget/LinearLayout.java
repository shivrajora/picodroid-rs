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

  /** For the inflater's native path, which makes the widget itself ({@link InflaterSupport}). */
  protected LinearLayout(int nativeHandle) {
    super(nativeHandle);
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
   * <p>An axis the gravity does not name falls back to its start, as on Android ({@code START |
   * TOP}). {@code FILL} places at the start, as Android's LinearLayout does: only {@code
   * match_parent} stretches a child. A child with a {@code LayoutParams.gravity} of its own is
   * placed across the axis by that instead.
   */
  public native void setGravity(int gravity);

  /**
   * Mirrors {@code android.widget.LinearLayout.LayoutParams}: {@code weight}, the child's share of
   * the space left along the layout's axis (as on Android, {@code 0dp} with a weight is a pure
   * share and {@code wrap_content} with one keeps its content and adds the share); {@code gravity},
   * where the child sits across the axis (a horizontal layout reads the vertical bits, a vertical
   * one the horizontal bits); and the margins, which keep that much space clear around the child.
   */
  public static class LayoutParams extends ViewGroup.MarginLayoutParams {
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
