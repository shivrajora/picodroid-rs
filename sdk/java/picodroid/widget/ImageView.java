// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.content.res.ColorStateList;
import picodroid.graphics.drawable.Drawable;
import picodroid.view.View;

/**
 * Mirrors {@code android.widget.ImageView} over LVGL's {@code lv_image}: shows a {@code
 * res/drawable/} PNG ({@link #setImageResource}) or a {@link Drawable}, placed by a {@link
 * ScaleType} ({@link ScaleType#FIT_CENTER} by default, as on Android).
 *
 * <p>Picodroid divergences: {@link ScaleType#FIT_START} and {@link ScaleType#FIT_END} draw as
 * {@link ScaleType#FIT_CENTER} (LVGL scales to fit only around the centre), and {@link
 * ScaleType#MATRIX} draws the image unscaled at the top-left, the identity matrix (there is no
 * {@code setImageMatrix}); {@link #getScaleType()} returns what was set either way. One colour
 * effect at a time: {@link #setImageTintList} and {@link #setColorFilter(int)} both recolour the
 * image, and the last one set shows. No {@code setImageBitmap}, {@code setImageURI}, {@code
 * setAdjustViewBounds} or tint mode.
 */
public class ImageView extends View {
  /**
   * Mirrors {@code android.widget.ImageView.ScaleType}: how the image fits the view. A final class
   * of constants rather than an enum (no {@code values()} or {@code switch}): an enum's tables
   * would cost flash on every board.
   */
  public static final class ScaleType {
    /** Unscaled at the top-left: the identity image matrix. */
    public static final ScaleType MATRIX = new ScaleType(8);

    /** Stretched to the view's width and height, aspect not kept. */
    public static final ScaleType FIT_XY = new ScaleType(2);

    /** As {@link #FIT_CENTER} here; Android aligns the fitted image to the top-left. */
    public static final ScaleType FIT_START = new ScaleType(6);

    /** Scaled, aspect kept, to fit inside the view, and centred. The default. */
    public static final ScaleType FIT_CENTER = new ScaleType(0);

    /** As {@link #FIT_CENTER} here; Android aligns the fitted image to the bottom-right. */
    public static final ScaleType FIT_END = new ScaleType(7);

    /** Unscaled and centred; clipped when larger than the view. */
    public static final ScaleType CENTER = new ScaleType(4);

    /** Scaled, aspect kept, to cover the view, and centred; the overflow is clipped. */
    public static final ScaleType CENTER_CROP = new ScaleType(1);

    /** Centred, and scaled down to fit when larger than the view; never scaled up. */
    public static final ScaleType CENTER_INSIDE = new ScaleType(5);

    /**
     * The code the layout compiler writes for {@code android:scaleType} and the runtime maps to an
     * LVGL inner alignment (papk-pack's {@code SCALE_TYPE}, {@code lvgl/widgets/image_view.rs}).
     */
    final int code;

    private ScaleType(int code) {
      this.code = code;
    }
  }

  private ScaleType mScaleType = ScaleType.FIT_CENTER;
  private ColorStateList mImageTint;

  public ImageView(Context ctx) {
    super(ctx, nativeCreate());
  }

  /** For the inflater's native path, which makes the widget itself ({@link InflaterSupport}). */
  protected ImageView(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();

  /**
   * Mirrors Android: shows {@code R.drawable.*} — a PNG under {@code res/drawable/}, decoded to the
   * display's pixel format at build time.
   *
   * @throws picodroid.content.res.Resources.NotFoundException if {@code resId} is not a drawable
   *     resource of this app
   */
  public native void setImageResource(int resId);

  /**
   * Show a Drawable: the {@link picodroid.graphics.drawable.BitmapDrawable} the package manager
   * returns for another app's icon, or any Drawable that can paint a view.
   */
  public void setImageDrawable(Drawable drawable) {
    drawable.applyTo(this);
  }

  /**
   * How the image fits the view; see {@link ScaleType} for the ones drawn differently here.
   *
   * @throws NullPointerException if {@code scaleType} is null, as on Android
   */
  public void setScaleType(ScaleType scaleType) {
    if (scaleType == null) {
      throw new NullPointerException();
    }
    if (scaleType != mScaleType) {
      mScaleType = scaleType;
      nativeSetScaleType(scaleType.code);
    }
  }

  public ScaleType getScaleType() {
    return mScaleType;
  }

  /**
   * Recolours the image with the tint's colour, as Android's default {@code SRC_IN} tint mode does;
   * the colour's alpha is the strength of the recolour. {@code null} removes the tint.
   */
  public void setImageTintList(ColorStateList tint) {
    mImageTint = tint;
    nativeSetTint(tint == null ? 0 : tint.getDefaultColor());
  }

  public ColorStateList getImageTintList() {
    return mImageTint;
  }

  /**
   * Mirrors {@code ImageView.setColorFilter(int)}, a {@code SRC_ATOP} filter: the colour is laid
   * over the image's opaque pixels, its alpha the strength. Alpha 0 clears it.
   */
  public final void setColorFilter(int color) {
    nativeSetTint(color);
  }

  public final void clearColorFilter() {
    nativeSetTint(0);
  }

  private native void nativeSetScaleType(int code);

  /** Recolours the image: the low 24 bits the colour, the top byte the strength (0 = none). */
  private native void nativeSetTint(int argb);
}
