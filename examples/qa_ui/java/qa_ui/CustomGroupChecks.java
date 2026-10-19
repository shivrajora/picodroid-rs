// SPDX-License-Identifier: GPL-3.0-only
package qa_ui;

import picodroid.concurrent.Executors;
import picodroid.content.Context;
import picodroid.util.Log;
import picodroid.view.MotionEvent;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * The checks for a ViewGroup that lays itself out ({@code ViewGroup(Context)} + {@code onLayout}),
 * {@code View.getContext}, {@code View.post} and {@code onTouchEvent}; apart from {@link
 * QaUiActivity} only because that class is at the PAPK's 64 KB class-file limit.
 */
final class CustomGroupChecks {
  /** A left-to-right row layout that wraps: Android's custom-ViewGroup shape, verbatim. */
  static final class Flow extends ViewGroup {
    int layouts;
    boolean lastChanged;
    int lastWidth;
    int lastHeight;
    int rows;

    Flow(Context ctx) {
      super(ctx);
    }

    @Override
    protected void onLayout(boolean changed, int l, int t, int r, int b) {
      layouts++;
      lastChanged = changed;
      lastWidth = r - l;
      lastHeight = b - t;
      int width = r - l;
      measureChildren(
          MeasureSpec.makeMeasureSpec(width, MeasureSpec.AT_MOST),
          MeasureSpec.makeMeasureSpec(b - t, MeasureSpec.AT_MOST));
      int x = 0;
      int y = 0;
      int rowHeight = 0;
      rows = 1;
      for (int i = 0; i < getChildCount(); i++) {
        View child = getChildAt(i);
        int w = child.getMeasuredWidth();
        int h = child.getMeasuredHeight();
        if (x > 0 && x + w > width) {
          x = 0;
          y += rowHeight;
          rowHeight = 0;
          rows++;
        }
        child.layout(x, y, x + w, y + h);
        x += w;
        rowHeight = Math.max(rowHeight, h);
      }
    }
  }

  /** A view that wants the finger itself, with no listener: Android's onTouchEvent override. */
  static final class TouchView extends View {
    int downs;
    int lastX;
    int lastY;

    TouchView(Context ctx) {
      super(ctx);
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
      if (event.getAction() == MotionEvent.ACTION_DOWN) {
        downs++;
        lastX = event.getX();
        lastY = event.getY();
      }
      return true;
    }
  }

  private final QaUiActivity activity;

  CustomGroupChecks(QaUiActivity activity) {
    this.activity = activity;
  }

  private Flow flow;
  private final TextView[] flowChildren = new TextView[5];
  private final int[] posted = new int[1];
  private int layoutsAfterPhase2;

  void customGroup(LinearLayout root) {
    Context ctx = activity;
    flow = new Flow(ctx);
    flow.setSize(120, 60);
    for (int i = 0; i < flowChildren.length; i++) {
      TextView tv = new TextView();
      tv.setText("item " + i);
      flowChildren[i] = tv;
      flow.addView(
          tv,
          new ViewGroup.LayoutParams(
              ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    }
    root.addView(flow);
    QaUiActivity.check("onLayout is deferred to the main loop", flow.layouts == 0);
    QaUiActivity.check(
        "indexOfChild", flow.indexOfChild(flowChildren[3]) == 3 && flow.indexOfChild(root) == -1);

    // getContext: the constructor's, a parent's for a view made without one, null detached.
    QaUiActivity.check("getContext from the constructor", flow.getContext() == ctx);
    QaUiActivity.check("getContext through the parent", flowChildren[0].getContext() == ctx);
    QaUiActivity.check(
        "getContext on a widget made with one", new TextView(ctx).getContext() == ctx);
    QaUiActivity.check(
        "getContext on a detached no-arg widget", new TextView().getContext() == null);
    QaUiActivity.check("getContext on a Button", new Button(ctx, "b").getContext() == ctx);

    // post: runs on a later frame, never inline.
    boolean accepted = root.post(() -> posted[0]++);
    QaUiActivity.check(
        "post is accepted and deferred", accepted && posted[0] == 0 && !root.post(null));

    // onTouchEvent through dispatchTouchEvent, listener first. In the tree: a view that draws
    // itself is bound to its widget from construction, so one nothing reaches is reported.
    TouchView touch = new TouchView(ctx);
    touch.setSize(10, 10);
    root.addView(touch);
    boolean consumed =
        touch.dispatchTouchEvent(MotionEvent.obtain(0, 5, MotionEvent.ACTION_DOWN, 3, 4, 0));
    QaUiActivity.check("onTouchEvent reached without a listener", consumed && touch.downs == 1);
    QaUiActivity.check(
        "MotionEvent.obtain carries the point", touch.lastX == 3 && touch.lastY == 4);
    touch.setOnTouchListener((v, e) -> true);
    touch.dispatchTouchEvent(MotionEvent.obtain(0, 6, MotionEvent.ACTION_DOWN, 1, 1, 0));
    QaUiActivity.check("a consuming listener runs first", touch.downs == 1);
    touch.setOnTouchListener((v, e) -> false);
    touch.dispatchTouchEvent(MotionEvent.obtain(0, 7, MotionEvent.ACTION_DOWN, 1, 1, 0));
    QaUiActivity.check("a declining listener falls through to onTouchEvent", touch.downs == 2);

    // removeViewAt on a framework layout.
    LinearLayout scratch = new LinearLayout(ctx);
    TextView a = new TextView(ctx);
    TextView b = new TextView(ctx);
    TextView c = new TextView(ctx);
    scratch.addView(a);
    scratch.addView(b);
    scratch.addView(c);
    scratch.removeViewAt(1);
    QaUiActivity.check(
        "removeViewAt",
        scratch.getChildCount() == 2
            && scratch.indexOfChild(c) == 1
            && scratch.indexOfChild(b) == -1
            && b.getParent() == null);
    scratch.removeViewAt(7); // out of range: a no-op
    QaUiActivity.check("removeViewAt out of range", scratch.getChildCount() == 2);
  }

  void customGroupLaidOut() {
    Log.i(
        QaUiActivity.TAG,
        "flow layouts="
            + flow.layouts
            + " frame="
            + flow.lastWidth
            + "x"
            + flow.lastHeight
            + " rows="
            + flow.rows
            + " c0="
            + flowChildren[0].getLeft()
            + ","
            + flowChildren[0].getTop()
            + " "
            + flowChildren[0].getWidth()
            + "x"
            + flowChildren[0].getHeight()
            + " c1="
            + flowChildren[1].getLeft()
            + " c4="
            + flowChildren[4].getLeft()
            + ","
            + flowChildren[4].getTop());
    QaUiActivity.check("onLayout ran once or twice", flow.layouts >= 1 && flow.layouts <= 2);
    QaUiActivity.check(
        "onLayout got the group's frame", flow.lastWidth == 120 && flow.lastHeight == 60);
    QaUiActivity.check(
        "children measured to their content",
        flowChildren[0].getWidth() > 0 && flowChildren[0].getWidth() < 120);
    QaUiActivity.check(
        "children placed where layout() said",
        flowChildren[0].getLeft() == 0
            && flowChildren[0].getTop() == 0
            && flowChildren[1].getLeft() == flowChildren[0].getWidth()
            && flowChildren[1].getTop() == 0);
    QaUiActivity.check("the row wrapped", flow.rows > 1 && flowChildren[4].getTop() > 0);
    QaUiActivity.check("post ran on a later frame, once", posted[0] == 1);
    layoutsAfterPhase2 = flow.layouts;
    flow.setSize(200, 60);
  }

  void customGroupResized(LinearLayout root) {
    Log.i(
        QaUiActivity.TAG,
        "flow layouts=" + flow.layouts + " frame=" + flow.lastWidth + "x" + flow.lastHeight);
    QaUiActivity.check(
        "a size change re-lays out (SIZE_CHANGED)",
        flow.layouts > layoutsAfterPhase2 && flow.lastWidth == 200 && flow.lastChanged);
    int before = flow.layouts;
    flowChildren[2].requestLayout();
    QaUiActivity.check("requestLayout is deferred", flow.layouts == before);
    // The layout task is queued ahead of this one; the group goes once it has run.
    Executors.mainExecutor()
        .execute(
            () -> {
              QaUiActivity.check("requestLayout bubbled to the group", flow.layouts == before + 1);
              root.removeView(flow);
            });
  }
}
