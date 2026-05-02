// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Fluent builder for short interpolated property animations on a single {@link View}, modeled
 * loosely on Android's {@code ViewPropertyAnimator}.
 *
 * <p>v1 caveats:
 *
 * <ul>
 *   <li>Linear interpolation only. Easing curves (ease-in-out, etc.) are a planned follow-up.
 *   <li>No completion listener. Apps that need "do X when this finishes" should fire whatever they
 *       need synchronously after {@link #start} and let the animation play in the background.
 *   <li>Both {@code from} and {@code to} are required (Android infers {@code from} from the View's
 *       current state via reflection; we don't have reflection, and explicit endpoints are
 *       unambiguous).
 * </ul>
 *
 * <p>Idiomatic use:
 *
 * <pre>{@code
 * view.animate().alpha(0f, 1f).x(20, 60).setDuration(250).start();
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

  private final View view;
  private int durationMs = 300;

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

  /** Begin every queued property animation. Subsequent calls require a fresh chain. */
  public void start() {
    int handle = view.nativeHandle;
    if (hasAlpha) {
      nativeStart(
          handle, PROPERTY_ALPHA, (int) (alphaFrom * 255), (int) (alphaTo * 255), durationMs);
    }
    if (hasX) {
      nativeStart(handle, PROPERTY_X, xFrom, xTo, durationMs);
    }
    if (hasY) {
      nativeStart(handle, PROPERTY_Y, yFrom, yTo, durationMs);
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
      int nativeHandle, int property, int from, int to, int durationMs);

  private static native void nativeCancel(int nativeHandle);
}
