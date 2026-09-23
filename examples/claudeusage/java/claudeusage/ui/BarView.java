// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.content.Context;
import picodroid.content.res.ColorStateList;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.ProgressBar;

/**
 * A rounded progress bar with a per-instance severity colour: the Models page's meters. The Limits
 * page draws {@link RingView}s.
 */
final class BarView {
  /** {@link Ui#DIM} as a tint alpha: a stale bar keeps its track, only the fill fades. */
  private static final int DIM_ALPHA = (int) (Ui.DIM * 255);

  private final ProgressBar bar;

  private int shownColor;
  private boolean shownDim;
  private boolean shownVisible = true;

  BarView(Context ctx, Palette p, ViewGroup parent, int x, int y, int width, int height) {
    bar = new ProgressBar(ctx);
    bar.setSize(width, height);
    bar.setPosition(x, y);
    bar.setProgressBackgroundTintList(ColorStateList.valueOf(p.track));
    parent.addView(bar);
  }

  /** Hide the whole bar, track included: an unused row should be blank, not an empty gauge. */
  void setVisible(boolean visible) {
    if (visible != shownVisible) {
      bar.setVisibility(visible ? View.VISIBLE : View.INVISIBLE);
      shownVisible = visible;
    }
  }

  /**
   * @param pct 0..100, or negative for "unknown" (an empty track)
   * @param color the fill colour
   */
  void show(int pct, int color, boolean dim) {
    // show() runs every second: the widget skips an unchanged value itself; the tint is diffed here
    // because every style set redraws the bar.
    bar.setProgress(pct < 0 ? 0 : pct, true);
    if (color != shownColor || dim != shownDim) {
      bar.setProgressTintList(ColorStateList.valueOf(color).withAlpha(dim ? DIM_ALPHA : 0xFF));
      shownColor = color;
      shownDim = dim;
    }
  }
}
