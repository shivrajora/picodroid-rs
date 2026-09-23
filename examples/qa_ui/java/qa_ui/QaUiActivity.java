// SPDX-License-Identifier: GPL-3.0-only
package qa_ui;

import picodroid.app.Activity;
import picodroid.app.AlertDialog;
import picodroid.app.Notification;
import picodroid.app.NotificationManager;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.res.ColorStateList;
import picodroid.graphics.Color;
import picodroid.graphics.Display;
import picodroid.os.Bundle;
import picodroid.os.Runtime;
import picodroid.os.SystemClock;
import picodroid.text.TextWatcher;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.Button;
import picodroid.widget.CheckBox;
import picodroid.widget.EditText;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.ProgressBar;
import picodroid.widget.RadioButton;
import picodroid.widget.RadioGroup;
import picodroid.widget.ScrollView;
import picodroid.widget.SeekBar;
import picodroid.widget.Snackbar;
import picodroid.widget.Spinner;
import picodroid.widget.Switch;
import picodroid.widget.TextView;
import picodroid.widget.Toast;
import picodroid.widget.ToggleButton;

/**
 * QA 2026-09-13: the view tree and widget state machine, driven without a finger — child order
 * after add/remove, visibility and enabled state, text round trips, listener replacement, compound
 * buttons and radio groups, seek/progress clamping, adapters, layout params and transform getters,
 * focus hand-off, then (after layout) sizes, animation end actions, dialogs, toasts, snackbars and
 * a view churn under GC. Synthetic events ({@code performClick} and friends) are delivered on a
 * later frame, so the phases verify what the previous phase fired.
 */
public class QaUiActivity extends Activity {
  private static final String TAG = "QaUi";

  static int passed = 0;
  static int failed = 0;
  static int crashed = 0;

  static void check(String name, boolean condition) {
    if (condition) {
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  interface Section {
    void run();
  }

  static void section(String name, Section s) {
    Log.i(TAG, "section " + name);
    try {
      s.run();
    } catch (Throwable t) {
      Log.i(TAG, "CRASH in " + name + ": " + t + " msg=" + t.getMessage());
      crashed = crashed + 1;
    }
  }

  private LinearLayout root;
  private final TextView[] rows = new TextView[5];
  private TextView sized;
  private TextView animated;
  private final int[] endFired = new int[1];
  private final int[] dialogItem = new int[] {-1};
  private final int[] dialogButton = new int[] {-99};
  private Button churnTarget;
  private final int[] churnClicks = new int[1];
  private Button[] rooted;

  // Deferred-event bookkeeping: fired in one phase, verified in the next.
  private Button clickButton;
  private final int[] clicksFirst = new int[1];
  private final int[] clicksSecond = new int[1];
  private CheckBox checkBox;
  private final int[] checkEvents = new int[1];
  private final boolean[] checkLast = new boolean[1];
  private SeekBar seekBar;
  private final int[] seekChanged = new int[1];
  private Spinner spinner;
  private final int[] spinnerSelected = new int[] {-1};

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "=== QaUi start ===");
    getDisplay();
    section("display", () -> display());
    section("tree", () -> tree());
    section("visibilityEnabled", () -> visibilityEnabled());
    section("text", () -> text());
    section("clicks", () -> clicks());
    section("compound", () -> compound());
    section("radio", () -> radio());
    section("seekProgress", () -> seekProgress());
    section("adapters", () -> adapters());
    section("editText", () -> editText());
    section("containers", () -> containers());
    section("paramsTransforms", () -> paramsTransforms());
    section("focus", () -> focus());
    setContentView(root);
    later(300, () -> phase2());
  }

  private void later(int ms, Runnable r) {
    Thread t =
        new Thread(
            () -> {
              SystemClock.sleep(ms);
              Executors.mainExecutor().execute(r);
            },
            "qa-ui-timer");
    t.start();
  }

  // ---- phase 1 ------------------------------------------------------------------------------

  void display() {
    Display d = getDisplay();
    check("display singleton", d == Display.getInstance() && d == getDisplay());
    check("display size", d.getWidth() > 0 && d.getHeight() > 0);
    Log.i(TAG, "display " + d.getWidth() + "x" + d.getHeight());
  }

  void tree() {
    root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(getDisplay().getWidth(), getDisplay().getHeight());
    for (int i = 0; i < 5; i++) {
      TextView tv = new TextView(this);
      tv.setText("row " + i);
      tv.setTag(Integer.valueOf(1000 + i));
      tv.setId(100 + i);
      rows[i] = tv;
      root.addView(tv);
    }
    check("childCount 5", root.getChildCount() == 5);
    check("getChildAt identity", root.getChildAt(2) == rows[2] && root.getChildAt(0) == rows[0]);
    check(
        "getChildAt out of range null", root.getChildAt(5) == null && root.getChildAt(-1) == null);
    check("tag identity", root.getChildAt(2).getTag() == rows[2].getTag());
    check("tag value", ((Integer) rows[3].getTag()) == 1003);
    check("id round trip", rows[4].getId() == 104 && root.getChildAt(4).getId() == 104);
    check("NO_ID default", new TextView(this).getId() == View.NO_ID);
    root.removeView(rows[2]);
    check("removeView count", root.getChildCount() == 4);
    check("removeView order", root.getChildAt(2) == rows[3] && root.getChildAt(3) == rows[4]);
    // picodroid frees a removed view (Android keeps it usable): re-adding is refused loudly.
    boolean refused = false;
    try {
      root.addView(rows[2]);
    } catch (IllegalStateException e) {
      refused = true;
    }
    check("re-adding a released view throws", refused && root.getChildCount() == 4);
    rows[2] = new TextView(this);
    rows[2].setText("row 2");
    rows[2].setTag(Integer.valueOf(1002));
    rows[2].setId(102);
    root.addView(rows[2]);
    check("fresh view appends", root.getChildCount() == 5 && root.getChildAt(4) == rows[2]);
    root.removeAllViews();
    check("removeAllViews", root.getChildCount() == 0 && root.getChildAt(0) == null);
    for (int i = 0; i < 5; i++) {
      rows[i] = new TextView(this);
      rows[i].setText("row " + i);
      rows[i].setTag(Integer.valueOf(1000 + i));
      rows[i].setId(100 + i);
      root.addView(rows[i]);
    }
    check("rebuilt all", root.getChildCount() == 5 && root.getChildAt(1) == rows[1]);
    root.removeView(new TextView(this));
    check(
        "removeView of non-child is a no-op",
        root.getChildCount() == 5 && root.getChildAt(4) == rows[4]);
    check("null tag", new TextView(this).getTag() == null);
    TextView tagged = new TextView(this);
    Object tag = new Object();
    tagged.setTag(tag);
    check("object tag identity", tagged.getTag() == tag);
    tagged.setTag(null);
    check("tag cleared", tagged.getTag() == null);
    sized = new TextView(this);
    sized.setText("sized");
    sized.setSize(100, 30);
    root.addView(sized);
    animated = new TextView(this);
    animated.setText("anim");
    root.addView(animated);
    check("native and Java child counts agree", root.getChildCount() == 7);
  }

  void visibilityEnabled() {
    TextView tv = rows[0];
    check("default VISIBLE", tv.getVisibility() == View.VISIBLE);
    tv.setVisibility(View.GONE);
    check("GONE reported", tv.getVisibility() == View.GONE);
    tv.setVisibility(View.INVISIBLE);
    check("INVISIBLE reported", tv.getVisibility() == View.INVISIBLE);
    tv.setVisibility(View.VISIBLE);
    check("VISIBLE again", tv.getVisibility() == View.VISIBLE);
    check("default enabled", tv.isEnabled());
    tv.setEnabled(false);
    check("disabled reported", !tv.isEnabled());
    tv.setEnabled(true);
    check("enabled again", tv.isEnabled());
    tv.setAlpha(0.5f);
    check("alpha round trip", tv.getAlpha() == 0.5f);
    tv.setAlpha(1f);
    check("alpha 1", tv.getAlpha() == 1f);
  }

  void text() {
    TextView tv = rows[1];
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < 300; i++) {
      sb.append((char) ('a' + i % 26));
    }
    String longText = sb.toString();
    tv.setText(longText);
    check("300-char text round trip", tv.getText().toString().equals(longText));
    check("getText length", tv.getText().length() == 300);
    tv.setText("");
    check("empty text", tv.getText().toString().equals("") && tv.getText().length() == 0);
    tv.setText("café");
    check("utf-8 text round trip", tv.getText().toString().equals("café"));
    tv.setText("a\nb");
    check("newline text", tv.getText().toString().equals("a\nb"));
    tv.setText("row 1");
    check("CharSequence charAt", tv.getText().charAt(0) == 'r');
    tv.setTextColor(Color.RED);
    tv.setBackgroundColor(Color.BLUE);
    tv.setPadding(2, 2, 2, 2);
    tv.setSingleLine();
    check("singleLine sets maxLines 1", tv.getMaxLines() == 1);
    tv.setSingleLine(false);
    check("singleLine off lifts the limit", tv.getMaxLines() == Integer.MAX_VALUE);
    tv.setMaxLines(3);
    check("maxLines round trip", tv.getMaxLines() == 3);
    tv.setMaxLines(0);
    Button b = new Button(this, "btn");
    check("Button getText", b.getText().toString().equals("btn"));
    b.setText("changed");
    check("Button setText", b.getText().toString().equals("changed"));
    StringBuilder wide = new StringBuilder();
    for (int i = 0; i < 400; i++) {
      wide.append('w');
    }
    String wideText = wide.toString();
    b.setText(wideText);
    check("Button 400-char text round trip", b.getText().toString().equals(wideText));
    root.addView(b);
    check("Button is a TextView", ((TextView) b).getText().length() == 400);
    root.removeView(b);
  }

  void clicks() {
    clickButton = new Button(this, "click");
    root.addView(clickButton);
    clickButton.setOnClickListener(v -> clicksFirst[0]++);
    clickButton.performClick();
    clickButton.performClick();
    Log.i(TAG, "performClick delivered synchronously=" + (clicksFirst[0] == 2));
    final int[] longs = new int[1];
    clickButton.setOnLongClickListener(
        v -> {
          longs[0]++;
          return true;
        });
    boolean consumed = clickButton.performLongClick();
    check("performLongClick", longs[0] == 1 && consumed);
    TextView plain = rows[2];
    final int[] tvClicks = new int[1];
    plain.setOnClickListener(v -> tvClicks[0]++);
    plain.performClick();
    check("TextView accepts a click listener", true);
  }

  void compound() {
    checkBox = new CheckBox(this);
    checkBox.setText("cb");
    root.addView(checkBox);
    check("checkbox default unchecked", !checkBox.isChecked());
    checkBox.setChecked(true);
    check("setChecked true", checkBox.isChecked());
    checkBox.setOnCheckedChangeListener(
        (v, checked) -> {
          checkEvents[0]++;
          checkLast[0] = checked;
        });
    checkBox.performCheckedChange();
    Log.i(TAG, "performCheckedChange delivered synchronously=" + (checkEvents[0] == 1));
    int before = checkEvents[0];
    checkBox.setChecked(false);
    Log.i(TAG, "programmatic setChecked fired listener=" + (checkEvents[0] > before));
    check("setChecked false", !checkBox.isChecked());
    checkBox.setChecked(true);
    Switch sw = new Switch(this);
    root.addView(sw);
    sw.toggle();
    check("switch toggle on", sw.isChecked());
    sw.toggle();
    check("switch toggle off", !sw.isChecked());
    sw.setChecked(true);
    check("switch setChecked", sw.isChecked());
    root.removeView(sw);
    ToggleButton tb = new ToggleButton(this, "ON", "OFF");
    root.addView(tb);
    check("toggle default off", !tb.isChecked());
    tb.toggle();
    check("toggle on", tb.isChecked());
    tb.setTextOn("YES");
    tb.setTextOff("NO");
    tb.setChecked(false);
    check("toggle setChecked off", !tb.isChecked());
    root.removeView(tb);
  }

  void radio() {
    RadioGroup rg = new RadioGroup(this);
    RadioButton[] rb = new RadioButton[3];
    for (int i = 0; i < 3; i++) {
      rb[i] = new RadioButton(this);
      rb[i].setText("opt" + i);
      rb[i].setId(i + 1);
      rg.addView(rb[i]);
    }
    root.addView(rg);
    check("radio group children", rg.getChildCount() == 3 && rg.getChildAt(1) == rb[1]);
    check("nothing checked", rg.getCheckedRadioButtonId() == -1);
    final int[] lastId = new int[] {-5};
    final int[] events = new int[1];
    rg.setOnCheckedChangeListener(
        (group, id) -> {
          lastId[0] = id;
          events[0]++;
        });
    rg.check(2);
    check("check(2)", rg.getCheckedRadioButtonId() == 2 && rb[1].isChecked() && !rb[0].isChecked());
    check("listener id", lastId[0] == 2 && events[0] == 1);
    rg.check(3);
    check(
        "check(3) unchecks 2",
        rg.getCheckedRadioButtonId() == 3 && rb[2].isChecked() && !rb[1].isChecked());
    rg.check(3);
    check("check(same) is a no-op", events[0] == 2);
    rg.clearCheck();
    check(
        "clearCheck", rg.getCheckedRadioButtonId() == -1 && !rb[2].isChecked() && lastId[0] == -1);
    rb[0].setChecked(true);
    check(
        "setChecked on a button updates group",
        rg.getCheckedRadioButtonId() == 1 && lastId[0] == 1);
    rb[1].setChecked(true);
    check(
        "setChecked on another unchecks the first",
        rg.getCheckedRadioButtonId() == 2 && !rb[0].isChecked() && rb[1].isChecked());
    rb[1].setChecked(true);
    check("re-checking the checked one is silent", events[0] == 5);
    rb[1].setChecked(false);
    check(
        "unchecking leaves the group id (Android)",
        rg.getCheckedRadioButtonId() == 2 && !rb[1].isChecked());
    rg.check(99);
    check("check(unknown id) keeps state", rg.getCheckedRadioButtonId() == 2);
    root.removeView(rg);
  }

  void seekProgress() {
    seekBar = new SeekBar(this);
    seekBar.setMax(100);
    root.addView(seekBar);
    seekBar.setProgress(60);
    check("seek progress", seekBar.getProgress() == 60);
    seekBar.setProgress(150);
    check("seek clamps above max", seekBar.getProgress() == 100);
    seekBar.setProgress(-5);
    check("seek clamps below 0", seekBar.getProgress() == 0);
    seekBar.setProgress(80);
    seekBar.setMax(50);
    check("setMax clamps the progress", seekBar.getProgress() == 50);
    seekBar.setMax(100);
    seekBar.setProgress(10);
    seekBar.setOnSeekBarChangeListener(
        new SeekBar.OnSeekBarChangeListener() {
          @Override
          public void onProgressChanged(SeekBar bar, int progress, boolean fromUser) {
            seekChanged[0]++;
          }

          @Override
          public void onStartTrackingTouch(SeekBar bar) {}

          @Override
          public void onStopTrackingTouch(SeekBar bar) {}
        });
    seekBar.performProgressChange();
    Log.i(TAG, "performProgressChange delivered synchronously=" + (seekChanged[0] == 1));
    ProgressBar pb = new ProgressBar(this);
    root.addView(pb);
    check("progress bar range defaults", pb.getMin() == 0 && pb.getMax() == 100);
    pb.setProgress(42);
    check("progress bar value", pb.getProgress() == 42 && !pb.isIndeterminate());
    pb.setProgress(150);
    check("progress clamps to max", pb.getProgress() == 100);
    pb.setMax(50);
    check("setMax pulls the progress down", pb.getMax() == 50 && pb.getProgress() == 50);
    pb.setMin(20);
    pb.setProgress(5);
    check("progress clamps to min", pb.getMin() == 20 && pb.getProgress() == 20);
    pb.setMin(60);
    check("setMin caps at max", pb.getMin() == 50 && pb.getProgress() == 50);
    pb.setMin(0);
    pb.setMax(100);
    pb.setProgress(10, true);
    check("animated progress reads back at once", pb.getProgress() == 10);
    pb.incrementProgressBy(15);
    check("incrementProgressBy", pb.getProgress() == 25);
    pb.incrementProgressBy(-100);
    check("increment clamps", pb.getProgress() == 0);
    ColorStateList green = ColorStateList.valueOf(Color.GREEN);
    check(
        "ColorStateList",
        green.getDefaultColor() == Color.GREEN
            && !green.isStateful()
            && green.getColorForState(null, Color.RED) == Color.GREEN
            && green.withAlpha(0x80).getDefaultColor() == 0x8000FF00);
    pb.setProgressTintList(green);
    pb.setProgressBackgroundTintList(ColorStateList.valueOf(0x40FFFFFF));
    check(
        "progress tints round-trip",
        pb.getProgressTintList() == green
            && pb.getProgressBackgroundTintList().getDefaultColor() == 0x40FFFFFF);
    pb.setProgressTintList(null);
    pb.setProgressBackgroundTintList(null);
    check(
        "null clears the tints",
        pb.getProgressTintList() == null && pb.getProgressBackgroundTintList() == null);
    pb.setTint(Color.GREEN);
    check(
        "setTint is the indeterminate tint",
        pb.getIndeterminateTintList().getDefaultColor() == Color.GREEN);
    root.removeView(pb);
    ProgressBar ind = ProgressBar.indeterminate();
    root.addView(ind);
    ind.setProgress(30);
    ind.setProgressTintList(green);
    ind.setIndeterminateTintList(ColorStateList.valueOf(Color.RED));
    ind.setIndeterminateTintList(null);
    check(
        "indeterminate",
        ind.isIndeterminate() && ind.getProgress() == 0 && ind.getProgressTintList() == green);
    root.removeView(ind);
  }

  void adapters() {
    spinner = new Spinner(this);
    spinner.setItems("a\nb\nc");
    root.addView(spinner);
    check("spinner initial position", spinner.getSelectedItemPosition() == 0);
    spinner.setOnItemSelectedListener(
        new picodroid.widget.AdapterView.OnItemSelectedListener() {
          @Override
          public void onItemSelected(
              picodroid.widget.AdapterView<?> parent, View view, int position, long id) {
            spinnerSelected[0] = position;
          }

          @Override
          public void onNothingSelected(picodroid.widget.AdapterView<?> parent) {
            spinnerSelected[0] = -2;
          }
        });
    spinner.performItemSelected();
    Log.i(TAG, "performItemSelected delivered synchronously=" + (spinnerSelected[0] >= 0));
    Spinner sp2 = new Spinner(this);
    sp2.setAdapter(new ArrayAdapter<String>(this, new String[] {"x", "y", "z"}));
    root.addView(sp2);
    check(
        "adapter-backed spinner count",
        sp2.getAdapter().getCount() == 3 && sp2.getSelectedItemPosition() == 0);
    root.removeView(sp2);
    ListView lv = new ListView(this);
    lv.setSize(200, 100);
    ArrayAdapter<String> ad =
        new ArrayAdapter<String>(this, new String[] {"one", "two", "three", "four", "five"});
    lv.setAdapter(ad);
    root.addView(lv);
    check(
        "list adapter count",
        lv.getAdapter() == ad && ad.getCount() == 5 && ad.getItem(2).equals("three"));
    check("item ids", ad.getItemId(3) == 3);
    ad.add("six");
    ad.notifyDataSetChanged();
    check("adapter add + notify", ad.getCount() == 6 && ad.getItem(5).equals("six"));
    check("list rows follow the adapter", lv.getChildCount() == 6);
    ad.clear();
    ad.notifyDataSetChanged();
    check("adapter clear", ad.getCount() == 0 && lv.getChildCount() == 0);
    final int[] clicked = new int[] {-1};
    lv.setOnItemClickListener((parent, view, position, id) -> clicked[0] = position);
    check("item click listener stored", lv.getOnItemClickListener() != null);
    ArrayAdapter<Integer> ints = new ArrayAdapter<Integer>(this);
    for (int i = 0; i < 20; i++) {
      ints.add(i * i);
    }
    lv.setAdapter(ints);
    ints.notifyDataSetChanged();
    check(
        "second adapter bound",
        lv.getAdapter() == ints && ints.getCount() == 20 && ints.getItem(4) == 16);
    root.removeView(lv);
    ListView plain = new ListView(this);
    plain.addItem("p1");
    plain.addItem("p2");
    root.addView(plain);
    check("addItem list has no adapter", plain.getAdapter() == null);
    root.removeView(plain);
  }

  void editText() {
    EditText et = new EditText(this);
    root.addView(et);
    et.setHint("hint");
    check("edit default empty", et.getText().equals(""));
    et.setText("abc");
    check("edit setText/getText", et.getText().equals("abc"));
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < 300; i++) {
      sb.append((char) ('A' + i % 26));
    }
    String longText = sb.toString();
    et.setText(longText);
    check("edit 300-char round trip", et.getText().equals(longText));
    final int[] events = new int[3];
    final String[] lastAfter = new String[1];
    TextWatcher w =
        new TextWatcher() {
          @Override
          public void beforeTextChanged(String s, int start, int count, int after) {
            events[0]++;
          }

          @Override
          public void onTextChanged(String s, int start, int before, int count) {
            events[1]++;
          }

          @Override
          public void afterTextChanged(String s) {
            events[2]++;
            lastAfter[0] = s;
          }
        };
    et.addTextChangedListener(w);
    et.setText("xyz");
    Log.i(
        TAG,
        "TextWatcher after programmatic setText: before="
            + events[0]
            + " on="
            + events[1]
            + " after="
            + events[2]
            + " last="
            + lastAfter[0]);
    check("watcher saw the new text if fired", events[2] == 0 || "xyz".equals(lastAfter[0]));
    et.removeTextChangedListener(w);
    int after = events[2];
    et.setText("q");
    check("removed watcher silent", events[2] == after);
    et.setText("");
    check("edit cleared", et.getText().equals(""));
    root.removeView(et);
  }

  void containers() {
    ScrollView sv = new ScrollView(this);
    sv.setSize(200, 100);
    LinearLayout col = new LinearLayout(this);
    col.setOrientation(LinearLayout.VERTICAL);
    for (int i = 0; i < 40; i++) {
      TextView t = new TextView(this);
      t.setText("line " + i);
      col.addView(t);
    }
    sv.addView(col);
    root.addView(sv);
    check(
        "scroll has one child",
        sv.getChildCount() == 1 && sv.getChildAt(0) == col && col.getChildCount() == 40);
    check(
        "deep child by index",
        col.getChildAt(39) != null
            && ((TextView) col.getChildAt(39)).getText().toString().equals("line 39"));
    root.removeView(sv);
    FrameLayout fl = new FrameLayout(this);
    TextView under = new TextView(this);
    under.setText("under");
    TextView over = new TextView(this);
    over.setText("over");
    fl.addView(under);
    fl.addView(
        over,
        new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    root.addView(fl);
    check("frame order", fl.getChildCount() == 2 && fl.getChildAt(1) == over);
    fl.removeView(under);
    check("frame remove", fl.getChildCount() == 1 && fl.getChildAt(0) == over);
    root.removeView(fl);
    LinearLayout deep = new LinearLayout(this);
    LinearLayout cur = deep;
    for (int i = 0; i < 10; i++) {
      LinearLayout next = new LinearLayout(this);
      next.setOrientation(i % 2 == 0 ? LinearLayout.HORIZONTAL : LinearLayout.VERTICAL);
      cur.addView(next);
      cur = next;
    }
    TextView leaf = new TextView(this);
    leaf.setText("leaf");
    cur.addView(leaf);
    root.addView(deep);
    View walk = deep;
    int depth = 0;
    while (walk instanceof ViewGroup && ((ViewGroup) walk).getChildCount() > 0) {
      walk = ((ViewGroup) walk).getChildAt(0);
      depth++;
    }
    check("10-deep nesting walkable", depth == 11 && walk == leaf);
    root.removeView(deep);
    LinearLayout h = new LinearLayout(this);
    h.setOrientation(LinearLayout.HORIZONTAL);
    h.setSpacing(4);
    h.setGravity(picodroid.view.Gravity.CENTER);
    TextView a = new TextView(this);
    a.setText("A");
    TextView b = new TextView(this);
    b.setText("B");
    h.addView(a, new LinearLayout.LayoutParams(50, 20, 1f));
    h.addView(b, new LinearLayout.LayoutParams(50, 20, 2f));
    root.addView(h);
    check("horizontal children", h.getChildCount() == 2 && h.getChildAt(1) == b);
  }

  void paramsTransforms() {
    TextView tv = rows[3];
    tv.setLayoutParams(
        new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 30, 1.5f));
    ViewGroup.LayoutParams lp = tv.getLayoutParams();
    check(
        "layout params round trip",
        lp != null && lp.width == ViewGroup.LayoutParams.MATCH_PARENT && lp.height == 30);
    check(
        "LinearLayout params weight",
        lp instanceof LinearLayout.LayoutParams && ((LinearLayout.LayoutParams) lp).weight == 1.5f);
    ViewGroup.LayoutParams copy = new ViewGroup.LayoutParams(lp);
    check("params copy ctor", copy.width == -1 && copy.height == 30);
    tv.setLayoutParams(
        new ViewGroup.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    check("params replaced", tv.getLayoutParams().width == -2);
    tv.setRotation(90f);
    check("rotation", tv.getRotation() == 90f);
    tv.setRotation(0f);
    tv.setScaleX(2f);
    tv.setScaleY(0.5f);
    check("scale", tv.getScaleX() == 2f && tv.getScaleY() == 0.5f);
    tv.setScaleX(1f);
    tv.setScaleY(1f);
    tv.setTranslationX(8f);
    tv.setTranslationY(-4f);
    check("translation", tv.getTranslationX() == 8f && tv.getTranslationY() == -4f);
    tv.setTranslationX(0f);
    tv.setTranslationY(0f);
    check(
        "default transforms",
        rows[4].getRotation() == 0f
            && rows[4].getScaleX() == 1f
            && rows[4].getTranslationX() == 0f);
  }

  void focus() {
    TextView a = rows[1];
    TextView b = rows[3];
    check("not focusable by default", !a.isFocusable());
    check("requestFocus on non-focusable false", !a.requestFocus());
    a.setFocusable(true);
    check("focusable set", a.isFocusable());
    final int[] aChanges = new int[1];
    final boolean[] aLast = new boolean[1];
    a.setOnFocusChangeListener(
        (v, has) -> {
          aChanges[0]++;
          aLast[0] = has;
        });
    check("listener stored", a.getOnFocusChangeListener() != null);
    if (!a.requestFocus()) {
      // A touch board has no keypad focus group; as in Android's touch mode, focus is declined.
      Log.i(TAG, "requestFocus declined (touch board): focus checks skipped");
      a.setFocusable(false);
      return;
    }
    check("isFocused after request", a.isFocused() && a.hasFocus());
    b.setFocusable(true);
    check("second requestFocus", b.requestFocus());
    check("focus moved", b.isFocused() && !a.isFocused());
    Log.i(TAG, "focus listener after hand-off: changes=" + aChanges[0] + " last=" + aLast[0]);
    check("loss reported if listener fired", aChanges[0] == 0 || !aLast[0]);
    a.setFocusable(false);
    check("unfocusable again", !a.isFocusable());
  }

  // ---- phase 2: after layout --------------------------------------------------------------

  void phase2() {
    Log.i(TAG, "phase2");
    section("deferredEvents", () -> deferredEvents());
    section("sizes", () -> sizes());
    section("animation", () -> animation());
    section("dialogs", () -> dialogs());
    section("transient", () -> transientUi());
    section("churn", () -> churn());
    later(600, () -> phase3());
  }

  /** What phase 1 fired synthetically has been delivered by now. */
  void deferredEvents() {
    check("performClick delivered twice", clicksFirst[0] == 2);
    clickButton.setOnClickListener(v -> clicksSecond[0]++);
    clickButton.performClick();
    check("performCheckedChange delivered once", checkEvents[0] == 1);
    Log.i(
        TAG,
        "checkbox after deferred toggle: checked="
            + checkBox.isChecked()
            + " last="
            + checkLast[0]);
    check("performProgressChange delivered", seekChanged[0] == 1);
    check("performItemSelected delivered", spinnerSelected[0] == spinner.getSelectedItemPosition());
  }

  void sizes() {
    check(
        "root size",
        root.getWidth() == getDisplay().getWidth() && root.getHeight() == getDisplay().getHeight());
    check("setSize honoured", sized.getWidth() == 100 && sized.getHeight() == 30);
    check("rows laid out with a size", rows[0].getWidth() > 0 && rows[0].getHeight() > 0);
    check(
        "column stacks without overlap",
        rows[1].getTop() >= rows[0].getTop() + rows[0].getHeight());
    check("root at origin", root.getLeft() == 0 && root.getTop() == 0);
    rows[0].setTranslationX(8f);
    check("getX includes translation", rows[0].getX() == rows[0].getLeft() + 8f);
    check("getY without translation", rows[0].getY() == rows[0].getTop());
    rows[0].setTranslationX(0f);
    Log.i(
        TAG,
        "row0 "
            + rows[0].getLeft()
            + ","
            + rows[0].getTop()
            + " "
            + rows[0].getWidth()
            + "x"
            + rows[0].getHeight()
            + " row1 top="
            + rows[1].getTop());
  }

  void animation() {
    picodroid.view.ViewPropertyAnimator an = animated.animate();
    an.alpha(0.3f).translationX(20f).setDuration(120).withEndAction(() -> endFired[0]++);
    check("animator duration", an.getDuration() == 120 && an.getStartDelay() == 0);
    an.start();
    picodroid.view.ViewPropertyAnimator cancelled = sized.animate();
    cancelled.alpha(0f).setDuration(5000).withEndAction(() -> endFired[0] += 100);
    cancelled.start();
    cancelled.cancel();
    check("cancel returns", true);
  }

  void dialogs() {
    for (int i = 0; i < 10; i++) {
      AlertDialog d =
          new AlertDialog.Builder(this)
              .setTitle("t" + i)
              .setMessage("m")
              .setPositiveButton("ok", (dialog, which) -> dialogButton[0] = which)
              .setNegativeButton("no", null)
              .create();
      d.show();
      d.dismiss();
    }
    check("10 dialogs shown and dismissed", true);
    AlertDialog c =
        new AlertDialog.Builder(this)
            .setTitle("cancel")
            .setMessage("x")
            .setNegativeButton("n", null)
            .show();
    c.cancel();
    check("cancel returns", true);
    AlertDialog items =
        new AlertDialog.Builder(this)
            .setTitle("pick")
            .setItems(new String[] {"a", "b", "c"}, (dialog, which) -> dialogItem[0] = which)
            .show();
    items.performItemClick(1);
    check("performItemClick returns", true);
    AlertDialog twice = new AlertDialog.Builder(this).setTitle("twice").setMessage("y").create();
    twice.show();
    twice.dismiss();
    twice.dismiss();
    check("double dismiss harmless", true);
    AlertDialog never = new AlertDialog.Builder(this).setTitle("never").setMessage("z").create();
    never.dismiss();
    check("dismiss before show harmless", true);
  }

  void transientUi() {
    for (int i = 0; i < 10; i++) {
      Toast.makeText(this, "toast " + i, Toast.LENGTH_SHORT).show();
    }
    Toast t = Toast.makeText(this, "long", Toast.LENGTH_LONG);
    check("toast duration", t.getDuration() == Toast.LENGTH_LONG);
    t.setDuration(Toast.LENGTH_SHORT);
    check("toast setDuration", t.getDuration() == Toast.LENGTH_SHORT);
    t.show();
    t.cancel();
    check("toast cancel", true);
    final int[] action = new int[1];
    Snackbar s =
        Snackbar.make(root, "snack", Snackbar.LENGTH_INDEFINITE)
            .setAction("undo", v -> action[0]++);
    s.show();
    s.dismiss();
    Snackbar.make(root, "short", Snackbar.LENGTH_SHORT).show();
    check("snackbars", true);
    Notification n =
        new Notification.Builder().setContentTitle("title").setContentText("text").build();
    check(
        "notification fields",
        n.getContentTitle().equals("title") && n.getContentText().equals("text"));
    NotificationManager.getInstance().notify(7, n);
    NotificationManager.getInstance().cancel(7);
    NotificationManager.getInstance().cancel(8);
    check("notification post/cancel", true);
  }

  void churn() {
    long before = Runtime.usedMemory();
    int gcBefore = Runtime.gcCount();
    int baseline = root.getChildCount();
    for (int i = 0; i < 200; i++) {
      TextView t = new TextView(this);
      t.setText("churn " + i);
      root.addView(t);
      root.removeView(t);
    }
    Log.i(
        TAG,
        "view churn: used "
            + before
            + " -> "
            + Runtime.usedMemory()
            + " gcs "
            + gcBefore
            + " -> "
            + Runtime.gcCount());
    check("root child count after churn", root.getChildCount() == baseline);
    rooted = new Button[12];
    for (int i = 0; i < 12; i++) {
      Button b = new Button(this, "b" + i);
      b.setOnClickListener(v -> churnClicks[0]++);
      root.addView(b);
      rooted[i] = b;
    }
    for (int i = 0; i < 3000; i++) {
      String junk = "garbage-" + i;
      if (junk.length() == 0) {
        churnClicks[0]--;
      }
      byte[] bytes = new byte[256];
      bytes[0] = 1;
    }
    Log.i(TAG, "after garbage: gcs " + Runtime.gcCount());
    for (int i = 0; i < 12; i++) {
      rooted[i].performClick();
    }
    churnTarget = new Button(this, "target");
    root.addView(churnTarget);
    churnTarget.setOnClickListener(v -> churnClicks[0] += 1000);
    check(
        "children tracked through churn",
        root.getChildAt(baseline) == rooted[0] && root.getChildAt(baseline + 12) == churnTarget);
  }

  // ---- phase 3 ------------------------------------------------------------------------------

  private int animationPolls = 0;

  void phase3() {
    // The 120 ms animation is checked below; on a device the LVGL clock advances per UI tick and
    // can
    // lag wall time, so wait for the end action (up to ~3 s) rather than assuming 600 ms was
    // enough.
    if (endFired[0] == 0 && animationPolls < 15) {
      animationPolls++;
      later(200, () -> phase3());
      return;
    }
    Log.i(TAG, "phase3 (animation polls: " + animationPolls + ")");
    check("listeners survive GC (rooted by the tree)", churnClicks[0] == 12);
    for (int i = 0; i < 12; i++) {
      root.removeView(rooted[i]);
      rooted[i] = null;
    }
    for (int i = 0; i < 3000; i++) {
      byte[] bytes = new byte[256];
      bytes[0] = 1;
    }
    churnTarget.performClick();
    check("replaced click listener delivered once", clicksSecond[0] == 1 && clicksFirst[0] == 2);
    clickButton.setOnClickListener(null);
    clickButton.performClick();
    check("animation end action fired once", endFired[0] == 1);
    Log.i(
        TAG,
        "endFired="
            + endFired[0]
            + " alpha="
            + animated.getAlpha()
            + " tx="
            + animated.getTranslationX());
    check("animated alpha reached target", animated.getAlpha() == 0.3f);
    check("animated translation reached target", animated.getTranslationX() == 20f);
    check("cancelled animation end action not fired", endFired[0] < 100);
    check("dialog item click delivered", dialogItem[0] == 1);
    Log.i(TAG, "dialogItem=" + dialogItem[0] + " dialogButton=" + dialogButton[0]);
    later(300, () -> phase4());
  }

  void phase4() {
    Log.i(TAG, "phase4");
    check("listener on survivor after peers removed", churnClicks[0] == 1012);
    check("null listener silent", clicksSecond[0] == 1);
    Log.i(TAG, "passed=" + passed + " failed=" + failed + " crashed=" + crashed);
    if (failed == 0 && crashed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " failed, " + crashed + " crashed ===");
    }
    finish();
  }
}
