// SPDX-License-Identifier: GPL-3.0-only
package layoutdemo;

import picodroid.app.Activity;
import picodroid.content.ComponentName;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.res.ColorStateList;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.os.Bundle;
import picodroid.util.AttributeSet;
import picodroid.util.Log;
import picodroid.view.AsyncLayoutInflater;
import picodroid.view.Gravity;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * The layout features an Android page is written with, checked end to end: {@code layout_margin}
 * and {@code layout_gravity}, a {@code <shape>} background, {@code <include>}, a style and the
 * theme, a view class of the app's own made by {@link #onCreateView(String, Context, AttributeSet)}
 * and sized by {@code onMeasure}, and the same layout inflated a slice per tick by {@link
 * AsyncLayoutInflater}. With them, the setters that do nothing when nothing changes, and the small
 * Android-shaped calls that sit beside ({@code runOnUiThread}, {@code getString} with arguments,
 * {@code ComponentName}).
 *
 * <p>Logs one line per check and {@code === ALL PASSED ===} when all hold.
 */
public class LayoutDemoActivity extends Activity {
  private static final String TAG = "LayoutDemo";

  private int failures;
  private int gauges;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    setTheme(R.style.AppTheme);
    check("setTheme: colorPrimary", Theme.colorPrimary == getColor(R.color.accent));
    check("setTheme: textColorPrimary", Theme.colorText == getColor(R.color.ink));
    check("setTheme: colorSurface", Theme.colorSurface == getColor(R.color.panel));

    setContentView(R.layout.activity_main);
    TextView title = findViewById(R.id.title);
    check("style: singleLine from the parent style", title.getMaxLines() == 1);
    check("style: textSize from ?attr", title.getTextSize() == 20f);
    check("text from @string", "Layout demo".equals(title.getText().toString()));

    // A LinearLayout keeps its children's margins clear.
    View first = findViewById(R.id.first);
    View second = findViewById(R.id.second);
    View third = findViewById(R.id.third);
    check("linear: marginLeft", second.getLeft() == first.getLeft() + 10 + 6);
    check("linear: marginRight", third.getLeft() == second.getLeft() + 10 + 9);
    ViewGroup.LayoutParams lp = second.getLayoutParams();
    check(
        "linear: MarginLayoutParams",
        lp instanceof LinearLayout.LayoutParams
            && ((ViewGroup.MarginLayoutParams) lp).leftMargin == 6
            && ((ViewGroup.MarginLayoutParams) lp).rightMargin == 9);

    FrameLayout stages = findViewById(R.id.stages);
    LayoutInflater inflater = getLayoutInflater();
    check("the Activity is the inflater's factory", inflater.getFactory() == this);
    View stage = inflater.inflate(R.layout.stage, stages, false);
    check("inflate made the custom views through onCreateView", gauges == 2);
    stages.addView(stage); // the one-argument add reads the LayoutParams the inflater set
    checkStage("sync", stage);

    // The same margins from code.
    View coded = new FrameLayout(this);
    FrameLayout.LayoutParams flp =
        new FrameLayout.LayoutParams(8, 8, Gravity.RIGHT | Gravity.CENTER_VERTICAL);
    flp.setMargins(0, 0, 5, 0);
    ((ViewGroup) stage).addView(coded, flp);
    check(
        "frame: params built in code",
        coded.getLeft() == 200 - 8 - 5 && coded.getTop() == (80 - 8) / 2);

    settersThatDoNothing(stage);
    androidShapedCalls();

    // The same layout again, a slice per tick; the verdict waits for its callback.
    final boolean[] returned = {false};
    new AsyncLayoutInflater(this)
        .inflate(
            R.layout.stage,
            stages,
            (view, resid, parent) -> {
              check("async: called back after inflate() returned", returned[0]);
              check("async: the resource and parent it was given", resid == R.layout.stage);
              check("async: parent passed through", parent == stages);
              check("async: not attached", view.getParent() == null);
              check("async: visible again", view.getVisibility() == View.VISIBLE);
              check("async: custom views made", gauges == 4);
              parent.addView(view);
              view.setTranslationY(90f);
              checkStage("async", view);
              report();
            });
    returned[0] = true;
  }

  /** Every element of the app's own that a layout names comes here to be made. */
  @Override
  public View onCreateView(String name, Context context, AttributeSet attrs) {
    if (name.equals("layoutdemo.Gauge")) {
      gauges++;
      check("factory: an empty AttributeSet", attrs != null && attrs.getAttributeCount() == 0);
      return new Gauge(context, attrs);
    }
    return super.onCreateView(name, context, attrs);
  }

  private void checkStage(String how, View stage) {
    check(how + ": root size from its layout", stage.getWidth() == 200 && stage.getHeight() == 80);
    check(how + ": <shape> background", stage.getBackground() instanceof GradientDrawable);
    check(
        how + ": <shape> with a stroke",
        stage.findViewById(R.id.outlined).getBackground() instanceof GradientDrawable);

    View atMargin = stage.findViewById(R.id.at_margin);
    check(how + ": margins place a child", atMargin.getLeft() == 7 && atMargin.getTop() == 9);
    View atCorner = stage.findViewById(R.id.at_corner);
    check(
        how + ": gravity right|bottom, in by the margins",
        atCorner.getLeft() == 200 - 10 - 3 && atCorner.getTop() == 80 - 10 - 4);
    View atCentre = stage.findViewById(R.id.at_centre);
    check(
        how + ": gravity center",
        atCentre.getLeft() == (200 - 10) / 2 && atCentre.getTop() == (80 - 10) / 2);
    ViewGroup.LayoutParams lp = atCorner.getLayoutParams();
    check(
        how + ": FrameLayout.LayoutParams",
        lp instanceof FrameLayout.LayoutParams
            && ((FrameLayout.LayoutParams) lp).gravity == (Gravity.RIGHT | Gravity.BOTTOM)
            && ((FrameLayout.LayoutParams) lp).rightMargin == 3
            && ((FrameLayout.LayoutParams) lp).bottomMargin == 4);

    View gauge = stage.findViewById(R.id.gauge);
    check(how + ": custom view class", gauge instanceof Gauge);
    check(
        how + ": wrap_content asks onMeasure",
        gauge.getWidth() == Gauge.WIDTH
            && gauge.getHeight() == Gauge.HEIGHT
            && ((Gauge) gauge).measured == 1);
    check(how + ": custom view placed", gauge.getLeft() == 40 && gauge.getTop() == 6);
    View wide = stage.findViewById(R.id.wide_gauge);
    check(
        how + ": a fixed width is EXACTLY, the wrapped height measured",
        wide.getWidth() == 50 && wide.getHeight() == Gauge.HEIGHT);
    check(how + ": gravity bottom", wide.getTop() == 80 - Gauge.HEIGHT && wide.getLeft() == 40);
    check(
        how + ": getMeasuredWidth",
        wide.getMeasuredWidth() == 50 && wide.getMeasuredHeight() == Gauge.HEIGHT);

    TextView included = stage.findViewById(R.id.included);
    check(how + ": <include> takes the include's id", included != null);
    check(how + ": ...and not its root's", stage.findViewById(R.id.badge) == null);
    check(
        how + ": <include> keeps the included attributes",
        included != null
            && "badge".equals(included.getText().toString())
            && included.getMaxLines() == 1
            && included.getLeft() == 100
            && included.getTop() == 30);

    TextView trimmed = stage.findViewById(R.id.trimmed);
    check(
        how + ": includeFontPadding=false hugs the glyphs",
        trimmed.getHeight() > 0 && trimmed.getHeight() < trimmed.getLineHeight());
    check(how + ": gravity right", trimmed.getLeft() + trimmed.getWidth() == 200);
  }

  /** A setter given the value the view already has does not reach the renderer. */
  private void settersThatDoNothing(View stage) {
    TextView label = stage.findViewById(R.id.included);
    label.setText("one");
    check("setText", "one".equals(label.getText().toString()));
    label.setText(null);
    check("setText(null) is the empty string", label.getText().length() == 0);

    View box = stage.findViewById(R.id.at_margin);
    check("no drawable behind a plain colour", box.getBackground() == null);
    check("no tint yet", box.getBackgroundTintList() == null);
    ColorStateList red = ColorStateList.valueOf(0xFFFF0000);
    box.setBackgroundTintList(red);
    check("setBackgroundTintList", box.getBackgroundTintList() == red);
    box.setBackgroundTintList(ColorStateList.valueOf(0xFFFF0000));
    check("an equal tint is the same tint", box.getBackgroundTintList() == red);
    box.setBackgroundTintList(null);
    check("a null tint clears it", box.getBackgroundTintList() == null);

    GradientDrawable pill = new GradientDrawable().setColor(0xFF00FF00).setCornerRadius(5);
    box.setBackground(pill);
    check("getBackground", box.getBackground() == pill);
    box.setBackgroundColor(0xFF0000FF);
    check("a colour replaces the drawable", box.getBackground() == null);

    box.setAlpha(0.5f);
    box.setAlpha(0.5f);
    check("setAlpha", box.getAlpha() == 0.5f);
    box.setVisibility(View.INVISIBLE);
    box.setVisibility(View.INVISIBLE);
    check("setVisibility", box.getVisibility() == View.INVISIBLE);
    box.setVisibility(View.VISIBLE);
    box.animate().alpha(0f).setDuration(5000).start();
    box.animate().cancel();
    check("a cancelled fade reads back where it stopped", box.getAlpha() > 0f);
    box.setAlpha(1f);
    check("...and setAlpha works after it", box.getAlpha() == 1f);
  }

  private void androidShapedCalls() {
    check("getString with arguments", "3 of four".equals(getString(R.string.count, 3, "four")));
    check(
        "Resources.getString with arguments",
        "1 of 2".equals(getResources().getString(R.string.count, 1, "2")));

    final boolean[] ran = {false};
    runOnUiThread(() -> ran[0] = true);
    check("runOnUiThread on the main thread runs at once", ran[0]);
    final boolean[] posted = {false};
    getMainExecutor().execute(() -> posted[0] = true);
    check("getMainExecutor posts, it does not run in place", !posted[0]);

    ComponentName name = new ComponentName(this, LayoutDemoActivity.class);
    check("ComponentName package", "layoutdemo".equals(name.getPackageName()));
    check("ComponentName class", "layoutdemo.LayoutDemoActivity".equals(name.getClassName()));
    check(
        "ComponentName equals",
        name.equals(new ComponentName("layoutdemo", "layoutdemo.LayoutDemoActivity"))
            && name.hashCode()
                == new ComponentName("layoutdemo", "layoutdemo.LayoutDemoActivity").hashCode());
    check(
        "flattenToString",
        "layoutdemo/layoutdemo.LayoutDemoActivity".equals(name.flattenToString()));
    check(
        "Intent(Context, Class)",
        "layoutdemo/LayoutDemoActivity"
            .equals(new Intent(this, LayoutDemoActivity.class).getTargetClassName()));
  }

  private void check(String what, boolean ok) {
    if (!ok) {
      failures++;
    }
    Log.i(TAG, (ok ? "ok   " : "FAIL ") + what);
  }

  private void report() {
    Log.i(TAG, failures == 0 ? "=== ALL PASSED ===" : "=== FAILED: " + failures + " ===");
  }
}
