// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.UsageSnapshot;
import picodroid.content.Context;
import picodroid.graphics.Canvas;
import picodroid.graphics.Paint;
import picodroid.util.AttributeSet;
import picodroid.view.View;

/**
 * The History page's week: one bar per day scaled to the week's busiest, today's in the accent
 * colour, with the day's letter under it. One view drawing bars and letters, where it used to be
 * fourteen views.
 */
public final class WeekChart extends View {
  static final int COLUMN = 40;

  /** The busiest day's bar, in pixels. */
  static final int BAR_MAX = 58;

  private static final int BAR_WIDTH = 26;
  private static final int STUB = 2;
  private static final int MIN_BAR = 4;

  /** The letters' line starts this far below the bars' baseline. */
  private static final int LETTER_GAP = 3;

  /** Room for one line of the letters' 14 px face. */
  private static final int LETTER_LINE = 18;

  private final Palette palette;
  private final Paint paint = new Paint();
  private final int days;
  private final int[] heights;
  private String letters = "";

  /**
   * Whether {@link #set} has run. Until then every bar is a stub in its day's colour; after it, a
   * day without tokens is a stub in the track colour.
   */
  private boolean sized;

  public WeekChart(Context context, AttributeSet attrs) {
    super(context);
    palette = Palette.of(context.getResources());
    days = UsageSnapshot.DAYS;
    heights = new int[days];
    for (int d = 0; d < days; d++) {
      heights[d] = STUB;
    }
    paint.setTextAlign(Paint.Align.CENTER);
    paint.setTextSize(14);
  }

  /** A column per day, the tallest bar and a line of text for the letters under it. */
  @Override
  protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
    setMeasuredDimension(
        resolveSize(days * COLUMN, widthMeasureSpec),
        resolveSize(BAR_MAX + LETTER_GAP + LETTER_LINE, heightMeasureSpec));
  }

  /**
   * The week's token counts, oldest first, and one letter per day. Returns whether anything drawn
   * changed; call {@link #invalidate} if so.
   */
  boolean set(int[] tokens, String dayLetters) {
    int max = 0;
    for (int d = 0; d < days; d++) {
      if (tokens[d] > max) {
        max = tokens[d];
      }
    }
    boolean changed = !sized || !dayLetters.equals(letters);
    sized = true;
    letters = dayLetters;
    for (int d = 0; d < days; d++) {
      int h =
          max == 0 || tokens[d] == 0
              ? STUB
              : MIN_BAR + (int) ((long) tokens[d] * (BAR_MAX - MIN_BAR) / max);
      if (h != heights[d]) {
        heights[d] = h;
        changed = true;
      }
    }
    return changed;
  }

  @Override
  protected void onDraw(Canvas canvas) {
    float baseline = BAR_MAX + LETTER_GAP - paint.ascent();
    for (int d = 0; d < days; d++) {
      boolean today = d == days - 1;
      int h = heights[d];
      int x = d * COLUMN + (COLUMN - BAR_WIDTH) / 2;
      boolean stub = sized && h <= STUB;
      paint.setColor(stub ? palette.track : (today ? palette.clay : palette.barPast));
      float radius = stub ? 1 : 4;
      canvas.drawRoundRect(x, BAR_MAX - h, x + BAR_WIDTH, BAR_MAX, radius, radius, paint);
      if (d < letters.length()) {
        paint.setColor(today ? palette.text : palette.muted);
        canvas.drawText(letters.substring(d, d + 1), d * COLUMN + COLUMN / 2f, baseline, paint);
      }
    }
  }
}
