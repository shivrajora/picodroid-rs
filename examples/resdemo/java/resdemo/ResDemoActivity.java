// SPDX-License-Identifier: GPL-3.0-only
package resdemo;

import picodroid.app.Activity;
import picodroid.content.res.Resources;
import picodroid.util.Log;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.CheckBox;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.ProgressBar;
import picodroid.widget.TextView;

/**
 * The resource system end to end: {@code res/values}, a layout inflated by {@code
 * setContentView(R.layout.*)}, {@code findViewById}, a second layout inflated into a parent, and a
 * {@code res/drawable} image. It checks what it built and logs one PASS/FAIL line per check, so a
 * sim or HIL run of it is a test, not only a picture.
 */
public class ResDemoActivity extends Activity {
  private static final String TAG = "ResDemo";

  private int failures;
  private int taps;

  @Override
  public void onCreate() {
    Resources res = getResources();
    check("getString", "Hello from res/values".equals(res.getString(R.string.greeting)));
    check("Context.getString", "Res Demo".equals(getString(R.string.app_name)));
    check("getColor", res.getColor(R.color.accent) == 0xFFFFB300);
    check("color alias", res.getColor(R.color.title) == res.getColor(R.color.accent));
    check("getDimension", res.getDimension(R.dimen.hairline) == 0.5f);
    check("getDimensionPixelSize", res.getDimensionPixelSize(R.dimen.hairline) == 1);
    check("getDimensionPixelOffset", res.getDimensionPixelOffset(R.dimen.gap) == 8);
    check("getInteger", res.getInteger(R.integer.max_taps) == 10);
    check("getBoolean", res.getBoolean(R.bool.show_logo));

    boolean threw = false;
    try {
      res.getString(R.color.accent);
    } catch (Resources.NotFoundException e) {
      threw = true;
    }
    check("NotFoundException", threw);

    setContentView(R.layout.activity_main);

    TextView title = findViewById(R.id.title);
    check("findViewById", title != null && title.getId() == R.id.title);
    check("text from @string", title != null && "Res Demo".equals(title.getText().toString()));
    check("missing id is null", findViewById(R.id.row_label) == null);

    final Button tap = findViewById(R.id.tap);
    check("literal text", tap != null && "Tap".equals(tap.getText().toString()));
    CheckBox armed = findViewById(R.id.armed);
    check("checked", armed != null && armed.isChecked());
    ProgressBar progress = findViewById(R.id.progress);
    check("progress", progress != null && progress.getProgress() == 30);
    ImageView logo = findViewById(R.id.logo);
    check("drawable", logo != null);

    LinearLayout buttons = findViewById(R.id.buttons);
    check("nested group", buttons != null && buttons.getChildCount() == 2);
    ViewGroup.LayoutParams lp = tap == null ? null : tap.getLayoutParams();
    check(
        "layout_weight",
        lp instanceof LinearLayout.LayoutParams && ((LinearLayout.LayoutParams) lp).weight == 1f);

    // inflate(id, parent, true) attaches and returns the parent; (…, false) only borrows its
    // LayoutParams type.
    LinearLayout rows = findViewById(R.id.rows);
    LayoutInflater inflater = getLayoutInflater();
    View attached = inflater.inflate(R.layout.row, rows, true);
    check("attachToRoot returns root", attached == rows && rows.getChildCount() == 1);
    View row = rows.getChildAt(0);
    check("row attributes", row.getId() == R.id.row_label && row.getVisibility() == View.INVISIBLE);
    row.setVisibility(View.VISIBLE);
    View detached = inflater.inflate(R.layout.row, rows, false);
    check(
        "attachToRoot=false",
        detached != rows
            && rows.getChildCount() == 1
            && detached.getLayoutParams() instanceof LinearLayout.LayoutParams);
    rows.addView(detached, detached.getLayoutParams());
    detached.setVisibility(View.VISIBLE);
    check("findViewById sees inflated rows", findViewById(R.id.row_label) == row);

    // Laid-out geometry: match_parent on the root fills the display, a dimension is pixels, and
    // the weighted button takes what the checkbox leaves.
    View root = findViewById(R.id.root);
    check(
        "match_parent root",
        root.getWidth() == getDisplay().getWidth() && root.getHeight() == getDisplay().getHeight());
    check("@dimen size", logo != null && logo.getWidth() == 64 && logo.getHeight() == 64);
    // Content width: the display less the 8 px padding and LinearLayout's 2 px theme border.
    View greeting = findViewById(R.id.greeting);
    check("padding", greeting != null && greeting.getWidth() == root.getWidth() - 2 * 8 - 2 * 2);
    check("wrap_content bar keeps its height", progress != null && progress.getHeight() > 0);
    check(
        "weighted width",
        tap != null
            && armed != null
            && tap.getWidth() > 0
            && tap.getWidth() + armed.getWidth() <= greeting.getWidth()
            && tap.getWidth() > armed.getWidth());

    if (tap != null) {
      final int max = res.getInteger(R.integer.max_taps);
      final String label = getString(R.string.count_label);
      tap.setOnClickListener(
          v -> {
            taps = Math.min(max, taps + 1);
            tap.setText(label + ": " + taps);
          });
    }

    Log.i(TAG, failures == 0 ? "ResDemo PASS" : "ResDemo FAIL (" + failures + ")");
  }

  private void check(String what, boolean ok) {
    if (!ok) {
      failures++;
    }
    Log.i(TAG, (ok ? "ok   " : "FAIL ") + what);
  }
}
