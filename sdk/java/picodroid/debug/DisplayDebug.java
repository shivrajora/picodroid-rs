// SPDX-License-Identifier: GPL-3.0-only
package picodroid.debug;

import picodroid.view.MotionEvent;

/**
 * Picodroid-specific debug helpers that don't have an Android equivalent. Lives outside {@code
 * picodroid.graphics.Display} so the {@link picodroid.graphics.Display} surface stays close to
 * Android's {@code android.view.Display} API.
 *
 * <ul>
 *   <li>{@link #calibrate} — runs the on-screen 4-point touch calibration UI (embedded targets
 *       only).
 *   <li>{@link #showFps} — toggles the live FPS overlay rendered by LVGL.
 *   <li>{@link #pollTouch} — pull-mode raw touch sample, alongside the listener-driven path.
 * </ul>
 */
public final class DisplayDebug {
  private DisplayDebug() {}

  /** Run the interactive 4-point touch calibration. Blocks until the user completes the routine. */
  public static native void calibrate();

  /**
   * Show the live FPS overlay. Call once during {@code Activity.onCreate}; idempotent thereafter.
   */
  public static native void showFps();

  /** Poll one raw touch sample. Returns {@code null} if the touch queue is empty. */
  public static native MotionEvent pollTouch();
}
