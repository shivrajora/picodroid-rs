// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

import picodroid.util.Log;
import picodroid.view.animation.AccelerateDecelerateInterpolator;
import picodroid.view.animation.AccelerateInterpolator;
import picodroid.view.animation.DecelerateInterpolator;
import picodroid.view.animation.Interpolator;
import picodroid.view.animation.LinearInterpolator;

/**
 * Fluent builder for short interpolated property animations on a single {@link View}. Mirrors
 * {@code android.view.ViewPropertyAnimator}: every property method takes only the target value —
 * the animation starts from the view's current value, read from the renderer when {@link #start()}
 * runs (or, for a {@link #setStartDelay delayed} animation, when the delay expires).
 *
 * <pre>{@code
 * view.animate().alpha(0f).translationX(40f).setDuration(250)
 *     .setInterpolator(new DecelerateInterpolator())
 *     .withEndAction(() -> done()).start();
 * }</pre>
 *
 * <p>Divergences from Android, all forced by the renderer:
 *
 * <ul>
 *   <li>{@link #start()} is explicit — Android starts on the next frame automatically; here nothing
 *       runs until {@code start()} is called.
 *   <li>Starting a property that is already animating on the same view replaces the running
 *       animation for that property (Android cancels it too). A {@link #setStartDelay delayed}
 *       start coexists with a running one until its delay expires, then takes over from wherever
 *       the running animation has got to — so {@code alpha(0.35f)} followed by {@code
 *       alpha(1f).setStartDelay(180)} is a pulse. Chaining through {@link #withEndAction} is the
 *       clearer idiom.
 *   <li>One end action per view: a second {@code withEndAction} registered before the first fires
 *       replaces it.
 *   <li>{@link #setInterpolator} honors the four built-in {@code picodroid.view.animation}
 *       interpolators; a custom one falls back to linear (the native tick can't upcall into Java
 *       per frame).
 *   <li>{@link #x}/{@link #y} move the view's layout position and do nothing inside a {@link
 *       picodroid.widget.LinearLayout}, which positions its children itself; {@link #translationX}/
 *       {@link #translationY} offset the view from wherever layout put it and work everywhere.
 *   <li>{@link #rotation}, {@link #scaleX} and {@link #scaleY} render the view through an
 *       off-screen ARGB layer of the view's own size, taken from LVGL's pool (64 KB by default): a
 *       60×30 tile costs ~7 KB, a full 240×240 screen would need 225 KB and is skipped with an
 *       "Allocating layer buffer failed" log. Keep transformed views small.
 *   <li>Up to 16 property animations run concurrently across all views; starts beyond that are
 *       dropped.
 * </ul>
 */
public class ViewPropertyAnimator {
  // Property codes — must mirror `PROPERTY_*` in `graphics/lvgl/animations.rs`. Shared with
  // View's transform accessors (nativeSetProperty / nativeGetProperty).
  static final int PROPERTY_ALPHA = 0;
  static final int PROPERTY_X = 1;
  static final int PROPERTY_Y = 2;
  static final int PROPERTY_TRANSLATION_X = 3;
  static final int PROPERTY_TRANSLATION_Y = 4;
  static final int PROPERTY_ROTATION = 5;
  static final int PROPERTY_SCALE_X = 6;
  static final int PROPERTY_SCALE_Y = 7;

  // Interpolator codes — must mirror constants in `lvgl/animations.rs`.
  static final int INTERP_LINEAR = 0;
  static final int INTERP_ACCELERATE = 1;
  static final int INTERP_DECELERATE = 2;
  static final int INTERP_ACCEL_DECEL = 3;

  private final View view;
  private long durationMs = 300;
  private long startDelayMs = 0;
  private int interpolatorCode = INTERP_LINEAR;
  private Runnable endAction;

  /** Bit {@code PROPERTY_*} set ⇒ that property is queued with the matching {@code *To} target. */
  private int queued;

  private float alphaTo;
  private float xTo;
  private float yTo;
  private float translationXTo;
  private float translationYTo;
  private float rotationTo;
  private float scaleXTo;
  private float scaleYTo;

  ViewPropertyAnimator(View view) {
    this.view = view;
  }

  /** Animate opacity to {@code value} (0 = transparent, 1 = opaque). */
  public ViewPropertyAnimator alpha(float value) {
    alphaTo = value;
    return queue(PROPERTY_ALPHA);
  }

  /** Animate the layout x position to {@code value} pixels. See the class note on {@code x}. */
  public ViewPropertyAnimator x(float value) {
    xTo = value;
    return queue(PROPERTY_X);
  }

  /** Animate the layout y position to {@code value} pixels. */
  public ViewPropertyAnimator y(float value) {
    yTo = value;
    return queue(PROPERTY_Y);
  }

  /** Animate {@link View#setTranslationX} to {@code value} pixels. */
  public ViewPropertyAnimator translationX(float value) {
    translationXTo = value;
    return queue(PROPERTY_TRANSLATION_X);
  }

  /** Animate {@link View#setTranslationY} to {@code value} pixels. */
  public ViewPropertyAnimator translationY(float value) {
    translationYTo = value;
    return queue(PROPERTY_TRANSLATION_Y);
  }

  /** Animate {@link View#setRotation} to {@code degrees} (clockwise, about the centre). */
  public ViewPropertyAnimator rotation(float degrees) {
    rotationTo = degrees;
    return queue(PROPERTY_ROTATION);
  }

  /** Animate {@link View#setScaleX} to {@code value} (1.0 = unscaled). */
  public ViewPropertyAnimator scaleX(float value) {
    scaleXTo = value;
    return queue(PROPERTY_SCALE_X);
  }

  /** Animate {@link View#setScaleY} to {@code value} (1.0 = unscaled). */
  public ViewPropertyAnimator scaleY(float value) {
    scaleYTo = value;
    return queue(PROPERTY_SCALE_Y);
  }

  private ViewPropertyAnimator queue(int property) {
    queued |= 1 << property;
    return this;
  }

  private boolean isQueued(int property) {
    return (queued & (1 << property)) != 0;
  }

  /** Total duration in milliseconds. Applies to every queued property; default is 300 ms. */
  public ViewPropertyAnimator setDuration(long ms) {
    this.durationMs = ms;
    return this;
  }

  /** Returns the duration set via {@link #setDuration}. Mirrors Android. */
  public long getDuration() {
    return durationMs;
  }

  /**
   * Delay before the queued properties start animating, in milliseconds (default 0). The view keeps
   * its current value during the delay; the animation then starts from whatever value the view has
   * at that moment. Mirrors {@code ViewPropertyAnimator#setStartDelay}.
   */
  public ViewPropertyAnimator setStartDelay(long ms) {
    this.startDelayMs = ms;
    return this;
  }

  /** Returns the delay set via {@link #setStartDelay}. Mirrors Android. */
  public long getStartDelay() {
    return startDelayMs;
  }

  /**
   * Set the easing curve. Mirrors {@code ViewPropertyAnimator#setInterpolator}. Only the four
   * built-in {@code picodroid.view.animation} interpolators map to a native easing code; any other
   * implementation falls back to linear with a logged warning, since the native per-frame tick
   * cannot call back into a Java interpolator.
   */
  public ViewPropertyAnimator setInterpolator(Interpolator interpolator) {
    if (interpolator == null || interpolator instanceof LinearInterpolator) {
      interpolatorCode = INTERP_LINEAR;
    } else if (interpolator instanceof AccelerateInterpolator) {
      interpolatorCode = INTERP_ACCELERATE;
    } else if (interpolator instanceof DecelerateInterpolator) {
      interpolatorCode = INTERP_DECELERATE;
    } else if (interpolator instanceof AccelerateDecelerateInterpolator) {
      interpolatorCode = INTERP_ACCEL_DECEL;
    } else {
      interpolatorCode = INTERP_LINEAR;
      Log.w("ViewPropertyAnimator", "custom Interpolator unsupported natively — using linear");
    }
    return this;
  }

  /**
   * Run {@code action} once every queued property finishes (a delayed leg included). Mirrors {@code
   * ViewPropertyAnimator#withEndAction}. The action runs on the main loop before the next render
   * tick; a {@link #cancel()} drops it without running, matching Android.
   */
  public ViewPropertyAnimator withEndAction(Runnable action) {
    this.endAction = action;
    return this;
  }

  /**
   * Begin every queued property animation. The implicit start value of each property is read from
   * the renderer now (or when the {@link #setStartDelay delay} expires). Subsequent calls require a
   * fresh chain.
   */
  public void start() {
    int handle = view.nativeHandle;
    int duration = clampMs(durationMs);
    int delay = clampMs(startDelayMs);
    if (isQueued(PROPERTY_ALPHA)) {
      nativeStart(handle, PROPERTY_ALPHA, alphaTo, duration, delay, interpolatorCode);
      view.alpha = alphaTo; // getAlpha() reports the target once started — see View.getAlpha
    }
    if (isQueued(PROPERTY_X)) {
      nativeStart(handle, PROPERTY_X, xTo, duration, delay, interpolatorCode);
    }
    if (isQueued(PROPERTY_Y)) {
      nativeStart(handle, PROPERTY_Y, yTo, duration, delay, interpolatorCode);
    }
    if (isQueued(PROPERTY_TRANSLATION_X)) {
      nativeStart(
          handle, PROPERTY_TRANSLATION_X, translationXTo, duration, delay, interpolatorCode);
    }
    if (isQueued(PROPERTY_TRANSLATION_Y)) {
      nativeStart(
          handle, PROPERTY_TRANSLATION_Y, translationYTo, duration, delay, interpolatorCode);
    }
    if (isQueued(PROPERTY_ROTATION)) {
      nativeStart(handle, PROPERTY_ROTATION, rotationTo, duration, delay, interpolatorCode);
    }
    if (isQueued(PROPERTY_SCALE_X)) {
      nativeStart(handle, PROPERTY_SCALE_X, scaleXTo, duration, delay, interpolatorCode);
    }
    if (isQueued(PROPERTY_SCALE_Y)) {
      nativeStart(handle, PROPERTY_SCALE_Y, scaleYTo, duration, delay, interpolatorCode);
    }
    if (endAction != null) {
      nativeSetEndAction(handle, endAction);
    }
  }

  /** Android's durations are {@code long}; the native slot table counts in {@code int} millis. */
  private static int clampMs(long ms) {
    if (ms < 0) {
      return 0;
    }
    return ms > Integer.MAX_VALUE ? Integer.MAX_VALUE : (int) ms;
  }

  /**
   * Cancel every property animation targeting this view. The view's properties are *not* reset to
   * their start values — they stay at whatever the last interpolation frame left them, matching
   * Android.
   */
  public void cancel() {
    nativeCancel(view.nativeHandle);
  }

  private static native void nativeStart(
      int nativeHandle, int property, float to, int durationMs, int startDelayMs, int interpolator);

  private static native void nativeSetEndAction(int nativeHandle, Runnable action);

  private static native void nativeCancel(int nativeHandle);
}
