// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

public interface OnTouchListener {
  /**
   * Called when a touch event lands on the registered View. Return {@code true} to indicate the
   * event was consumed — currently advisory only (the framework doesn't propagate to a parent View
   * in v1, since per-widget LVGL hit testing already picks the deepest target). Returning false is
   * the no-op default.
   */
  boolean onTouch(View v, MotionEvent event);
}
