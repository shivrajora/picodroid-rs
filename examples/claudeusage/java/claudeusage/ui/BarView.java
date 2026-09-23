// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.content.Context;
import picodroid.content.res.ColorStateList;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;
import picodroid.widget.ProgressBar;

/**
 * A rounded progress bar with a per-instance severity colour, and an optional pace marker: a thin
 * tick at "how far through the window we are". Fill past the tick means the limit is being used
 * faster than the window replenishes it.
 */
final class BarView {
  /** {@link Ui#DIM} as a tint alpha: a stale bar keeps its track, only the fill fades. */
  private static final int DIM_ALPHA = (int) (Ui.DIM * 255);

  private final ProgressBar bar;
  private final FrameLayout marker;
  private final int x;
  private final int y;
  private final int width;

  private int shownColor;
  private int shownMarker = -2;
  private boolean shownDim;
  private boolean shownVisible = true;

  BarView(
      Context ctx,
      Palette p,
      ViewGroup parent,
      int x,
      int y,
      int width,
      int height,
      boolean withMarker) {
    this.x = x;
    this.y = y;
    this.width = width;
    bar = new ProgressBar(ctx);
    bar.setSize(width, height);
    bar.setPosition(x, y);
    bar.setProgressBackgroundTintList(ColorStateList.valueOf(p.track));
    parent.addView(bar);
    if (withMarker) {
      marker = Ui.box(ctx, x, y - 3, 2, height + 6, p.text, 1);
      marker.setVisibility(View.INVISIBLE);
      parent.addView(marker);
    } else {
      marker = null;
    }
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
   * @param markerPct 0..100 for the pace tick, negative to hide it
   */
  void show(int pct, int color, int markerPct, boolean dim) {
    // show() runs every second: the widget skips an unchanged value itself; the tint is diffed here
    // because every style set redraws the bar.
    bar.setProgress(pct < 0 ? 0 : pct, true);
    if (color != shownColor || dim != shownDim) {
      bar.setProgressTintList(ColorStateList.valueOf(color).withAlpha(dim ? DIM_ALPHA : 0xFF));
      shownColor = color;
      shownDim = dim;
    }
    if (marker != null && markerPct != shownMarker) {
      if (markerPct < 0) {
        marker.setVisibility(View.INVISIBLE);
      } else {
        int mx = x + width * (markerPct > 100 ? 100 : markerPct) / 100 - 1;
        marker.setPosition(mx < x ? x : mx, y - 3);
        marker.setVisibility(View.VISIBLE);
      }
      shownMarker = markerPct;
    }
  }
}
