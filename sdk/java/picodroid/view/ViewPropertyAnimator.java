// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

import picodroid.util.Log;
import picodroid.view.animation.AccelerateDecelerateInterpolator;
import picodroid.view.animation.AccelerateInterpolator;
import picodroid.view.animation.DecelerateInterpolator;
import picodroid.view.animation.Interpolator;
import picodroid.view.animation.LinearInterpolator;

/**
 * Fluent builder for short interpolated property animations on a single {@link View}, modeled
 * loosely on Android's {@code ViewPropertyAnimator}.
 *
 * <p>v1 caveats:
 *
 * <ul>
 *   <li>Both {@code from} and {@code to} are required (Android infers {@code from} from the View's
 *       current state via reflection; we don't have reflection, and explicit endpoints are
 *       unambiguous).
 *   <li>{@link #setInterpolator} honors the four built-in {@code picodroid.view.animation}
 *       interpolators; a custom one falls back to linear (the native tick can't upcall into Java
 *       per frame).
 * </ul>
 *
 * <p>Idiomatic use:
 *
 * <pre>{@code
 * view.animate().alpha(0f, 1f).x(20, 60).setDuration(250)
 *     .setInterpolator(new DecelerateInterpolator())
 *     .withEndAction(() -> done()).start();
 * }</pre>
 *
 * Multiple property calls in the same chain run concurrently; the slot table holds them as
 * independent (handle, property) entries.
 */
public class ViewPropertyAnimator {
  /** Native property codes — must mirror constants in `lvgl/animations.rs`. */
  static final int PROPERTY_ALPHA = 0;

  static final int PROPERTY_X = 1;
  static final int PROPERTY_Y = 2;

  // Interpolator codes — must mirror constants in `lvgl/animations.rs`.
  static final int INTERP_LINEAR = 0;
  static final int INTERP_ACCELERATE = 1;
  static final int INTERP_DECELERATE = 2;
  static final int INTERP_ACCEL_DECEL = 3;

  private final View view;
  private int durationMs = 300;
  private int interpolatorCode = INTERP_LINEAR;
  private Runnable endAction;

  private boolean hasAlpha = false;
  private float alphaFrom, alphaTo;
  private boolean hasX = false;
  private int xFrom, xTo;
  private boolean hasY = false;
  private int yFrom, yTo;

  ViewPropertyAnimator(View view) {
    this.view = view;
  }

  public ViewPropertyAnimator alpha(float from, float to) {
    hasAlpha = true;
    alphaFrom = from;
    alphaTo = to;
    return this;
  }

  public ViewPropertyAnimator x(int from, int to) {
    hasX = true;
    xFrom = from;
    xTo = to;
    return this;
  }

  public ViewPropertyAnimator y(int from, int to) {
    hasY = true;
    yFrom = from;
    yTo = to;
    return this;
  }

  /** Total duration in milliseconds. Applies to every queued property; default is 300 ms. */
  public ViewPropertyAnimator setDuration(int ms) {
    this.durationMs = ms;
    return this;
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
   * Run {@code action} once every queued property finishes. Mirrors {@code
   * ViewPropertyAnimator#withEndAction}. The action runs on the main loop before the next render
   * tick; a {@link #cancel()} drops it without running, matching Android.
   */
  public ViewPropertyAnimator withEndAction(Runnable action) {
    this.endAction = action;
    return this;
  }

  /** Begin every queued property animation. Subsequent calls require a fresh chain. */
  public void start() {
    int handle = view.nativeHandle;
    if (hasAlpha) {
      nativeStart(
          handle,
          PROPERTY_ALPHA,
          (int) (alphaFrom * 255),
          (int) (alphaTo * 255),
          durationMs,
          interpolatorCode);
    }
    if (hasX) {
      nativeStart(handle, PROPERTY_X, xFrom, xTo, durationMs, interpolatorCode);
    }
    if (hasY) {
      nativeStart(handle, PROPERTY_Y, yFrom, yTo, durationMs, interpolatorCode);
    }
    if (endAction != null) {
      nativeSetEndAction(handle, endAction);
    }
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
      int nativeHandle, int property, int from, int to, int durationMs, int interpolator);

  private static native void nativeSetEndAction(int nativeHandle, Runnable action);

  private static native void nativeCancel(int nativeHandle);
}
