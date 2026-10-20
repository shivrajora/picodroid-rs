// SPDX-License-Identifier: GPL-3.0-only
package layoutdemo;

import picodroid.content.Context;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * A custom layout, written as it would be on Android: {@code ViewGroup(Context)}, children measured
 * in {@link #onLayout} and placed left to right with {@link View#layout}, wrapping to the next row
 * when one is full. {@link #onMeasure} sizes a {@code wrap_content} group to its rows.
 */
public final class FlowLayout extends ViewGroup {
  /** How many times the framework laid the children out, and the rows the last pass made. */
  int layouts;

  int rows;

  public FlowLayout(Context context) {
    super(context);
  }

  @Override
  protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
    int width = MeasureSpec.getSize(widthMeasureSpec);
    measureChildren(
        MeasureSpec.makeMeasureSpec(width, MeasureSpec.AT_MOST),
        MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED));
    int height = rowsHeight(width, null);
    setMeasuredDimension(
        resolveSize(width, widthMeasureSpec), resolveSize(height, heightMeasureSpec));
  }

  @Override
  protected void onLayout(boolean changed, int l, int t, int r, int b) {
    layouts++;
    int width = r - l;
    measureChildren(
        MeasureSpec.makeMeasureSpec(width, MeasureSpec.AT_MOST),
        MeasureSpec.makeMeasureSpec(b - t, MeasureSpec.AT_MOST));
    rowsHeight(width, this);
  }

  /** Walks the rows; with {@code place} set, lays each child out where it falls. */
  private int rowsHeight(int width, FlowLayout place) {
    int x = 0;
    int y = 0;
    int rowHeight = 0;
    int count = 1;
    for (int i = 0; i < getChildCount(); i++) {
      View child = getChildAt(i);
      if (child.getVisibility() == GONE) {
        continue;
      }
      int w = child.getMeasuredWidth();
      int h = child.getMeasuredHeight();
      if (x > 0 && x + w > width) {
        x = 0;
        y += rowHeight;
        rowHeight = 0;
        count++;
      }
      if (place != null) {
        child.layout(x, y, x + w, y + h);
      }
      x += w;
      rowHeight = Math.max(rowHeight, h);
    }
    if (place != null) {
      rows = count;
    }
    return y + rowHeight;
  }
}
