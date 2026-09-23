// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.content.Context;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;

/**
 * A rounded progress bar with a per-instance colour and a vertical gradient, which the SDK's
 * ProgressBar cannot do. The Models page's meters; the Limits page uses {@link RingView}.
 */
final class BarView {
  private final FrameLayout track;
  private final FrameLayout fill;
  private final int width;
  private final int height;

  private int shownPct = -2;
  private int shownColor;
  private boolean shownDim;
  private boolean shownVisible = true;

  BarView(Context ctx, Palette p, ViewGroup parent, int x, int y, int width, int height) {
    this.width = width;
    this.height = height;
    track = Ui.box(ctx, x, y, width, height, p.track, height / 2);
    parent.addView(track);
    fill = Ui.box(ctx, 0, 0, height, height, p.good, height / 2);
    fill.setVisibility(View.INVISIBLE);
    track.addView(fill);
  }

  /** Hide the whole bar, track included: an unused row should be blank, not an empty gauge. */
  void setVisible(boolean visible) {
    if (visible != shownVisible) {
      track.setVisibility(visible ? View.VISIBLE : View.INVISIBLE);
      shownVisible = visible;
    }
  }

  /**
   * @param pct 0..100, or negative for "unknown" (an empty track)
   * @param color the bright end of the fill, {@code deep} the dark end
   */
  void show(int pct, int color, int deep, boolean dim) {
    if (pct != shownPct || color != shownColor) {
      if (pct <= 0) {
        fill.setVisibility(View.INVISIBLE);
      } else {
        int w = width * pct / 100;
        fill.setSize(w < height ? height : w, height);
        if (color != shownColor || shownPct <= 0) {
          fill.setBackground(
              new GradientDrawable()
                  .setCornerRadius(height / 2)
                  .setGradient(color, deep, GradientDrawable.Orientation.TOP_BOTTOM));
        }
        fill.setVisibility(View.VISIBLE);
      }
      shownPct = pct;
      shownColor = color;
    }
    if (dim != shownDim) {
      fill.setAlpha(dim ? Ui.DIM : 1f);
      shownDim = dim;
    }
  }
}
