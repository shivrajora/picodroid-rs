// SPDX-License-Identifier: GPL-3.0-only
package layoutdemo;

import picodroid.app.Activity;
import picodroid.content.ComponentName;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.pm.PackageManager;
import picodroid.content.res.ColorStateList;
import picodroid.content.res.Configuration;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.os.Bundle;
import picodroid.os.SystemClock;
import picodroid.util.AttributeSet;
import picodroid.util.DisplayMetrics;
import picodroid.util.Log;
import picodroid.view.AsyncLayoutInflater;
import picodroid.view.Gravity;
import picodroid.view.KeyCharacterMap;
import picodroid.view.KeyEvent;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;
import picodroid.widget.GridLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.RelativeLayout;
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

  /** How many {@link Gauge}s the inflater has constructed, and whether each got an empty set. */
  static int gauges;

  static boolean emptyAttrs = true;

  /** {@link Gauge}'s constructor reports here: the inflater made one, with these attributes. */
  static void gaugeMade(AttributeSet attrs) {
    gauges++;
    if (attrs == null || attrs.getAttributeCount() != 0) {
      emptyAttrs = false;
    }
  }

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    // No setTheme: the manifest's android:theme is applied before onCreate (F4).
    check("manifest theme: colorPrimary", Theme.colorPrimary == getColor(R.color.accent));
    check("manifest theme: textColorPrimary", Theme.colorText == getColor(R.color.ink));
    check("manifest theme: colorSurface", Theme.colorSurface == getColor(R.color.panel));

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

    checkVocabulary();
    checkGrid();
    checkRelativeCode();
    checkRelativeXml();
    checkLinearGravity();
    checkConfiguration();
    buildFlow();

    FrameLayout stages = findViewById(R.id.stages);
    LayoutInflater inflater = getLayoutInflater();
    check("the Activity is the inflater's factory", inflater.getFactory() == this);
    View stage = inflater.inflate(R.layout.stage, stages, false);
    check("inflate made the custom views by reflection", gauges == 2 && emptyAttrs);
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
              checkFlow();
              report();
              // The verdict is out; the benchmark gets a main-loop turn of its own.
              getMainExecutor().execute(() -> benchInflate());
            });
    returned[0] = true;
  }

  /**
   * The rest of the vocabulary a layout written once needs (docs/designs/app-portability-2026-10.md
   * D4): a weighted Space, minimum and maximum sizes, INVISIBLE keeping its room, and a column's
   * children at the start of the cross axis.
   */
  private void checkVocabulary() {
    View vocab = findViewById(R.id.vocab);
    View spacer = findViewById(R.id.spacer);
    View rightBox = findViewById(R.id.right_box);
    check("Space: weighted, it fills the gap", spacer.getWidth() == vocab.getWidth() - 20);
    check("Space: the neighbour sits at the far edge", rightBox.getLeft() == vocab.getWidth() - 10);

    View floored = findViewById(R.id.floored);
    check(
        "minWidth/minHeight floor a wrap_content view",
        floored.getWidth() == 30 && floored.getHeight() == 12);
    check(
        "getMinimumWidth/Height",
        floored.getMinimumWidth() == 30 && floored.getMinimumHeight() == 12);
    TextView capped = findViewById(R.id.capped);
    check("maxWidth caps a wrap_content label", capped.getWidth() > 0 && capped.getWidth() <= 40);
    check("getMaxWidth", capped.getMaxWidth() == 40);

    View ghost = findViewById(R.id.ghost);
    View afterGhost = findViewById(R.id.after_ghost);
    check("INVISIBLE from the layout", ghost.getVisibility() == View.INVISIBLE);
    check("INVISIBLE keeps its room", afterGhost.getLeft() == ghost.getLeft() + 10);
    ghost.setVisibility(View.GONE);
    check("GONE gives it up", afterGhost.getLeft() == capped.getLeft() + capped.getWidth());
    ghost.setVisibility(View.INVISIBLE);
    check("INVISIBLE again takes it back", afterGhost.getLeft() == ghost.getLeft() + 10);

    View atStart = findViewById(R.id.at_start);
    check("a column's child sits at the start of the cross axis", atStart.getLeft() == 0);
  }

  /**
   * A GridLayout, inflated and then built in code to the same shape: placement wraps at the column
   * count, a span, a weighted track that takes the free width, a cell aligned right, a cell named
   * outright.
   */
  private void checkGrid() {
    GridLayout grid = findViewById(R.id.grid);
    checkGridGeometry(
        "grid xml",
        grid,
        findViewById(R.id.g00),
        findViewById(R.id.g01),
        findViewById(R.id.g02),
        findViewById(R.id.g10),
        findViewById(R.id.g12),
        findViewById(R.id.g22),
        findViewById(R.id.g31));
    ViewGroup.LayoutParams lp = findViewById(R.id.g10).getLayoutParams();
    check(
        "grid xml: GridLayout.LayoutParams with the span",
        lp instanceof GridLayout.LayoutParams
            && ((GridLayout.LayoutParams) lp).columnSpec.getSize() == 2
            && ((GridLayout.LayoutParams) lp).columnSpec.getStart() == 0
            && ((GridLayout.LayoutParams) lp).rowSpec.getStart() == 1);
    boolean threw = false;
    try {
      grid.setColumnCount(4);
    } catch (IllegalStateException e) {
      threw = true;
    }
    check("grid: setColumnCount after a child throws", threw);

    // The same grid from code, under the inflated one.
    GridLayout coded = new GridLayout(this);
    coded.setColumnCount(3);
    View[] boxes = new View[7];
    for (int i = 0; i < boxes.length; i++) {
      boxes[i] = new FrameLayout(this);
      boxes[i].setBackgroundColor(getColor(R.color.accent));
    }
    GridLayout.LayoutParams plain =
        box(GridLayout.spec(GridLayout.UNDEFINED), GridLayout.spec(GridLayout.UNDEFINED));
    coded.addView(boxes[0], plain);
    coded.addView(
        boxes[1],
        box(GridLayout.spec(GridLayout.UNDEFINED), GridLayout.spec(GridLayout.UNDEFINED)));
    coded.addView(
        boxes[2],
        box(GridLayout.spec(GridLayout.UNDEFINED), GridLayout.spec(GridLayout.UNDEFINED)));
    coded.addView(
        boxes[3],
        box(GridLayout.spec(GridLayout.UNDEFINED), GridLayout.spec(GridLayout.UNDEFINED, 2)));
    coded.addView(
        boxes[4],
        box(
            GridLayout.spec(GridLayout.UNDEFINED),
            GridLayout.spec(GridLayout.UNDEFINED, 1, GridLayout.FILL, 1f)));
    coded.addView(boxes[5], box(GridLayout.spec(2), GridLayout.spec(2, GridLayout.RIGHT)));
    coded.addView(boxes[6], box(GridLayout.spec(3), GridLayout.spec(1)));
    LinearLayout root = findViewById(R.id.root);
    int at = 0;
    while (root.getChildAt(at) != grid) {
      at++;
    }
    root.addView(
        coded,
        at + 1,
        new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    checkGridGeometry(
        "grid code", coded, boxes[0], boxes[1], boxes[2], boxes[3], boxes[4], boxes[5], boxes[6]);
    check("grid code: getRowCount counts the rows in use", coded.getRowCount() == 4);
    // Checked; out of the way. The stages below need the column's full height on a 240 px
    // panel for their `fit ok`, and GONE gives the room back (removeView frees the coded one).
    grid.setVisibility(View.GONE);
    root.removeView(coded);
  }

  /**
   * A RelativeLayout built in code, 200x60: the picoclock header (a caption right of the back
   * button, centred on it), a corner pinned by margins, a child centred in the parent, one below
   * and left-aligned with the caption, and a match_parent child stretched between two siblings.
   */
  private void checkRelativeCode() {
    RelativeLayout rel = new RelativeLayout(this);
    View back = relBox(rel, 1001, 48, 30, RelativeLayout.ALIGN_PARENT_LEFT, 0);
    RelativeLayout.LayoutParams captionLp = new RelativeLayout.LayoutParams(60, 16);
    captionLp.addRule(RelativeLayout.RIGHT_OF, 1001);
    captionLp.addRule(RelativeLayout.CENTER_VERTICAL);
    View caption = relBox(rel, 1002, captionLp);
    RelativeLayout.LayoutParams cornerLp = new RelativeLayout.LayoutParams(10, 10);
    cornerLp.addRule(RelativeLayout.ALIGN_PARENT_RIGHT);
    cornerLp.addRule(RelativeLayout.ALIGN_PARENT_BOTTOM);
    cornerLp.setMargins(0, 0, 4, 6);
    View corner = relBox(rel, 1003, cornerLp);
    View centre = relBox(rel, 1004, 20, 10, RelativeLayout.CENTER_IN_PARENT, 0);
    RelativeLayout.LayoutParams underLp = new RelativeLayout.LayoutParams(10, 10);
    underLp.addRule(RelativeLayout.BELOW, 1002);
    underLp.addRule(RelativeLayout.ALIGN_LEFT, 1002);
    View under = relBox(rel, 1005, underLp);
    RelativeLayout.LayoutParams stretchLp =
        new RelativeLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 8);
    stretchLp.addRule(RelativeLayout.RIGHT_OF, 1001);
    stretchLp.addRule(RelativeLayout.LEFT_OF, 1003);
    stretchLp.addRule(RelativeLayout.ALIGN_PARENT_BOTTOM);
    View stretch = relBox(rel, 1006, stretchLp);
    LinearLayout root = findViewById(R.id.root);
    root.addView(rel, new LinearLayout.LayoutParams(200, 60));
    checkRelative("relative code", back, caption, corner, centre, under, stretch);
    ViewGroup.LayoutParams lp = stretch.getLayoutParams();
    check(
        "relative code: getRules",
        lp instanceof RelativeLayout.LayoutParams
            && ((RelativeLayout.LayoutParams) lp).getRule(RelativeLayout.RIGHT_OF) == 1001
            && ((RelativeLayout.LayoutParams) lp).getRule(RelativeLayout.ALIGN_PARENT_BOTTOM)
                == RelativeLayout.TRUE
            && ((RelativeLayout.LayoutParams) lp).getRules().length == 22);
    // A late rule: the layout follows.
    cornerLp.removeRule(RelativeLayout.ALIGN_PARENT_RIGHT);
    cornerLp.addRule(RelativeLayout.ALIGN_PARENT_LEFT);
    rel.removeView(corner);
    corner = relBox(rel, 1003, cornerLp);
    check("relative code: a rule changed", corner.getLeft() == 0 && corner.getTop() == 44);
    root.removeView(rel);
  }

  /**
   * Per-child layout_gravity in a LinearLayout and Android's weights, from XML and from code: a row
   * places each child across its height by the child's own vertical bits (horizontal bits are
   * ignored), a column by the horizontal ones; a weighted Space takes the rest; match_parent fills
   * within its margins; a weight shares what is left sequentially; GONE gives its room up.
   */
  private void checkLinearGravity() {
    LinearLayout row = findViewById(R.id.gravity_row);
    int w = row.getWidth();
    View top = findViewById(R.id.gr_top);
    View centre = findViewById(R.id.gr_centre);
    View bottom = findViewById(R.id.gr_bottom);
    View right = findViewById(R.id.gr_right);
    View fill = findViewById(R.id.gr_fill);
    View end = findViewById(R.id.gr_end);
    check("linear gravity: a child without one sits at the top", top.getTop() == 0);
    check("linear gravity: center_vertical", centre.getTop() == 10 && centre.getLeft() == 10);
    check("linear gravity: bottom", bottom.getTop() == 20 && bottom.getLeft() == 20);
    check(
        "linear gravity: a row ignores the horizontal bits",
        right.getTop() == 0 && right.getLeft() == 30);
    check("linear gravity: the weighted Space takes the rest", fill.getWidth() == w - 50);
    check(
        "linear gravity: bottom in by the margin, at the end",
        end.getLeft() == w - 10 && end.getTop() == 17);
    centre.setVisibility(View.GONE);
    check("linear gravity: GONE gives its room up", bottom.getLeft() == 10);
    centre.setVisibility(View.VISIBLE);
    check("linear gravity: VISIBLE takes it back", bottom.getLeft() == 20);

    LinearLayout column = findViewById(R.id.gravity_column);
    View cRight = findViewById(R.id.gc_right);
    View cCentre = findViewById(R.id.gc_centre);
    View cFill = findViewById(R.id.gc_fill);
    check("linear gravity: right in a column", cRight.getLeft() == w - 10);
    check("linear gravity: center_horizontal in a column", cCentre.getLeft() == (w - 20) / 2);
    check(
        "linear gravity: match_parent within its margins",
        cFill.getLeft() == 8 && cFill.getWidth() == w - 12);
    check("linear gravity: wrap_content column", column.getHeight() == 30);

    // Weights from code, in a 100 px row: 1.5 : 1, and content plus a share.
    LinearLayout coded = new LinearLayout(this);
    coded.setOrientation(LinearLayout.HORIZONTAL);
    View a = new FrameLayout(this);
    View b = new FrameLayout(this);
    coded.addView(a, new LinearLayout.LayoutParams(0, 10, 1.5f));
    coded.addView(b, new LinearLayout.LayoutParams(0, 10, 1f));
    LinearLayout root = findViewById(R.id.root);
    root.addView(coded, new LinearLayout.LayoutParams(100, 10));
    check("linear weight: 1.5 : 1 of 100", a.getWidth() == 60 && b.getWidth() == 40);
    coded.removeView(a);
    coded.removeView(b);
    View fixed = new FrameLayout(this);
    View grown = new FrameLayout(this);
    LinearLayout.LayoutParams grownLp = new LinearLayout.LayoutParams(20, 10, 1f);
    grownLp.gravity = Gravity.CENTER_VERTICAL;
    coded.addView(fixed, new LinearLayout.LayoutParams(40, 10));
    coded.addView(grown, grownLp);
    check(
        "linear weight: a measured size plus its share",
        grown.getLeft() == 40 && grown.getWidth() == 60);
    check(
        "linear gravity: LayoutParams.gravity from code",
        ((LinearLayout.LayoutParams) grown.getLayoutParams()).gravity == Gravity.CENTER_VERTICAL);
    root.removeView(coded);
    // Checked; out of the way, as the grids were.
    row.setVisibility(View.GONE);
    column.setVisibility(View.GONE);
  }

  /** The geometry both RelativeLayouts must show (200x60). */
  private void checkRelative(
      String how, View back, View caption, View corner, View centre, View under, View stretch) {
    check(how + ": alignParentLeft", back.getLeft() == 0 && back.getTop() == 0);
    check(
        how + ": toRightOf + centerVertical",
        caption.getLeft() == 48 && caption.getTop() == (60 - 16) / 2);
    check(
        how + ": alignParentRight|Bottom in by the margins",
        corner.getLeft() == 200 - 10 - 4 && corner.getTop() == 60 - 10 - 6);
    check(how + ": centerInParent", centre.getLeft() == 90 && centre.getTop() == 25);
    check(how + ": below + alignLeft", under.getLeft() == 48 && under.getTop() == 22 + 16);
    check(
        how + ": match_parent between two siblings",
        stretch.getLeft() == 48 && stretch.getWidth() == 186 - 48 && stretch.getTop() == 52);
  }

  /** The same layout from XML, through the inflater's native path. */
  private void checkRelativeXml() {
    LinearLayout root = findViewById(R.id.root);
    View rel = getLayoutInflater().inflate(R.layout.relative, root, false);
    check("relative xml: a RelativeLayout", rel instanceof RelativeLayout);
    root.addView(rel, new LinearLayout.LayoutParams(200, 60));
    checkRelative(
        "relative xml",
        rel.findViewById(R.id.rel_back),
        rel.findViewById(R.id.rel_caption),
        rel.findViewById(R.id.rel_corner),
        rel.findViewById(R.id.rel_centre),
        rel.findViewById(R.id.rel_under),
        rel.findViewById(R.id.rel_stretch));
    ViewGroup.LayoutParams lp = rel.findViewById(R.id.rel_stretch).getLayoutParams();
    check(
        "relative xml: RelativeLayout.LayoutParams with the rules",
        lp instanceof RelativeLayout.LayoutParams
            && ((RelativeLayout.LayoutParams) lp).getRule(RelativeLayout.RIGHT_OF) == R.id.rel_back
            && ((RelativeLayout.LayoutParams) lp).getRule(RelativeLayout.LEFT_OF) == R.id.rel_corner
            && ((RelativeLayout.LayoutParams) lp).getRule(RelativeLayout.ALIGN_PARENT_BOTTOM)
                == RelativeLayout.TRUE);
    root.removeView(rel);
  }

  private View relBox(RelativeLayout rel, int id, int w, int h, int verb, int subject) {
    RelativeLayout.LayoutParams lp = new RelativeLayout.LayoutParams(w, h);
    if (subject != 0) {
      lp.addRule(verb, subject);
    } else {
      lp.addRule(verb);
    }
    return relBox(rel, id, lp);
  }

  private View relBox(RelativeLayout rel, int id, RelativeLayout.LayoutParams lp) {
    View v = new FrameLayout(this);
    v.setId(id);
    v.setBackgroundColor(getColor(R.color.accent));
    rel.addView(v, lp);
    return v;
  }

  private static GridLayout.LayoutParams box(GridLayout.Spec row, GridLayout.Spec column) {
    GridLayout.LayoutParams lp = new GridLayout.LayoutParams(row, column);
    lp.width = 10;
    lp.height = 10;
    return lp;
  }

  private void checkGridGeometry(
      String how,
      GridLayout grid,
      View g00,
      View g01,
      View g02,
      View g10,
      View g12,
      View g22,
      View g31) {
    int w = grid.getWidth();
    check(how + ": the grid takes the row's width", w > 30 && grid.getHeight() == 40);
    check(
        how + ": placement runs along the row",
        g00.getLeft() == 0 && g01.getLeft() == 10 && g02.getLeft() == 20 && g02.getTop() == 0);
    check(how + ": ...and wraps at the column count", g10.getLeft() == 0 && g10.getTop() == 10);
    check(
        how + ": a span keeps the child's own size at the start",
        g10.getWidth() == 10 && g12.getLeft() == 20);
    check(
        how + ": a weighted track takes the free width, fill stretches the cell",
        g12.getWidth() == w - 20 && g12.getTop() == 10);
    check(
        how + ": a cell named outright, aligned right",
        g22.getLeft() == w - 10 && g22.getTop() == 20 && g22.getWidth() == 10);
    check(how + ": a cell named outright", g31.getLeft() == 10 && g31.getTop() == 30);
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

  /**
   * What the app can learn about the board it landed on: the window's size and the input profile
   * ({@code Resources.getConfiguration()}), and which keys exist ({@code
   * KeyCharacterMap.deviceHasKey}). Logged in one line each so a sim row can assert the exact
   * values per board; the checks hold on every board.
   */
  private void checkConfiguration() {
    Configuration c = getResources().getConfiguration();
    Log.i(TAG, "config " + c);
    check(
        "config: window size",
        c.screenWidthDp == getDisplay().getWidth() && c.screenHeightDp == getDisplay().getHeight());
    check(
        "config: smallest side",
        c.smallestScreenWidthDp == Math.min(c.screenWidthDp, c.screenHeightDp));
    check("config: floor", c.smallestScreenWidthDp >= 240);
    check(
        "config: orientation",
        c.orientation
            == (c.screenWidthDp > c.screenHeightDp
                ? Configuration.ORIENTATION_LANDSCAPE
                : Configuration.ORIENTATION_PORTRAIT));
    check("config: density", c.densityDpi == DisplayMetrics.DENSITY_DEFAULT);
    // Touch mode and the navigation field are two views of the same board fact.
    check(
        "config: navigation vs touch mode",
        (c.navigation == Configuration.NAVIGATION_DPAD)
            == !findViewById(R.id.first).isInTouchMode());
    check(
        "config: touchscreen vs feature",
        (c.touchscreen == Configuration.TOUCHSCREEN_FINGER)
            == getPackageManager().hasSystemFeature(PackageManager.FEATURE_TOUCHSCREEN));
    check("config: a copy is equal", new Configuration(c).toString().equals(c.toString()));
    check(
        "config: defaults are undefined",
        new Configuration().orientation == Configuration.ORIENTATION_UNDEFINED);

    boolean back = KeyCharacterMap.deviceHasKey(KeyEvent.KEYCODE_BACK);
    boolean home = KeyCharacterMap.deviceHasKey(KeyEvent.KEYCODE_HOME);
    boolean up = KeyCharacterMap.deviceHasKey(KeyEvent.KEYCODE_DPAD_UP);
    boolean center = KeyCharacterMap.deviceHasKey(KeyEvent.KEYCODE_DPAD_CENTER);
    Log.i(TAG, "keys back=" + back + " home=" + home + " up=" + up + " center=" + center);
    // Every board has BACK one way or another (a key, or the on-screen control).
    check("keys: BACK everywhere", back);
    // The four navigation keys come together, and only with DPAD navigation.
    check(
        "keys: dpad matches navigation",
        up == center && up == (c.navigation == Configuration.NAVIGATION_DPAD));
    check("keys: no such key", !KeyCharacterMap.deviceHasKey(KeyEvent.KEYCODE_BUTTON_A));
    boolean[] both =
        KeyCharacterMap.deviceHasKeys(new int[] {KeyEvent.KEYCODE_BACK, KeyEvent.KEYCODE_BUTTON_A});
    check("keys: deviceHasKeys", both.length == 2 && both[0] && !both[1]);
  }

  /** How many times {@link #benchInflate} inflates the benchmark layout. */
  private static final int BENCH_ROUNDS = 3;

  /** The benchmark's running totals: views counted and microseconds inflating. */
  private int benchViews;

  private long benchUs;

  /**
   * What a view costs to inflate: {@code R.layout.bench} (forty-five nodes, the attributes a page's
   * views carry) inflated {@link #BENCH_ROUNDS} times, each round on a main-loop turn of its own so
   * the runtime's span report (bytecodes, natives) covers exactly one inflation; the tree is
   * counted and closed on the turn after, before the next round, so one tree is alive at a time.
   * The line logged at the end is the figure docs/designs/ui-build-budget-2026-10.md tracks.
   */
  private void benchInflate() {
    benchViews = 0;
    benchUs = 0;
    benchRound(0);
  }

  private void benchRound(int round) {
    long start = SystemClock.elapsedRealtimeNanos();
    final View tree = getLayoutInflater().inflate(R.layout.bench, null);
    benchUs += (SystemClock.elapsedRealtimeNanos() - start) / 1000L;
    getMainExecutor()
        .execute(
            () -> {
              benchViews += countViews(tree);
              tree.close();
              if (round + 1 < BENCH_ROUNDS) {
                getMainExecutor().execute(() -> benchRound(round + 1));
              } else {
                Log.i(
                    TAG,
                    "inflate bench: "
                        + benchViews
                        + " views, "
                        + benchUs
                        + " us, "
                        + (benchUs / benchViews)
                        + " us/view");
              }
            });
  }

  private static int countViews(View v) {
    int n = 1;
    if (v instanceof ViewGroup) {
      ViewGroup g = (ViewGroup) v;
      for (int i = 0; i < g.getChildCount(); i++) {
        n += countViews(g.getChildAt(i));
      }
    }
    return n;
  }

  // ── A custom ViewGroup: FlowLayout, written as on Android ──────────────────────────────────

  private FlowLayout flow;
  private final TextView[] flowItems = new TextView[5];

  /**
   * Five labels in a 100 px wide FlowLayout, in the free corner of the stages frame: they wrap into
   * rows, and onLayout runs on a later main-loop turn.
   */
  private void buildFlow() {
    FrameLayout stages = findViewById(R.id.stages);
    flow = new FlowLayout(this);
    for (int i = 0; i < flowItems.length; i++) {
      TextView item = new TextView(); // no Context of its own: getContext() asks the parent
      item.setText("item" + i);
      flowItems[i] = item;
      flow.addView(
          item,
          new ViewGroup.LayoutParams(
              ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    }
    stages.addView(flow, new FrameLayout.LayoutParams(100, 48, Gravity.RIGHT | Gravity.TOP));
    check("flow: onLayout waits for the main loop", flow.layouts == 0);
    check("flow: getContext through the parent", flowItems[0].getContext() == this);
  }

  /** After a few main-loop turns: the rows are laid out where onLayout put them. */
  private void checkFlow() {
    Log.i(
        TAG,
        "flow: "
            + flow.layouts
            + " layouts, "
            + flow.rows
            + " rows, item1 at "
            + flowItems[1].getLeft()
            + ","
            + flowItems[1].getTop()
            + ", item4 at "
            + flowItems[4].getLeft()
            + ","
            + flowItems[4].getTop());
    check("flow: onLayout ran", flow.layouts >= 1);
    check(
        "flow: children abut on the first row",
        flowItems[0].getLeft() == 0
            && flowItems[1].getLeft() == flowItems[0].getWidth()
            && flowItems[1].getTop() == 0);
    check("flow: the rows wrapped", flow.rows > 1 && flowItems[4].getTop() > 0);
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
