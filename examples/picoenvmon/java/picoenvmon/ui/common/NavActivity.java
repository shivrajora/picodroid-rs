// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.common;

import picodroid.app.Activity;
import picodroid.graphics.Theme;
import picodroid.widget.LinearLayout;

/**
 * Base for every picoenvmon screen under the standardized 4-button navigation model:
 *
 * <ul>
 *   <li><b>A</b> = up (previous focusable)
 *   <li><b>B</b> = down (next focusable)
 *   <li><b>X</b> = open / activate the focused item
 *   <li><b>Y</b> = back
 * </ul>
 *
 * Navigation itself is handled natively by the per-Activity keypad focus group — PREV/NEXT move the
 * focus highlight between focusable widgets, ENTER clicks the focused widget, and ESC runs the back
 * chain (dismiss keyboard/dialog, then {@link Activity#onBackPressed()}). So this base does not
 * implement an {@code OnKeyListener}; it only standardizes the screen frame and the always-visible
 * {@link ButtonHintBar} legend, removing the per-screen focus/key boilerplate every screen used to
 * repeat. Subclasses just build their content with focusable widgets (Buttons, ListView rows,
 * EditTexts, Switches) and the four buttons behave identically everywhere.
 */
public abstract class NavActivity extends Activity {

  /** Build the standard full-screen vertical root (themed background + padding). */
  protected LinearLayout makeScreenRoot() {
    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(8, 6, 8, 6);
    root.setBackgroundColor(Theme.colorBackground);
    return root;
  }

  /** Append the standardized A/B/X/Y button legend to the bottom of {@code root}. */
  protected void installHintBar(LinearLayout root, String hints) {
    root.addView(ButtonHintBar.create(hints));
  }
}
