// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.R;
import picodroid.content.Context;
import picodroid.content.res.ColorStateList;
import picodroid.graphics.Theme;
import picodroid.util.AttributeSet;
import picodroid.view.View;

/**
 * Mirrors {@code android.widget.ProgressBar}. {@code new ProgressBar(context)} is Android's default
 * style, the indeterminate spinner (LVGL's {@code lv_spinner}); the determinate horizontal bar
 * (LVGL's {@code lv_bar}) is {@code new ProgressBar(context, null,
 * picodroid.R.attr.progressBarStyleHorizontal)}, or {@code
 * style="?android:attr/progressBarStyleHorizontal"} in a layout. Progress, range and tint lists are
 * cached Java-side so the getters answer at once, as Android's do; the natives only push state to
 * LVGL.
 *
 * <p>Picodroid divergences: the mode is the style's. The spinner is indeterminate only, as
 * Android's default style is ({@link #setIndeterminate(boolean) setIndeterminate(false)} leaves it
 * spinning, as there); a horizontal bar cannot become indeterminate ({@code setIndeterminate(true)}
 * throws), since LVGL's bar has no indeterminate animation and a widget cannot change its LVGL
 * class. The small and large spinner styles are the spinner at the size its layout gives. A {@link
 * ColorStateList} is a single colour; no secondary progress, progress drawable, interpolator or
 * tint mode.
 */
public class ProgressBar extends View {
  private int min;
  private int max = 100;

  /** Stays 0 while indeterminate, so {@link #getProgress()} is 0 there, as on Android. */
  private int progress;

  /** The spinner; fixed at construction: LVGL can't morph an lv_bar into an lv_spinner. */
  private boolean indeterminate;

  private ColorStateList progressTint;
  private ColorStateList progressBackgroundTint;
  private ColorStateList indeterminateTint;

  /**
   * Android's default style: the indeterminate spinner. Its moving arc is {@link
   * Theme#colorPrimary} (rebrand by assigning {@code Theme} fields before any UI is built);
   * override it per instance with {@link #setIndeterminateTintList(ColorStateList)}.
   */
  public ProgressBar(Context context) {
    this(context, null, 0);
  }

  /** As {@link #ProgressBar(Context)}: the attributes of a layout are applied by the inflater. */
  public ProgressBar(Context context, AttributeSet attrs) {
    this(context, attrs, 0);
  }

  /**
   * {@code picodroid.R.attr.progressBarStyleHorizontal} makes the determinate horizontal bar; any
   * other style attribute (0, {@code progressBarStyle}, {@code progressBarStyleSmall}, {@code
   * progressBarStyleLarge}) the spinner.
   */
  public ProgressBar(Context context, AttributeSet attrs, int defStyleAttr) {
    super(
        context,
        defStyleAttr == R.attr.progressBarStyleHorizontal
            ? nativeCreate()
            : nativeCreateIndeterminate(Theme.colorPrimary));
    indeterminate = defStyleAttr != R.attr.progressBarStyleHorizontal;
  }

  /**
   * Adopts a widget a subclass created — {@link CircularProgressIndicator} passes its {@code
   * lv_arc} here so the LVGL bar is never built for it.
   */
  protected ProgressBar(int nativeHandle) {
    super(nativeHandle);
  }

  protected ProgressBar(Context ctx, int nativeHandle) {
    super(ctx, nativeHandle);
  }

  private static native int nativeCreate();

  private static native int nativeCreateIndeterminate(int argb);

  // The natives below address the determinate lv_bar (nativeSetTint target 2 excepted). LVGL's
  // object asserts are off, so an lv_bar call on the spinner would be undefined behaviour rather
  // than a checked no-op: every caller tests `indeterminate` first. They are package-private,
  // not private, for the same reason: a private native compiles to invokespecial, which the
  // runtime dispatches by this class, so a subclass over another LVGL widget (the ring gauge's
  // lv_arc) would receive lv_bar calls; invokevirtual routes by the receiver's class.
  native void nativeSetProgress(int value, boolean animate);

  native void nativeSetRange(int min, int max, int progress);

  /**
   * {@code target} 0 = progress (the fill), 1 = progress background (the track), 2 = the
   * indeterminate arc; keep in step with lvgl/widgets/progress_bar.rs. {@code apply == false} drops
   * the instance colour so the theme's shows again (bar targets only).
   */
  native void nativeSetTint(int target, int argb, boolean apply);

  /**
   * Sets the progress at once, clamped to {@code [getMin(), getMax()]}; no-op when indeterminate.
   */
  public void setProgress(int progress) {
    setProgressInternal(progress, false);
  }

  /** As {@link #setProgress(int)}; with {@code animate} the fill moves there over 80 ms. */
  public void setProgress(int progress, boolean animate) {
    setProgressInternal(progress, animate);
  }

  private void setProgressInternal(int value, boolean animate) {
    if (indeterminate) {
      return;
    }
    if (value < min) {
      value = min;
    } else if (value > max) {
      value = max;
    }
    if (value == progress) {
      return;
    }
    progress = value;
    nativeSetProgress(value, animate);
  }

  public final void incrementProgressBy(int diff) {
    setProgress(progress + diff);
  }

  /**
   * The last value set, clamped to the range (the target while animating); 0 when indeterminate.
   */
  public int getProgress() {
    return progress;
  }

  /**
   * Upper bound of the range, never below {@link #getMin()}; a progress above it is pulled down.
   */
  public void setMax(int max) {
    if (max < min) {
      max = min;
    }
    if (max == this.max) {
      return;
    }
    this.max = max;
    if (progress > max) {
      progress = max;
    }
    if (!indeterminate) {
      nativeSetRange(min, max, progress);
    }
  }

  public int getMax() {
    return max;
  }

  /** Lower bound of the range, never above {@link #getMax()}; a progress below it is pushed up. */
  public void setMin(int min) {
    if (min > max) {
      min = max;
    }
    if (min == this.min) {
      return;
    }
    this.min = min;
    if (progress < min) {
      progress = min;
    }
    if (!indeterminate) {
      nativeSetRange(min, max, progress);
    }
  }

  public int getMin() {
    return min;
  }

  /** Whether this is the spinner. Mirrors {@code android.widget.ProgressBar#isIndeterminate()}. */
  public boolean isIndeterminate() {
    return indeterminate;
  }

  /**
   * Mirrors {@code android.widget.ProgressBar#setIndeterminate(boolean)} where the mode can follow:
   * the spinner is indeterminate only, as Android's default style is, so {@code false} leaves it
   * spinning, as on Android.
   *
   * @throws UnsupportedOperationException for {@code true} on a horizontal bar (a picodroid
   *     divergence: LVGL's bar has no indeterminate animation); use {@link #ProgressBar(Context)}
   */
  public void setIndeterminate(boolean indeterminate) {
    if (indeterminate && !this.indeterminate) {
      throw new UnsupportedOperationException(
          "a horizontal ProgressBar cannot be indeterminate; use new ProgressBar(context)");
    }
  }

  /**
   * Colours the fill of a determinate bar; the colour's alpha is honoured. {@code null} returns the
   * fill to the theme colour. On an indeterminate bar the list is kept for {@link
   * #getProgressTintList()} but not shown, as Android applies it to the determinate drawable only.
   */
  public void setProgressTintList(ColorStateList tint) {
    if (sameTint(tint, progressTint)) {
      return;
    }
    progressTint = tint;
    if (!indeterminate) {
      applyTint(0, tint);
    }
  }

  public ColorStateList getProgressTintList() {
    return progressTint;
  }

  /**
   * Colours the track behind the fill; otherwise as {@link #setProgressTintList(ColorStateList)}.
   */
  public void setProgressBackgroundTintList(ColorStateList tint) {
    if (sameTint(tint, progressBackgroundTint)) {
      return;
    }
    progressBackgroundTint = tint;
    if (!indeterminate) {
      applyTint(1, tint);
    }
  }

  public ColorStateList getProgressBackgroundTintList() {
    return progressBackgroundTint;
  }

  /**
   * Colours the moving arc of an indeterminate bar; {@code null} returns it to {@link
   * Theme#colorPrimary}. Kept but not shown on a determinate bar.
   */
  public void setIndeterminateTintList(ColorStateList tint) {
    indeterminateTint = tint;
    if (indeterminate) {
      applyTint(2, tint);
    }
  }

  public ColorStateList getIndeterminateTintList() {
    return indeterminateTint;
  }

  /** Whether {@code a} tints as {@code b} does: setting such a tint again changes nothing. */
  private static boolean sameTint(ColorStateList a, ColorStateList b) {
    if (a == null || b == null) {
      return a == b;
    }
    return a.getDefaultColor() == b.getDefaultColor();
  }

  private void applyTint(int target, ColorStateList tint) {
    if (tint != null) {
      nativeSetTint(target, tint.getDefaultColor(), true);
    } else if (target == 2) {
      nativeSetTint(2, Theme.colorPrimary, true);
    } else {
      nativeSetTint(target, 0, false);
    }
  }
}
