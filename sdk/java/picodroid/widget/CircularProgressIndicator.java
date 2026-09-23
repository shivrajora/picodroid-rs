// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.content.res.ColorStateList;
import picodroid.graphics.Theme;

/**
 * A determinate ring gauge: the progress is drawn as an arc over a circular track. Mirrors Material
 * Components' {@code com.google.android.material.progressindicator.CircularProgressIndicator}, a
 * {@link ProgressBar} subclass, folded into {@code picodroid.widget} like {@link Snackbar}. Backed
 * by LVGL's {@code lv_arc}.
 *
 * <p>Progress and range are {@link ProgressBar}'s. The ring's own knobs keep Material's names:
 * {@link #setIndicatorColor(int)}, {@link #setTrackColor(int)}, {@link #setTrackThickness(int)},
 * {@link #setIndicatorSize(int)}, {@link #setIndicatorDirection(int)} and {@link
 * #setTrackCornerRadius(int)}.
 *
 * <p>Picodroid extensions, for gauges that are not a full circle: {@link #setStartAngle(float)} and
 * {@link #setSweepAngle(float)}, in {@code android.graphics.Canvas#drawArc} terms — degrees, 0 at 3
 * o'clock, clockwise. The defaults (270, 360) draw a full ring that fills from 12 o'clock; a
 * dashboard gauge is {@code setStartAngle(135); setSweepAngle(270)}.
 *
 * <p>Divergences: determinate only (for a spinner use {@link ProgressBar#indeterminate()}); one
 * indicator colour, so {@link #getIndicatorColor()} returns an {@code int} where Material returns
 * {@code int[]}; no {@code indicatorInset}; a colour's alpha is honoured, so {@code
 * Color.TRANSPARENT} hides a track; angles are rounded to whole degrees; {@code indicatorSize} is
 * the intrinsic size, and explicit layout dimensions win over it, as on Android.
 */
public class CircularProgressIndicator extends ProgressBar {
  public static final int INDICATOR_DIRECTION_CLOCKWISE = 0;
  public static final int INDICATOR_DIRECTION_COUNTERCLOCKWISE = 1;

  /** Material's medium size; the ring is square. */
  private static final int DEFAULT_INDICATOR_SIZE = 48;

  private static final int DEFAULT_TRACK_THICKNESS = 4;

  private int indicatorColor = Theme.colorPrimary;
  private int trackColor = Theme.colorOutline;
  private int trackThickness = DEFAULT_TRACK_THICKNESS;
  private int indicatorSize = DEFAULT_INDICATOR_SIZE;
  private int indicatorDirection = INDICATOR_DIRECTION_CLOCKWISE;
  private int trackCornerRadius = DEFAULT_TRACK_THICKNESS / 2;
  private float startAngle = 270f;
  private float sweepAngle = 360f;

  /** A 48 px ring in the theme's primary colour over an outline-coloured track, at 0. */
  public CircularProgressIndicator() {
    super(nativeCreate(Theme.colorPrimary, Theme.colorOutline));
  }

  public CircularProgressIndicator(Context ctx) {
    this();
  }

  private static native int nativeCreate(int indicatorArgb, int trackArgb);

  /**
   * {@link ProgressBar}'s tint lists apply to a ring too, as Material's do: the progress tint is
   * the indicator colour and the progress-background tint the track colour; {@code null} returns
   * each to its theme default.
   */
  @Override
  public void setProgressTintList(ColorStateList tint) {
    super.setProgressTintList(tint);
    setIndicatorColor(tint == null ? Theme.colorPrimary : tint.getDefaultColor());
  }

  @Override
  public void setProgressBackgroundTintList(ColorStateList tint) {
    super.setProgressBackgroundTintList(tint);
    setTrackColor(tint == null ? Theme.colorOutline : tint.getDefaultColor());
  }

  /** The colour of the progress arc, {@code 0xAARRGGBB}. */
  public void setIndicatorColor(int argb) {
    if (argb != indicatorColor) {
      indicatorColor = argb;
      nativeSetIndicatorColor(argb);
    }
  }

  public int getIndicatorColor() {
    return indicatorColor;
  }

  /** The colour of the track behind the arc, {@code 0xAARRGGBB}. */
  public void setTrackColor(int argb) {
    if (argb != trackColor) {
      trackColor = argb;
      nativeSetTrackColor(argb);
    }
  }

  public int getTrackColor() {
    return trackColor;
  }

  /** Stroke width of both the track and the arc, in pixels. */
  public void setTrackThickness(int px) {
    if (px != trackThickness) {
      trackThickness = px;
      nativeSetTrackThickness(px);
    }
  }

  public int getTrackThickness() {
    return trackThickness;
  }

  /** The ring's diameter in pixels; sets the view's size. */
  public void setIndicatorSize(int px) {
    indicatorSize = px;
    setSize(px, px);
  }

  public int getIndicatorSize() {
    return indicatorSize;
  }

  /** {@link #INDICATOR_DIRECTION_CLOCKWISE} or {@link #INDICATOR_DIRECTION_COUNTERCLOCKWISE}. */
  public void setIndicatorDirection(int direction) {
    if (direction != indicatorDirection) {
      indicatorDirection = direction;
      nativeSetIndicatorDirection(direction);
    }
  }

  public int getIndicatorDirection() {
    return indicatorDirection;
  }

  /**
   * Rounds the ends of the track and the arc. LVGL knows only round or square caps, so any positive
   * radius rounds them fully and 0 squares them; the default is rounded.
   */
  public void setTrackCornerRadius(int radius) {
    if (radius != trackCornerRadius) {
      trackCornerRadius = radius;
      nativeSetTrackCornerRadius(radius);
    }
  }

  public int getTrackCornerRadius() {
    return trackCornerRadius;
  }

  /** Where the track begins, in degrees clockwise from 3 o'clock (picodroid extension). */
  public void setStartAngle(float degrees) {
    if (degrees != startAngle) {
      startAngle = degrees;
      pushAngles();
    }
  }

  public float getStartAngle() {
    return startAngle;
  }

  /**
   * How far the track sweeps clockwise from the start angle, 0..360 (picodroid extension). Values
   * outside that range are clamped; a counter-clockwise fill is {@link
   * #setIndicatorDirection(int)}.
   */
  public void setSweepAngle(float degrees) {
    if (degrees != sweepAngle) {
      sweepAngle = degrees;
      pushAngles();
    }
  }

  public float getSweepAngle() {
    return sweepAngle;
  }

  private void pushAngles() {
    nativeSetAngles(Math.round(startAngle), Math.round(sweepAngle));
  }

  private native void nativeSetIndicatorColor(int argb);

  private native void nativeSetTrackColor(int argb);

  private native void nativeSetTrackThickness(int px);

  private native void nativeSetIndicatorDirection(int direction);

  private native void nativeSetTrackCornerRadius(int radius);

  private native void nativeSetAngles(int startDegrees, int sweepDegrees);
}
