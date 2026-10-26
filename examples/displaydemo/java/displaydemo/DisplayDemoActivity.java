// SPDX-License-Identifier: GPL-3.0-only
package displaydemo;

import picodroid.app.Activity;
import picodroid.debug.DisplayDebug;
import picodroid.graphics.Color;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.os.Bundle;
import picodroid.pio.Gpio;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.AdapterView;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.Button;
import picodroid.widget.CheckBox;
import picodroid.widget.CircularProgressIndicator;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.ProgressBar;
import picodroid.widget.ScrollView;
import picodroid.widget.SeekBar;
import picodroid.widget.Spinner;
import picodroid.widget.Switch;
import picodroid.widget.TextView;
import picodroid.widget.ToggleButton;

public class DisplayDemoActivity extends Activity {
  private static final int WRAP = LinearLayout.LayoutParams.WRAP_CONTENT;

  /** The space between the column's rows. */
  private static final int GAP = 4;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    getDisplay();
    DisplayDebug.calibrate();
    DisplayDebug.showFps();
    Log.i("DisplayDemo", "Display ready");

    ScrollView scroll = new ScrollView(this);

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 5, 10, 5);
    scroll.addView(root, new ViewGroup.LayoutParams(320, ViewGroup.LayoutParams.WRAP_CONTENT));

    TextView title = new TextView(this);
    title.setText("Picodroid UI Demo");
    title.setTextColor(Color.WHITE);
    title.setIncludeFontPadding(false);
    add(root, title, WRAP, WRAP);

    // -- Button with tap counter --
    Button btn = new Button(this);
    btn.setText("Tap Me!");
    btn.setOnClickListener(new TapHandler(title));
    add(root, btn, 200, 50);

    // -- ToggleButton controlling LED --
    TextView toggleLabel = new TextView(this);
    toggleLabel.setText("LED: OFF");
    toggleLabel.setTextColor(Color.WHITE);
    toggleLabel.setIncludeFontPadding(false);
    add(root, toggleLabel, WRAP, WRAP);

    ToggleButton toggle = new ToggleButton(this);
    toggle.setTextOn("ON");
    toggle.setTextOff("OFF");
    PeripheralManager manager = PeripheralManager.getInstance();
    Gpio led = manager.openGpio("GP25");
    led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
    toggle.setOnCheckedChangeListener(
        (view, checked) -> {
          if (checked) {
            toggleLabel.setText("LED: ON");
            led.setValue(true);
          } else {
            toggleLabel.setText("LED: OFF");
            led.setValue(false);
          }
        });
    add(root, toggle, 200, 50);

    // -- Switch --
    TextView switchLabel = new TextView(this);
    switchLabel.setText("Switch: OFF");
    switchLabel.setTextColor(Color.WHITE);
    switchLabel.setIncludeFontPadding(false);
    add(root, switchLabel, WRAP, WRAP);

    Switch sw = new Switch(this);
    sw.setOnCheckedChangeListener(new SwitchHandler(switchLabel));
    add(root, sw, 60, 30);

    // -- SeekBar driving a determinate ProgressBar --
    TextView seekLabel = new TextView(this);
    seekLabel.setText("SeekBar: 0");
    seekLabel.setTextColor(Color.CYAN);
    seekLabel.setIncludeFontPadding(false);
    add(root, seekLabel, WRAP, WRAP);

    ProgressBar progress = new ProgressBar(this);
    add(root, progress, 200, 12);

    // The same value as a ring gauge: a three-quarter dial, open at the bottom.
    CircularProgressIndicator ring = new CircularProgressIndicator(this);
    ring.setIndicatorSize(56);
    ring.setTrackThickness(6);
    ring.setIndicatorColor(Color.CYAN);
    ring.setStartAngle(135f);
    ring.setSweepAngle(270f);
    add(root, ring, 56, 56);

    SeekBar seekBar = new SeekBar(this);
    seekBar.setMax(100);
    seekBar.setOnSeekBarChangeListener(
        (bar, v, fromUser) -> {
          seekLabel.setText("SeekBar: " + v);
          progress.setProgress(v, true);
          ring.setProgress(v);
        });
    add(root, seekBar, 200, 20);

    // -- Indeterminate spinner --
    TextView spinnerProgressLabel = new TextView(this);
    spinnerProgressLabel.setText("Working...");
    spinnerProgressLabel.setTextColor(Color.WHITE);
    spinnerProgressLabel.setIncludeFontPadding(false);
    add(root, spinnerProgressLabel, WRAP, WRAP);

    ProgressBar busy = ProgressBar.indeterminate();
    add(root, busy, 40, 40);

    progress.setProgress(40);
    ring.setProgress(40);
    busy.setProgress(99); // ignored: indeterminate
    Log.i(
        "DisplayDemo",
        "readback progress="
            + progress.getProgress()
            + " busy="
            + busy.getProgress()
            + " indet="
            + busy.isIndeterminate());

    // -- ImageView showing a bundled PNG asset --
    ImageView img = new ImageView(this);
    img.setImageSource("logo.png");
    img.setScaleType(ImageView.SCALE_FIT_CENTER);
    add(root, img, 48, 48);

    // -- CheckBox --
    CheckBox checkBox = new CheckBox(this);
    checkBox.setText("Enable notifications");
    checkBox.setOnCheckedChangeListener(
        (view, checked) -> {
          if (checked) {
            Log.i("DisplayDemo", "Notifications enabled");
          } else {
            Log.i("DisplayDemo", "Notifications disabled");
          }
        });
    add(root, checkBox, WRAP, WRAP);

    // -- Spinner --
    TextView spinnerLabel = new TextView(this);
    spinnerLabel.setText("Color: Red");
    spinnerLabel.setTextColor(Color.RED);
    spinnerLabel.setIncludeFontPadding(false);
    add(root, spinnerLabel, WRAP, WRAP);

    Spinner spinner = new Spinner(this);
    spinner.setAdapter(
        new ArrayAdapter<>(
            this,
            picodroid.R.layout.simple_list_item_1,
            new String[] {"Red", "Green", "Blue", "Yellow"}));
    spinner.setOnItemSelectedListener(
        new AdapterView.OnItemSelectedListener() {
          @Override
          public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
            if (position == 0) {
              spinnerLabel.setText("Color: Red");
              spinnerLabel.setTextColor(Color.RED);
            } else if (position == 1) {
              spinnerLabel.setText("Color: Green");
              spinnerLabel.setTextColor(Color.GREEN);
            } else if (position == 2) {
              spinnerLabel.setText("Color: Blue");
              spinnerLabel.setTextColor(Color.BLUE);
            } else {
              spinnerLabel.setText("Color: Yellow");
              spinnerLabel.setTextColor(Color.YELLOW);
            }
          }

          @Override
          public void onNothingSelected(AdapterView<?> parent) {}
        });
    add(root, spinner, 200, 40);

    // -- Themed widgets: GradientDrawable + Theme palette --
    TextView themedHeader = new TextView(this);
    themedHeader.setText("  Themed widgets");
    themedHeader.setTextColor(Theme.colorOnPrimary);
    GradientDrawable headerBg =
        new GradientDrawable(
            GradientDrawable.Orientation.TOP_BOTTOM,
            new int[] {Theme.colorPrimary, darken(Theme.colorPrimary)});
    headerBg.setCornerRadius(8);
    themedHeader.setBackground(headerBg);
    add(root, themedHeader, 220, 40);

    // LEFT_RIGHT is a different render path from TOP_BOTTOM: LVGL blends a
    // horizontal gradient as an RGB888 source image, a vertical one as solid
    // lines. This row went fully transparent when the RGB888 source blender
    // was compiled out (lv_conf.h LV_DRAW_SW_SUPPORT_RGB888; D2 in
    // docs/designs/claudeusage-gaps-roadmap-2026-09.md).
    TextView themedBar = new TextView(this);
    themedBar.setText("  Left-to-right gradient");
    themedBar.setTextColor(Theme.colorOnPrimary);
    GradientDrawable barBg =
        new GradientDrawable(
            GradientDrawable.Orientation.LEFT_RIGHT,
            new int[] {Theme.colorPrimary, Theme.colorBackground});
    barBg.setCornerRadius(8);
    themedBar.setBackground(barBg);
    add(root, themedBar, 220, 40);
    Log.i("DisplayDemo", "gradient rows: TOP_BOTTOM + LEFT_RIGHT");

    TextView card = new TextView(this);
    card.setText("  Card surface");
    card.setTextColor(Theme.colorText);
    GradientDrawable cardBg = new GradientDrawable();
    cardBg.setColor(Theme.colorSurface);
    cardBg.setCornerRadius(10);
    cardBg.setStroke(1, Theme.colorOutline);
    card.setBackground(cardBg);
    add(root, card, 220, 50);

    Button pill = new Button(this);
    pill.setText("Pill button");
    GradientDrawable pillBg = new GradientDrawable();
    pillBg.setColor(Theme.colorPrimary);
    // Pill = corner radius >= half the height. Setting it past
    // that doesn't visually overshoot — LVGL clamps internally.
    pillBg.setCornerRadius(20);
    pill.setBackground(pillBg);
    pill.setOnClickListener(v -> Log.i("DisplayDemo", "pill clicked"));
    add(root, pill, 220, 38);

    Button ghost = new Button(this);
    ghost.setText("Ghost button");
    GradientDrawable ghostBg = new GradientDrawable();
    ghostBg.setColor(Theme.colorBackground);
    ghostBg.setCornerRadius(8);
    ghostBg.setStroke(2, Theme.colorPrimary);
    ghost.setBackground(ghostBg);
    ghost.setOnClickListener(v -> Log.i("DisplayDemo", "ghost clicked"));
    add(root, ghost, 220, 38);

    // A custom view: onDraw(Canvas) draws shapes and text instead of stacking widgets.
    add(root, new ShapesView(this), ShapesView.WIDTH, ShapesView.HEIGHT);

    setContentView(scroll);

    // Property read-back: laid-out geometry and the cached state getters.
    Log.i(
        "DisplayDemo",
        "btn frame="
            + btn.getLeft()
            + ","
            + btn.getTop()
            + " "
            + btn.getWidth()
            + "x"
            + btn.getHeight());
    btn.setEnabled(false);
    Log.i(
        "DisplayDemo",
        "readback enabled=" + btn.isEnabled() + " vis=" + (title.getVisibility() == View.VISIBLE));
    btn.setEnabled(true);
    title.setAlpha(0.5f);
    Log.i("DisplayDemo", "readback alpha-half=" + (title.getAlpha() == 0.5f));
    title.setAlpha(1.0f);
  }

  /** Adds {@code child} at {@code w} x {@code h}, {@link #GAP} below the one before it. */
  private static void add(LinearLayout parent, View child, int w, int h) {
    LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(w, h);
    if (parent.getChildCount() > 0) {
      lp.topMargin = GAP;
    }
    parent.addView(child, lp);
  }

  /** Quick-and-dirty 70%-brightness shade for the gradient end stop. */
  private static int darken(int argb) {
    int a = (argb >> 24) & 0xFF;
    int r = ((argb >> 16) & 0xFF) * 70 / 100;
    int g = ((argb >> 8) & 0xFF) * 70 / 100;
    int b = (argb & 0xFF) * 70 / 100;
    return Color.argb(a, r, g, b);
  }
}
