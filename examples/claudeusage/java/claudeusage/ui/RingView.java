// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.content.Context;
import picodroid.graphics.Canvas;
import picodroid.graphics.Paint;
import picodroid.util.AttributeSet;
import picodroid.view.View;

/**
 * A three-quarter ring gauge with a per-instance colour and a pace tick: a thin radial mark at "how
 * far through the window we are". Fill past the tick means the limit is being used faster than the
 * window replenishes it. One view drawing the track, the fill and the tick.
 */
final class RingView extends View {
  /** The dial opens at the bottom: 270 degrees from 7:30 round to 4:30. */
  private static final float START = 135f;

  private static final float SWEEP = 270f;

  private static final float TICK_SWEEP = 4f;

  /** The ring's width, and its diameter when the layout leaves the size to the view. */
  private static final int STROKE = 10;

  private static final int DIAMETER = 104;

  /**
   * The tick stands proud of the ring on both sides; the view is this much larger than the ring all
   * round to hold it.
   */
  private static final int TICK_OVERHANG = 2;

  private final Palette palette;
  private final Paint paint = new Paint();

  private int pct = -1;
  private int color;
  private int marker = -1;
  private boolean dim;

  RingView(Context context, AttributeSet attrs) {
    super(context);
    palette = Palette.of(context.getResources());
    color = palette.good;
    paint.setStyle(Paint.Style.STROKE);
  }

  @Override
  protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
    int size = DIAMETER + 2 * TICK_OVERHANG;
    setMeasuredDimension(resolveSize(size, widthMeasureSpec), resolveSize(size, heightMeasureSpec));
  }

  /**
   * @param pct 0..100, or negative for "unknown" (an empty track)
   * @param color the fill colour
   * @param markerPct 0..100 for the pace tick, negative to hide it
   * @param dim whether the ring is faded (stale numbers); the tick never is
   */
  void show(int pct, int color, int markerPct, boolean dim) {
    int marker = markerPct > 100 ? 100 : markerPct;
    if (pct == this.pct && color == this.color && marker == this.marker && dim == this.dim) {
      return;
    }
    this.pct = pct;
    this.color = color;
    this.marker = marker;
    this.dim = dim;
    invalidate();
  }

  @Override
  protected void onDraw(Canvas canvas) {
    // A stroke straddles its circle, so the circle sits half a stroke inside the ring's edge.
    float near = TICK_OVERHANG + STROKE / 2f;
    float far = getWidth() - near;
    paint.setStrokeWidth(STROKE);
    paint.setStrokeCap(Paint.Cap.ROUND);
    paint.setColor(palette.track);
    if (dim) {
      paint.setAlpha(Ui.DIM_ALPHA);
    }
    canvas.drawArc(near, near, far, far, START, SWEEP, false, paint);
    if (pct > 0) {
      paint.setColor(color);
      if (dim) {
        paint.setAlpha(Ui.DIM_ALPHA);
      }
      canvas.drawArc(near, near, far, far, START, SWEEP * pct / 100f, false, paint);
    }
    if (marker >= 0) {
      // Square ends make the tick's edges radial, so it reads as a mark rather than a dot.
      paint.setStrokeWidth(STROKE + 2 * TICK_OVERHANG);
      paint.setStrokeCap(Paint.Cap.BUTT);
      paint.setColor(palette.text);
      float at = START + SWEEP * marker / 100f - TICK_SWEEP / 2f;
      canvas.drawArc(near, near, far, far, at, TICK_SWEEP, false, paint);
    }
  }
}
