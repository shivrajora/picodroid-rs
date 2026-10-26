// SPDX-License-Identifier: GPL-3.0-only
package weather;

import picodroid.content.Context;
import picodroid.graphics.Canvas;
import picodroid.graphics.Color;
import picodroid.graphics.Paint;
import picodroid.view.View;

/**
 * The next hours as columns: the hour, its glyph, its temperature. One view drawing every column,
 * as many as fit its width at {@link #COLUMN} pixels each.
 */
final class HourlyStrip extends View {
  static final int HEIGHT = 58;
  private static final int COLUMN = 36;
  private static final int ICON = 22;
  private static final int TEXT = Color.rgb(255, 255, 255);
  private static final int TEXT_DIM = Color.rgb(214, 222, 236);

  private final Paint paint = new Paint();
  private final String now;
  private Forecast forecast;

  HourlyStrip(Context ctx) {
    super(ctx);
    now = ctx.getString(R.string.now);
  }

  /** {@link #HEIGHT} tall; as wide as the parent makes it. */
  @Override
  protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
    setMeasuredDimension(
        getDefaultSize(getSuggestedMinimumWidth(), widthMeasureSpec),
        resolveSize(HEIGHT, heightMeasureSpec));
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
    int columns = Math.max(1, Math.min(Forecast.HOURS, getWidth() / COLUMN));
    int n = Math.min(columns, f.hours);
    float colW = (float) getWidth() / n;
    paint.setTextSize(14);
    paint.setTextAlign(Paint.Align.CENTER);
    float top = -paint.ascent();
    float bottom = HEIGHT - paint.descent();
    for (int i = 0; i < n; i++) {
      float cx = colW * i + colW / 2;
      paint.setStyle(Paint.Style.FILL);
      paint.setColor(TEXT_DIM);
      canvas.drawText(i == 0 ? now : String.format("%02d", f.hourOfDay[i]), cx, top, paint);
      WeatherIcons.draw(canvas, paint, f.hourCode[i], f.hourIsDay[i], cx, HEIGHT / 2f, ICON);
      paint.setStyle(Paint.Style.FILL);
      paint.setColor(TEXT);
      paint.setTextAlign(Paint.Align.CENTER);
      canvas.drawText(
          getContext().getString(R.string.temperature, f.hourTemp[i]), cx, bottom, paint);
    }
  }
}
