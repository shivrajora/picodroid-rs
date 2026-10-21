// SPDX-License-Identifier: GPL-3.0-only
package qa_list;

import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.BaseAdapter;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.TextView;

/**
 * QA 2026-10-20: a {@link ListView} of 200 rows, driven by touch and keys from {@code test.ctrl}.
 * The adapter counts the rows it builds, so the log shows the window the list keeps alive; a click
 * checks that the row's text is the item at the position the list reports, after the window moved.
 * {@code input back} ends the app so the heap line prints.
 */
public class QaListActivity extends Activity {
  static final String TAG = "QaList";
  static final int ROWS = 200;

  /** Rows read "item N"; builds and re-binds are counted. */
  static final class CountingAdapter extends BaseAdapter {
    final Context ctx;
    int built;
    int rebound;

    CountingAdapter(Context ctx) {
      this.ctx = ctx;
    }

    @Override
    public int getCount() {
      return ROWS;
    }

    @Override
    public Object getItem(int position) {
      return "item " + position;
    }

    @Override
    public View getView(int position, View convertView, ViewGroup parent) {
      TextView row = (TextView) convertView;
      if (row == null) {
        row = new TextView(ctx);
        built++;
      } else {
        rebound++;
      }
      row.setText((String) getItem(position));
      return row;
    }
  }

  private int clicks;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    getDisplay();
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.MATCH_PARENT);

    TextView header = new TextView(this);
    header.setText(ROWS + " rows");
    root.addView(header);

    ListView list = new ListView(this);
    CountingAdapter adapter = new CountingAdapter(this);
    list.setAdapter(adapter);
    list.setOnItemClickListener(
        (parent, view, position, id) -> {
          String shown = ((TextView) view).getText().toString();
          boolean ok = shown.equals(adapter.getItem(position));
          clicks++;
          Log.i(
              TAG,
              (ok ? "click ok" : "click MISMATCH")
                  + " #"
                  + clicks
                  + " position="
                  + position
                  + " first="
                  + list.getFirstVisiblePosition()
                  + " last="
                  + list.getLastVisiblePosition()
                  + " window="
                  + list.getChildCount()
                  + " built="
                  + adapter.built
                  + " rebound="
                  + adapter.rebound
                  + " text="
                  + shown);
        });
    // Weighted: the list takes what the header leaves, so the window is the panel's.
    root.addView(
        list, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));
    setContentView(root);
    Log.i(TAG, "ready window=" + list.getChildCount() + " built=" + adapter.built);
  }
}
