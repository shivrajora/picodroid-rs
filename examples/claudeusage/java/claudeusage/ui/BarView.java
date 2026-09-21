// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.graphics.drawable.GradientDrawable;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;

/**
 * A rounded progress bar with a per-instance colour, which the SDK's ProgressBar cannot do, and an
 * optional pace marker: a thin tick at "how far through the window we are". Fill past the tick
 * means the limit is being used faster than the window replenishes it.
 */
final class BarView {
  private final FrameLayout track;
  private final FrameLayout fill;
  private final FrameLayout marker;
  private final int x;
  private final int y;
  private final int width;
  private final int height;

  private int shownPct = -2;
  private int shownColor;
  private int shownMarker = -2;
  private boolean shownDim;
  private boolean shownVisible = true;

  BarView(ViewGroup parent, int x, int y, int width, int height, boolean withMarker) {
    this.x = x;
    this.y = y;
    this.width = width;
    this.height = height;
    track = Ui.box(x, y, width, height, Palette.TRACK, height / 2);
    parent.addView(track);
    fill = Ui.box(0, 0, height, height, Palette.GOOD, height / 2);
    fill.setVisibility(View.INVISIBLE);
    track.addView(fill);
    if (withMarker) {
      marker = Ui.box(x, y - 3, 2, height + 6, Palette.TEXT, 1);
      marker.setVisibility(View.INVISIBLE);
      parent.addView(marker);
    } else {
      marker = null;
    }
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
   * @param markerPct 0..100 for the pace tick, negative to hide it
   */
  void show(int pct, int color, int deep, int markerPct, boolean dim) {
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
    if (dim != shownDim) {
      fill.setAlpha(dim ? Ui.DIM : 1f);
      shownDim = dim;
    }
  }
}
