// SPDX-License-Identifier: GPL-3.0-only
package qa_ui;

import picodroid.content.Context;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.util.Log;
import picodroid.view.Gravity;
import picodroid.view.ViewGroup;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Layout params applied to a view already in a layout, and {@code GradientDrawable}'s Android
 * constructor; apart from {@link QaUiActivity} only because that class is near the PAPK's 64 KB
 * class-file limit.
 */
final class ParamsChecks {
  private final Context ctx;
  private LinearLayout h;
  private TextView a;
  private TextView b;

  ParamsChecks(Context ctx) {
    this.ctx = ctx;
  }

  /**
   * A weighted pair in a horizontal row that stays in the tree (the churn sections count it). The 4
   * px gap is b's start margin. Then a is re-parameterised while parented with plain {@code
   * MarginLayoutParams}: a new width and a start margin applied at once, and its weight cleared,
   * since params of a plainer class carry none (Android's parent converts them).
   */
  void weightedRow(LinearLayout root) {
    h = new LinearLayout(ctx);
    h.setOrientation(LinearLayout.HORIZONTAL);
    h.setGravity(Gravity.CENTER);
    a = new TextView(ctx);
    a.setText("A");
    b = new TextView(ctx);
    b.setText("B");
    h.addView(a, new LinearLayout.LayoutParams(50, 20, 1f));
    LinearLayout.LayoutParams blp = new LinearLayout.LayoutParams(50, 20, 2f);
    blp.leftMargin = 4;
    h.addView(b, blp);
    root.addView(h);
    QaUiActivity.check("horizontal children", h.getChildCount() == 2 && h.getChildAt(1) == b);
    ViewGroup.MarginLayoutParams alp = new ViewGroup.MarginLayoutParams(30, 20);
    alp.leftMargin = 6;
    a.setLayoutParams(alp);
    QaUiActivity.check("setLayoutParams replaces the params", a.getLayoutParams() == alp);
    boolean npe = false;
    try {
      a.setLayoutParams(null);
    } catch (NullPointerException e) {
      npe = true;
    }
    QaUiActivity.check(
        "setLayoutParams(null) throws NullPointerException", npe && a.getLayoutParams() == alp);
  }

  void weightedRowLaidOut() {
    Log.i(
        QaUiActivity.TAG,
        "weighted row w="
            + h.getWidth()
            + " a="
            + a.getLeft()
            + "+"
            + a.getWidth()
            + " b="
            + b.getLeft()
            + "+"
            + b.getWidth());
    QaUiActivity.check(
        "setLayoutParams on a parented view: new width, plainer params clear the weight",
        a.getWidth() == 30 && a.getHeight() == 20);
    QaUiActivity.check("setLayoutParams on a parented view: margins at once", a.getLeft() == 6);
    QaUiActivity.check(
        "the weighted sibling takes the rest",
        b.getLeft() == a.getRight() + 4 && b.getRight() == h.getWidth() && b.getWidth() > 50);
  }

  /** The Android constructor and setters, both reversed orientations, applied as a background. */
  void gradients(LinearLayout root) {
    int[] colors = new int[] {Color.RED, Color.BLUE};
    GradientDrawable up = new GradientDrawable(GradientDrawable.Orientation.BOTTOM_TOP, colors);
    QaUiActivity.check(
        "GradientDrawable(Orientation, int[])",
        up.getOrientation() == GradientDrawable.Orientation.BOTTOM_TOP && up.getColors() == colors);
    GradientDrawable left = new GradientDrawable();
    left.setColors(new int[] {Color.GREEN, Color.BLACK});
    left.setOrientation(GradientDrawable.Orientation.RIGHT_LEFT);
    left.setCornerRadius(4f);
    left.setStroke(1, Color.WHITE);
    QaUiActivity.check(
        "GradientDrawable setColors + setOrientation",
        left.getOrientation() == GradientDrawable.Orientation.RIGHT_LEFT
            && left.getColors().length == 2);
    TextView v = new TextView(ctx);
    v.setText("grad");
    root.addView(v, new LinearLayout.LayoutParams(60, 20));
    v.setBackground(up);
    v.setBackground(left);
    QaUiActivity.check("setBackground keeps the drawable", v.getBackground() == left);
    left.setColor(Color.GREEN);
    QaUiActivity.check("setColor drops the gradient", left.getColors() == null);
    root.removeView(v);
  }
}
