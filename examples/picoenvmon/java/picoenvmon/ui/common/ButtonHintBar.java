// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.common;

import picodroid.content.Context;
import picodroid.graphics.Theme;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Always-visible on-screen legend mapping the four hardware buttons (A/B/X/Y) to their current
 * actions, so users of this touchless 4-button device always know what each button does. Built once
 * per screen by {@link NavActivity#installHintBar}, replacing the old hand-written, drift-prone
 * footer strings.
 *
 * <p>Implemented as a factory rather than a {@code View} subclass to keep the bar a plain composed
 * {@link LinearLayout} (a label row), matching how the rest of the app builds UI. Its height is the
 * caller's layout params.
 */
public final class ButtonHintBar {

  private ButtonHintBar() {}

  /** Build a single-line legend row, e.g. {@code "A:Up B:Down X:Open Y:Back"}. */
  public static LinearLayout create(Context context, String hints) {
    LinearLayout bar = new LinearLayout(context);
    bar.setOrientation(LinearLayout.HORIZONTAL);

    TextView label = new TextView(context);
    label.setText(hints);
    label.setTextColor(Theme.colorTextSecondary);
    bar.addView(label);

    return bar;
  }
}
