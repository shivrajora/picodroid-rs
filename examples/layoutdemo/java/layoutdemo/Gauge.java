// SPDX-License-Identifier: GPL-3.0-only
package layoutdemo;

import picodroid.content.Context;
import picodroid.graphics.Canvas;
import picodroid.graphics.Paint;
import picodroid.util.AttributeSet;
import picodroid.view.View;

/**
 * A custom view a layout names by class: it says how large it wants to be, and draws a bar. Public,
 * with a public {@code (Context, AttributeSet)} constructor, which is what the inflater constructs
 * it through (as on Android; {@code Activity.onCreateView} is not overridden).
 */
public final class Gauge extends View {
  static final int WIDTH = 30;
  static final int HEIGHT = 12;

  private final Paint paint = new Paint();

  /** How many times the framework asked for a size. */
  int measured;

  public Gauge(Context context, AttributeSet attrs) {
    super(context);
    LayoutDemoActivity.gaugeMade(attrs);
    paint.setColor(0xFFD97757);
  }

  @Override
  protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
    measured++;
    setMeasuredDimension(
        resolveSize(WIDTH, widthMeasureSpec), resolveSize(HEIGHT, heightMeasureSpec));
  }

  @Override
  protected void onDraw(Canvas canvas) {
    canvas.drawRoundRect(0, 0, getWidth(), getHeight(), 3, 3, paint);
  }
}
