// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Mirrors {@code android.view.View.OnKeyListener}: a hardware key, delivered to the focused view
 * before the Activity's {@code onKeyDown} / {@code onKeyUp}. {@code keyCode} is {@link
 * KeyEvent#getKeyCode()} as on Android, for the usual {@code switch (keyCode)}.
 */
public interface OnKeyListener {
  boolean onKey(View v, int keyCode, KeyEvent event);
}
