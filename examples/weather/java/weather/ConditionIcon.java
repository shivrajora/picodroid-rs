// SPDX-License-Identifier: GPL-3.0-only
package weather;

import picodroid.content.Context;
import picodroid.graphics.Canvas;
import picodroid.graphics.Paint;
import picodroid.view.View;

/** The hero glyph beside the temperature: one condition, drawn at the view's size. */
final class ConditionIcon extends View {
  private final Paint paint = new Paint();
  private final int size;
  private int code;
  private boolean day = true;

  ConditionIcon(Context ctx, int size) {
    super(ctx);
    this.size = size;
  }

  /** A square of {@code size}, unless the parent says otherwise. */
  @Override
  protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
    setMeasuredDimension(resolveSize(size, widthMeasureSpec), resolveSize(size, heightMeasureSpec));
  }

  void set(int code, boolean day) {
    if (this.code == code && this.day == day) {
      return;
    }
    this.code = code;
    this.day = day;
    invalidate();
  }

  @Override
  protected void onDraw(Canvas canvas) {
    WeatherIcons.draw(canvas, paint, code, day, size / 2f, size / 2f, size);
  }
}
