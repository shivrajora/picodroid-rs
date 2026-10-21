// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

/**
 * {@link ComponentCallbacks} plus the graded memory signal, mirroring {@code
 * android.content.ComponentCallbacks2}. The levels are Android's values; on picodroid the one that
 * arrives is {@link #TRIM_MEMORY_RUNNING_CRITICAL}: the LVGL pool is below an eighth free or the
 * native heap below a sixteenth. It is delivered once per episode from the event loop and, at an
 * Activity launch, before the framework destroys a covered Activity to make room — a view cache
 * dropped then (its views go back to the pool) can save that Activity. The other levels never
 * arrive: one app runs at a time, so its UI is never hidden or in the background. Android 14
 * deprecated every level but {@link #TRIM_MEMORY_UI_HIDDEN} and {@link #TRIM_MEMORY_BACKGROUND};
 * the constants stay so ported code compiles.
 */
public interface ComponentCallbacks2 extends ComponentCallbacks {
  /** The process is at the end of the background list; about to be killed. Never sent. */
  int TRIM_MEMORY_COMPLETE = 80;

  /** The process is in the middle of the background list. Never sent. */
  int TRIM_MEMORY_MODERATE = 60;

  /** The process has gone to the background list. Never sent. */
  int TRIM_MEMORY_BACKGROUND = 40;

  /** The UI is no longer visible. Never sent. */
  int TRIM_MEMORY_UI_HIDDEN = 20;

  /** Memory is critically short while the app runs: the level picodroid delivers. */
  int TRIM_MEMORY_RUNNING_CRITICAL = 15;

  /** Memory is low while the app runs. Never sent (the critical level is the one signal). */
  int TRIM_MEMORY_RUNNING_LOW = 10;

  /** Memory is moderately short while the app runs. Never sent. */
  int TRIM_MEMORY_RUNNING_MODERATE = 5;

  /**
   * Release memory in proportion to {@code level}, one of the {@code TRIM_MEMORY_*} constants. Runs
   * on the main thread; return quickly.
   */
  void onTrimMemory(int level);
}
