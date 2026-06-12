// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

public class ImageView extends View {
  // Mirrors the relevant subset of Android's ScaleType ordinals.
  /** Aspect-preserving fit, centered, no clipping (LVGL CONTAIN). */
  public static final int SCALE_FIT_CENTER = 0;

  /** Aspect-preserving fill — image may be cropped (LVGL COVER). */
  public static final int SCALE_CENTER_CROP = 1;

  /** Stretch to fill the view's width and height, ignoring aspect (LVGL STRETCH). */
  public static final int SCALE_FIT_XY = 2;

  /** Tile the image across the view's area (LVGL TILE). */
  public static final int SCALE_TILE = 3;

  /**
   * Center the image at its intrinsic size with no scaling — may clip if larger than the view (LVGL
   * CENTER). Mirrors Android's {@code ScaleType.CENTER}. ({@code FIT_START} is not supported:
   * LVGL's directional aligns don't auto-scale, so it would need intrinsic-size math — see the
   * compatibility matrix.)
   */
  public static final int SCALE_CENTER = 4;

  /** Native zoom unit: {@code 256} = 1.0×. */
  public static final int SCALE_1X = 256;

  public ImageView() {
    super(nativeCreate());
  }

  public ImageView(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  /**
   * Loads an image source. Path-based loading is not yet implemented on embedded targets — see
   * {@code project_future_milestones.md} (ImageView asset pipeline).
   */
  public native void setImageSource(String path);

  /** One of {@link #SCALE_FIT_CENTER}, {@link #SCALE_CENTER_CROP}, etc. */
  public native void setScaleType(int scaleType);

  /**
   * Apply a tint to the image. {@code argbColor}'s alpha controls blend strength: alpha 0 = no
   * tint, 255 = fully recolored. The lower 24 bits give the tint color.
   */
  public native void setTint(int argbColor);

  /** Uniform scale; use {@link #SCALE_1X} for 1.0×. */
  public native void setScale(int zoom);
}
