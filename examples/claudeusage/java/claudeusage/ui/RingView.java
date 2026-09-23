// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.content.Context;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.CircularProgressIndicator;

/**
 * A three-quarter ring gauge with a per-instance colour and an optional pace tick: a thin radial
 * mark at "how far through the window we are". Fill past the tick means the limit is being used
 * faster than the window replenishes it. Two {@link CircularProgressIndicator}s: the gauge, and an
 * overlay with a transparent track whose four-degree indicator is the tick.
 */
final class RingView {
  /** The dial opens at the bottom: 270 degrees from 7:30 round to 4:30. */
  static final float START = 135f;

  static final float SWEEP = 270f;

  private static final float TICK_SWEEP = 4f;

  /** The tick stands proud of the ring on both sides, like the bar marker did. */
  private static final int TICK_OVERHANG = 2;

  private final CircularProgressIndicator ring;
  private final CircularProgressIndicator tick;

  private int shownPct = -2;
  private int shownColor;
  private int shownMarker = -2;
  private boolean shownDim;

  RingView(
      Context ctx,
      Palette p,
      ViewGroup parent,
      int x,
      int y,
      int diameter,
      int stroke,
      boolean withMarker) {
    ring = new CircularProgressIndicator(ctx);
    ring.setIndicatorSize(diameter);
    ring.setPosition(x, y);
    ring.setTrackThickness(stroke);
    ring.setTrackColor(p.track);
    ring.setIndicatorColor(p.good);
    ring.setStartAngle(START);
    ring.setSweepAngle(SWEEP);
    parent.addView(ring);
    shownColor = p.good;
    if (withMarker) {
      tick = new CircularProgressIndicator(ctx);
      tick.setIndicatorSize(diameter + 2 * TICK_OVERHANG);
      tick.setPosition(x - TICK_OVERHANG, y - TICK_OVERHANG);
      tick.setTrackThickness(stroke + 2 * TICK_OVERHANG);
      tick.setTrackColor(Ui.TRANSPARENT);
      // Square ends make the tick's edges radial, so it reads as a mark rather than a dot.
      tick.setTrackCornerRadius(0);
      tick.setIndicatorColor(p.text);
      tick.setSweepAngle(TICK_SWEEP);
      tick.setProgress(100);
      tick.setVisibility(View.INVISIBLE);
      parent.addView(tick);
    } else {
      tick = null;
    }
  }

  /**
   * @param pct 0..100, or negative for "unknown" (an empty track)
   * @param color the fill colour
   * @param markerPct 0..100 for the pace tick, negative to hide it
   */
  void show(int pct, int color, int markerPct, boolean dim) {
    if (pct != shownPct || color != shownColor) {
      ring.setIndicatorColor(color);
      ring.setProgress(pct < 0 ? 0 : pct);
      shownPct = pct;
      shownColor = color;
    }
    if (tick != null && markerPct != shownMarker) {
      if (markerPct < 0) {
        tick.setVisibility(View.INVISIBLE);
      } else {
        int clamped = markerPct > 100 ? 100 : markerPct;
        // Centre the tick on the elapsed angle; the widget wraps past 360.
        float start = START + SWEEP * clamped / 100f - TICK_SWEEP / 2f;
        if (start >= 360f) {
          start -= 360f;
        }
        tick.setStartAngle(start);
        tick.setVisibility(View.VISIBLE);
      }
      shownMarker = markerPct;
    }
    if (dim != shownDim) {
      ring.setAlpha(dim ? Ui.DIM : 1f);
      shownDim = dim;
    }
  }
}
