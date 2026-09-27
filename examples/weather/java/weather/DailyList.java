// SPDX-License-Identifier: GPL-3.0-only
package weather;

import picodroid.content.Context;
import picodroid.graphics.Canvas;
import picodroid.graphics.Color;
import picodroid.graphics.Paint;
import picodroid.view.View;

/**
 * The week, one row per day: the day, its glyph, the low, a bar spanning the day's range across the
 * week's, the high. One view drawing every row: three text ops, the bar and a glyph of at most four
 * ops per row, which is what a view's 2 KB recording holds for seven rows (a track under the bar
 * would be the op too many).
 */
final class DailyList extends View {
  static final int ROW = 20;
  private static final int ICON = 18;
  private static final int ICON_X = 58;
  private static final int LOW_RIGHT = 100;
  private static final int BAR_LEFT = 108;
  private static final int BAR_RIGHT_INSET = 34;
  private static final int TEXT = Color.rgb(255, 255, 255);
  private static final int TEXT_DIM = Color.rgb(214, 222, 236);
  private static final int COLD = Color.rgb(120, 190, 255);
  private static final int MILD = Color.rgb(255, 214, 96);
  private static final int HOT = Color.rgb(255, 140, 80);

  private final Paint paint = new Paint();
  private final int width;
  private Forecast forecast;

  DailyList(Context ctx, int width) {
    super(ctx);
    this.width = width;
    setSize(width, Forecast.DAYS * ROW);
  }

  void set(Forecast f) {
    forecast = f;
    invalidate();
  }

  @Override
  protected void onDraw(Canvas canvas) {
    Forecast f = forecast;
    if (f == null) {
      return;
    }
    int weekLow = f.dayLow[0];
    int weekHigh = f.dayHigh[0];
    for (int i = 1; i < f.days; i++) {
      weekLow = Math.min(weekLow, f.dayLow[i]);
      weekHigh = Math.max(weekHigh, f.dayHigh[i]);
    }
    int span = Math.max(1, weekHigh - weekLow);
    int barLeft = BAR_LEFT;
    int barRight = width - BAR_RIGHT_INSET;
    float barWidth = barRight - barLeft;
    paint.setTextSize(14);
    float baselineOffset = -(paint.ascent() + paint.descent()) / 2;
    for (int i = 0; i < f.days; i++) {
      float cy = i * ROW + ROW / 2f;
      float baseline = cy + baselineOffset;
      paint.setStyle(Paint.Style.FILL);
      paint.setColor(TEXT);
      paint.setTextAlign(Paint.Align.LEFT);
      canvas.drawText(f.dayName[i], 0, baseline, paint);
      WeatherIcons.draw(canvas, paint, f.dayCode[i], true, ICON_X, cy, ICON);
      paint.setStyle(Paint.Style.FILL);
      paint.setColor(TEXT_DIM);
      paint.setTextAlign(Paint.Align.RIGHT);
      canvas.drawText(f.dayLow[i] + "°", LOW_RIGHT, baseline, paint);
      float x0 = barLeft + (f.dayLow[i] - weekLow) * barWidth / span;
      float x1 = barLeft + (f.dayHigh[i] - weekLow) * barWidth / span;
      if (x1 < x0 + 4) {
        x1 = x0 + 4;
      }
      paint.setColor(rangeColor((f.dayLow[i] + f.dayHigh[i]) / 2));
      canvas.drawRoundRect(x0, cy - 2, x1, cy + 2, 2, 2, paint);
      paint.setColor(TEXT);
      canvas.drawText(f.dayHigh[i] + "°", width, baseline, paint);
    }
  }

  /** Cold below 10, hot from 25: the bar's colour for a day's mean. In Fahrenheit, scaled. */
  private static int rangeColor(int mean) {
    int celsius = MainActivity.FAHRENHEIT ? (mean - 32) * 5 / 9 : mean;
    if (celsius < 10) {
      return COLD;
    }
    return celsius < 25 ? MILD : HOT;
  }
}
