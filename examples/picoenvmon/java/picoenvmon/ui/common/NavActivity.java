// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.common;

import picodroid.app.Activity;
import picodroid.graphics.Theme;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;
import picoenvmon.R;

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

  /**
   * Build the standard vertical root (themed background + padding). As the content view it fills
   * the panel, whichever board this runs on: the app was first written against the Enviro+'s
   * 240x240 and must look the same on a wider or taller one.
   */
  protected LinearLayout makeScreenRoot() {
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    int padH = dimen(R.dimen.screen_pad_h);
    int padV = dimen(R.dimen.screen_pad_v);
    root.setPadding(padH, padV, padH, padV);
    root.setBackgroundColor(Theme.colorBackground);
    // A live monitor is watched, not touched: hold the panel on while a screen shows.
    root.setKeepScreenOn(true);
    return root;
  }

  /** A title line in the primary colour, appended to {@code root}. */
  protected void addTitle(LinearLayout root, int textRes) {
    TextView title = new TextView(this);
    title.setText(getString(textRes));
    title.setTextColor(Theme.colorPrimary);
    root.addView(title);
  }

  /** Append the standardized A/B/X/Y button legend to the bottom of {@code root}. */
  protected void installHintBar(LinearLayout root, int hintsRes) {
    installHintBar(root, hintsRes, 0);
  }

  /** As {@link #installHintBar(LinearLayout, int)}, {@code topMargin} below the block above. */
  protected void installHintBar(LinearLayout root, int hintsRes, int topMargin) {
    LinearLayout.LayoutParams lp = stacked(dimen(R.dimen.hint_bar_height), topMargin);
    root.addView(ButtonHintBar.create(this, getString(hintsRes)), lp);
  }

  /** Layout params for a child of a vertical screen: full width, {@code height}, a top margin. */
  protected static LinearLayout.LayoutParams stacked(int height, int topMargin) {
    LinearLayout.LayoutParams lp =
        new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, height);
    lp.topMargin = topMargin;
    return lp;
  }

  /** A pixel size from {@code res/values/dimens.xml}. */
  protected int dimen(int dimenRes) {
    return getResources().getDimensionPixelSize(dimenRes);
  }
}
