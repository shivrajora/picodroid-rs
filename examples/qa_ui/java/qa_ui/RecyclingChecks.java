// SPDX-License-Identifier: GPL-3.0-only
package qa_ui;

import picodroid.content.Context;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.TextView;

/**
 * {@code ListView} recycling through {@code getView} / {@code convertView}; apart from {@link
 * QaUiActivity} only because that class is at the PAPK's 64 KB class-file limit.
 */
final class RecyclingChecks {
  private final Context ctx;

  /**
   * The recycling list ({@link #listRecycling}): a click on a window slot and a smooth scroll are
   * delivered on later frames, so the activity's phases 2 and 3 read them here.
   */
  ListView list;

  final int[] click = new int[] {-1};

  RecyclingChecks(Context ctx) {
    this.ctx = ctx;
  }

  /** The row of {@code lv} whose first line reads {@code text}, or null. */
  private static View rowShowing(ListView lv, String text) {
    for (int i = 0; i < lv.getChildCount(); i++) {
      View row = lv.getChildAt(i);
      if (((TextView) ((LinearLayout) row).getChildAt(0)).getText().toString().equals(text)) {
        return row;
      }
    }
    return null;
  }

  /**
   * A list longer than its window keeps only the rows in view (plus one beyond each edge) and hands
   * them back to {@code getView} as the window moves; a short one is still built whole.
   */
  void listRecycling(LinearLayout root) {
    ListView lv = new ListView(ctx);
    root.addView(lv, new LinearLayout.LayoutParams(200, 120));
    String[] titles = new String[60];
    for (int i = 0; i < 60; i++) {
      titles[i] = "r" + i;
    }
    RowAdapter rows = new RowAdapter(ctx, titles);
    lv.setAdapter(rows);
    int w = lv.getChildCount();
    Log.i(QaUiActivity.TAG, "recycling window=" + w + " built=" + rows.built);
    QaUiActivity.check("a long list keeps a window of rows", w >= 8 && w < 60 && rows.built == w);
    QaUiActivity.check(
        "the window starts at the top",
        lv.getFirstVisiblePosition() == 0 && lv.getPositionForView(lv.getChildAt(0)) == 0);
    lv.setSelection(30);
    QaUiActivity.check(
        "setSelection moves the window",
        lv.getFirstVisiblePosition() == 30 && lv.getChildCount() == w);
    QaUiActivity.check(
        "the window recycles rather than builds", rows.built == w && rows.rebound > 0);
    View r30 = rowShowing(lv, "r30");
    QaUiActivity.check("the selected row is live", r30 != null && lv.getPositionForView(r30) == 30);
    titles[30] = "R30";
    rows.notifyDataSetChanged();
    QaUiActivity.check(
        "notify re-binds the window in place", rows.built == w && rowShowing(lv, "R30") != null);
    rows.visible = 20;
    rows.notifyDataSetChanged();
    QaUiActivity.check(
        "fewer rows clamps the window to the end",
        lv.getChildCount() == Math.min(20, w)
            && lv.getPositionForView(lv.getChildAt(lv.getChildCount() - 1)) == 19);
    rows.visible = 5;
    rows.notifyDataSetChanged();
    QaUiActivity.check(
        "a short list is whole again",
        lv.getChildCount() == 5 && lv.getPositionForView(lv.getChildAt(0)) == 0);
    rows.visible = 60;
    rows.notifyDataSetChanged();
    lv.setSelection(45);
    lv.setOnItemClickListener((parent, view, position, id) -> click[0] = position);
    // Slot 2 of a window whose first row is one above position 45: position 46. The click is
    // delivered next frame, before the scroll below moves the window.
    lv.getChildAt(2).performClick();
    lv.smoothScrollToPosition(59);
    list = lv;
  }

  /** A two-line row adapter in the Android shape: build on null, re-bind the convertView. */
  static final class RowAdapter extends picodroid.widget.BaseAdapter {
    final Context ctx;
    final String[] titles;

    /** How many of {@link #titles} the adapter currently reports: lowered to shrink the list. */
    int visible;

    int built = 0;
    int rebound = 0;

    RowAdapter(Context ctx, String[] titles) {
      this.ctx = ctx;
      this.titles = titles;
      this.visible = titles.length;
    }

    @Override
    public int getCount() {
      return visible;
    }

    @Override
    public Object getItem(int position) {
      return titles[position];
    }

    @Override
    public View getView(int position, View convertView, ViewGroup parent) {
      LinearLayout row = (LinearLayout) convertView;
      if (row == null) {
        row = new LinearLayout(ctx);
        row.setOrientation(LinearLayout.VERTICAL);
        row.addView(new TextView(ctx));
        row.addView(new TextView(ctx));
        built++;
      } else {
        rebound++;
      }
      ((TextView) row.getChildAt(0)).setText(titles[position]);
      ((TextView) row.getChildAt(1)).setText("row " + position);
      return row;
    }
  }

  void getViewRecycling(LinearLayout root) {
    ListView lv = new ListView(ctx);
    root.addView(lv, new LinearLayout.LayoutParams(200, 120));
    RowAdapter rows = new RowAdapter(ctx, new String[] {"alpha", "beta", "gamma"});
    lv.setAdapter(rows);
    QaUiActivity.check("getView built every row", rows.built == 3 && rows.rebound == 0);
    QaUiActivity.check("rows are the list's children", lv.getChildCount() == 3);
    View second = lv.getChildAt(1);
    QaUiActivity.check(
        "getChildAt is the row getView returned",
        second instanceof LinearLayout
            && ((TextView) ((LinearLayout) second).getChildAt(0))
                .getText()
                .toString()
                .equals("beta"));
    rows.titles[1] = "BETA";
    rows.notifyDataSetChanged();
    QaUiActivity.check("notify re-binds every row in place", rows.built == 3 && rows.rebound == 3);
    QaUiActivity.check("convertView kept its identity", lv.getChildAt(1) == second);
    QaUiActivity.check(
        "re-bound row shows the new item",
        ((TextView) ((LinearLayout) second).getChildAt(0)).getText().toString().equals("BETA"));
    RowAdapter more = new RowAdapter(ctx, new String[] {"a", "b", "c", "d", "e"});
    lv.setAdapter(more);
    QaUiActivity.check(
        "a new adapter starts from fresh rows", more.built == 5 && lv.getChildCount() == 5);
    QaUiActivity.check("the old adapter's rows were freed", second.getParent() == null);
    View first = lv.getChildAt(0);
    more.visible = 2;
    more.notifyDataSetChanged();
    QaUiActivity.check(
        "fewer rows frees the surplus",
        lv.getChildCount() == 2 && lv.getChildAt(0) == first && more.built == 5);
    more.visible = 4;
    more.notifyDataSetChanged();
    QaUiActivity.check(
        "growth re-binds the kept rows and builds the rest",
        lv.getChildCount() == 4 && more.built == 7 && lv.getChildAt(0) == first);
    ArrayAdapter<String> plainText =
        new ArrayAdapter<String>(
            ctx, picodroid.R.layout.simple_list_item_1, new String[] {"one", "two"});
    lv.setAdapter(plainText);
    QaUiActivity.check(
        "ArrayAdapter rows are TextViews",
        lv.getChildCount() == 2
            && lv.getChildAt(1) instanceof TextView
            && ((TextView) lv.getChildAt(1)).getText().toString().equals("two"));
    plainText.add("three");
    plainText.notifyDataSetChanged();
    QaUiActivity.check("ArrayAdapter growth appends a row", lv.getChildCount() == 3);
    QaUiActivity.check("ArrayAdapter.getPosition", plainText.getPosition("three") == 2);
    plainText.remove("two");
    plainText.insert("zero", 0);
    plainText.notifyDataSetChanged();
    QaUiActivity.check(
        "ArrayAdapter insert/remove re-bind in place",
        lv.getChildCount() == 3
            && ((TextView) lv.getChildAt(0)).getText().toString().equals("zero")
            && ((TextView) lv.getChildAt(2)).getText().toString().equals("three"));
    root.removeView(lv);
  }
}
