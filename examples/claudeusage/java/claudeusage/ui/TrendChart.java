// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.UsageService;
import picodroid.content.Context;
import picodroid.graphics.Canvas;
import picodroid.graphics.Paint;
import picodroid.util.AttributeSet;
import picodroid.view.View;

/**
 * The Burn page's last hour: one bar per sample, newest at the right, coloured by how full the
 * session was. Slots without a sample are short stubs in the track colour. One view drawing its
 * bars, where it used to be one view per bar.
 */
final class TrendChart extends View {
  static final int BAR_WIDTH = 8;
  static final int BAR_PITCH = 11;

  /** The tallest bar, at 100 %. */
  static final int HEIGHT = 50;

  private static final int STUB = 2;
  private static final int MIN_BAR = 3;

  private final Palette palette;
  private final Paint paint = new Paint();

  /** Percent per slot, or -1 for a slot with no sample yet. */
  private final int[] values;

  TrendChart(Context context, AttributeSet attrs) {
    super(context);
    palette = Palette.of(context.getResources());
    values = new int[UsageService.TREND_SLOTS];
    for (int i = 0; i < values.length; i++) {
      values[i] = -1;
    }
  }

  /** As wide as its bars and as tall as the tallest can be. */
  @Override
  protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
    setMeasuredDimension(
        resolveSize(values.length * BAR_PITCH - (BAR_PITCH - BAR_WIDTH), widthMeasureSpec),
        resolveSize(HEIGHT, heightMeasureSpec));
  }

  /** The number of bars. */
  int bars() {
    return values.length;
  }

  /** Sets slot {@code i}; returns whether it changed. Call {@link #invalidate} after a change. */
  boolean set(int i, int value) {
    if (values[i] == value) {
      return false;
    }
    values[i] = value;
    return true;
  }

  @Override
  protected void onDraw(Canvas canvas) {
    for (int i = 0; i < values.length; i++) {
      int value = values[i];
      int h = value < 0 ? STUB : MIN_BAR + value * (HEIGHT - MIN_BAR) / 100;
      int x = i * BAR_PITCH;
      paint.setColor(value < 0 ? palette.track : palette.severity(value));
      float radius = value < 0 ? 1 : 2;
      canvas.drawRoundRect(x, HEIGHT - h, x + BAR_WIDTH, HEIGHT, radius, radius, paint);
    }
  }
}
