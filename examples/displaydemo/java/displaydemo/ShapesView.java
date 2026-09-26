// SPDX-License-Identifier: GPL-3.0-only
package displaydemo;

import picodroid.content.Context;
import picodroid.graphics.Canvas;
import picodroid.graphics.Paint;
import picodroid.graphics.Theme;
import picodroid.view.View;

/** One of each thing a {@link Canvas} draws, in a custom view's {@code onDraw}. */
final class ShapesView extends View {
  static final int WIDTH = 300;
  static final int HEIGHT = 72;

  private final Paint fill = new Paint();
  private final Paint stroke = new Paint();
  private final Paint text = new Paint();

  ShapesView(Context ctx) {
    super(ctx);
    setSize(WIDTH, HEIGHT);
    fill.setColor(Theme.colorPrimary);
    stroke.setColor(Theme.colorPrimary);
    stroke.setStyle(Paint.Style.STROKE);
    stroke.setStrokeWidth(3);
    text.setColor(Theme.colorText);
    text.setTextAlign(Paint.Align.CENTER);
    text.setTextSize(14);
  }

  @Override
  protected void onDraw(Canvas canvas) {
    canvas.drawRoundRect(2, 4, 58, 44, 8, 8, fill);
    canvas.drawRect(70, 6, 116, 42, stroke);

    stroke.setStrokeCap(Paint.Cap.ROUND);
    stroke.setStrokeWidth(5);
    canvas.drawLine(130, 42, 166, 6, stroke);

    canvas.drawCircle(196, 24, 18, fill);

    stroke.setStrokeWidth(6);
    canvas.drawArc(222, 2, 266, 46, 135, 270, false, stroke);

    canvas.drawArc(272, 2, 300, 30, -90, 120, true, fill);

    stroke.setStrokeCap(Paint.Cap.BUTT);
    stroke.setStrokeWidth(3);
    canvas.drawText("drawn with Canvas", WIDTH / 2f, HEIGHT - 4 - text.descent(), text);
  }
}
