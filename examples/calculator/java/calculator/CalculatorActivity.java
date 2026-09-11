// SPDX-License-Identifier: GPL-3.0-only
package calculator;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.text.TextUtils;
import picodroid.util.Log;
import picodroid.view.Gravity;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * A four-function calculator for a touch panel, built for {@code pico_touch_kit} — a 320x480 screen
 * a finger drives. {@link CalculatorEngine} does the arithmetic; this class is only the keypad and
 * the two display lines.
 *
 * <p>The layout is the one every pocket calculator uses, four columns by five rows, with the result
 * right-aligned above it. Nothing here is 320x480 specific: every box is derived from {@link
 * picodroid.graphics.Display#getWidth()} and {@code getHeight()}, so the same app fills a 320x240
 * testbench or a 240x240 Enviro screen, with smaller keys.
 *
 * <p>Touch is the only input. The keys are deliberately not focusable: the board's two hardware
 * buttons are BACK and HOME, not a d-pad, so a focus ring would sit on a key that nothing can move.
 * BACK leaves the app for the launcher, which is the {@link Activity} default.
 *
 * <p>Every key press is logged as {@code <key> -> <display>}, which is what lets a sim or HIL run
 * tap a sequence and check the arithmetic without reading the screen.
 */
public class CalculatorActivity extends Activity {
  private static final String TAG = "Calculator";

  /** Gap between keys, and between the keypad and the display panel. */
  private static final int GAP = 4;

  /** Margin around the whole screen. */
  private static final int PAD = 6;

  private static final int COLUMNS = 4;
  private static final int ROWS = 5;

  /**
   * The keypad, in reading order. A label is both what the key shows and what {@link #onKey} acts
   * on, except {@code C}, whose face alternates with {@link CalculatorEngine#clearLabel()}.
   * Multiply is {@code x} and divide {@code /} because the bundled Montserrat font carries no
   * {@code ×} or {@code ÷} glyph.
   */
  private static final String[][] KEYS = {
    {"C", "+/-", "%", "/"},
    {"7", "8", "9", "x"},
    {"4", "5", "6", "-"},
    {"1", "2", "3", "+"},
    {"0", ".", "="},
  };

  /** The double-width key on the bottom row, as on every calculator. */
  private static final String WIDE_KEY = "0";

  /** Amber for {@code =}, the one key that is neither a digit nor a pending operator. */
  private static final int COLOR_EQUALS = 0xFFE08A2C;

  /** Digit keys: a touch lighter than {@link Theme#colorSurface} so they read as raised. */
  private static final int COLOR_DIGIT = 0xFF2B2B36;

  /** {@code C}, {@code +/-} and {@code %}: lighter again, the way Android greys its modifiers. */
  private static final int COLOR_FUNCTION = 0xFF44444F;

  private final CalculatorEngine engine = new CalculatorEngine();

  /** Held for the lifetime of the Activity so the keys outlive {@code onCreate}. */
  private View[] keys;

  private TextView expressionView;
  private TextView resultView;
  private Button clearKey;

  @Override
  public void onCreate() {
    int width = getDisplay().getWidth();
    int height = getDisplay().getHeight();
    int contentWidth = width - 2 * PAD;
    int keyWidth = (contentWidth - (COLUMNS - 1) * GAP) / COLUMNS;
    int panelHeight = panelHeight(height);
    int keyHeight = (height - 2 * PAD - panelHeight - ROWS * GAP) / ROWS;

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(width, height);
    root.setPadding(PAD, PAD, PAD, PAD);
    root.setSpacing(GAP);
    flatten(root, Theme.colorBackground);

    root.addView(buildPanel(contentWidth, panelHeight));

    int total = 0;
    for (String[] row : KEYS) {
      total += row.length;
    }
    keys = new View[total];
    int next = 0;
    for (String[] row : KEYS) {
      LinearLayout line = new LinearLayout();
      line.setOrientation(LinearLayout.HORIZONTAL);
      line.setSize(contentWidth, keyHeight);
      line.setPadding(0, 0, 0, 0);
      line.setSpacing(GAP);
      flatten(line, Color.TRANSPARENT);
      for (String label : row) {
        int w = label.equals(WIDE_KEY) ? 2 * keyWidth + GAP : keyWidth;
        Button key = buildKey(label, w, keyHeight);
        line.addView(key);
        keys[next++] = key;
      }
      root.addView(line);
    }

    setContentView(root);
    render();
    Log.i(TAG, "ready: " + width + "x" + height + " keys " + keyWidth + "x" + keyHeight);
  }

  /**
   * How much of the screen the two display lines take. A quarter of a tall panel, but never so
   * little that the result crowds the keys on a short one.
   */
  private static int panelHeight(int height) {
    int quarter = height / 4;
    if (quarter < 64) {
      return 64;
    }
    return quarter > 140 ? 140 : quarter;
  }

  /** The display: the running expression above, the current value below, both right-aligned. */
  private LinearLayout buildPanel(int contentWidth, int panelHeight) {
    LinearLayout panel = new LinearLayout();
    panel.setOrientation(LinearLayout.VERTICAL);
    panel.setSize(contentWidth, panelHeight);
    panel.setPadding(GAP, GAP, GAP, GAP);
    panel.setSpacing(GAP);
    flatten(panel, Color.TRANSPARENT);
    // BOTTOM keeps both lines against the keypad, so the display grows upwards
    // as a calculator's does rather than drifting away from the keys.
    panel.setGravity(Gravity.BOTTOM);

    expressionView = new TextView();
    expressionView.setTextColor(Theme.colorTextSecondary);
    expressionView.setSingleLine();
    expressionView.setEllipsize(TextUtils.TruncateAt.END);
    panel.addView(rightAligned(expressionView, contentWidth - 2 * GAP));

    resultView = new TextView();
    resultView.setTextColor(Theme.colorText);
    resultView.setSingleLine();
    resultView.setEllipsize(TextUtils.TruncateAt.END);
    panel.addView(rightAligned(resultView, contentWidth - 2 * GAP));

    return panel;
  }

  /**
   * A full-width row holding one label at its right edge. {@link LinearLayout#setGravity} aligns
   * along the layout's own axis, so right alignment is a horizontal row ending in the label rather
   * than a property of the label itself.
   */
  private static LinearLayout rightAligned(TextView text, int width) {
    LinearLayout row = new LinearLayout();
    row.setOrientation(LinearLayout.HORIZONTAL);
    row.setSize(width, View.WRAP_CONTENT);
    row.setPadding(0, 0, 0, 0);
    row.setGravity(Gravity.RIGHT);
    flatten(row, Color.TRANSPARENT);
    row.addView(text);
    return row;
  }

  /**
   * Paint a layout as Android paints one: a flat fill, no rounded corners and no outline. An LVGL
   * object carries the theme's card styling, which on a nested layout draws a box around every row
   * — a {@link GradientDrawable} is what sets the border width back to zero, and a transparent fill
   * is what lets the screen behind show through.
   */
  private static void flatten(View v, int argb) {
    new GradientDrawable().setColor(argb).setCornerRadius(0).setStroke(0, argb).applyTo(v);
  }

  private Button buildKey(String label, int width, int height) {
    Button key = new Button(label);
    key.setSize(width, height);
    key.setTextColor(isOperator(label) ? Theme.colorOnPrimary : Theme.colorText);
    new GradientDrawable().setColor(keyColor(label)).setCornerRadius(8).applyTo(key);
    key.setOnClickListener(v -> onKey(label));
    if (label.equals("C")) {
      clearKey = key;
    }
    return key;
  }

  private static boolean isOperator(String label) {
    return label.equals("+") || label.equals("-") || label.equals("x") || label.equals("/");
  }

  private static int keyColor(String label) {
    if (label.equals("=")) {
      return COLOR_EQUALS;
    }
    if (isOperator(label)) {
      return Theme.colorPrimary;
    }
    if (label.equals("C") || label.equals("+/-") || label.equals("%")) {
      return COLOR_FUNCTION;
    }
    return COLOR_DIGIT;
  }

  /** One key press: feed the engine, then repaint from it. */
  private void onKey(String label) {
    char c = label.charAt(0);
    if (label.length() == 1 && c >= '0' && c <= '9') {
      engine.digit(c - '0');
    } else if (label.equals(".")) {
      engine.dot();
    } else if (label.equals("=")) {
      engine.equals();
    } else if (label.equals("%")) {
      engine.percent();
    } else if (label.equals("+/-")) {
      engine.negate();
    } else if (label.equals("C")) {
      engine.clear();
    } else {
      engine.operator(c);
    }
    render();
    Log.i(TAG, label + " -> " + engine.display());
  }

  /** The display is written from the engine only, never edited in place by a key. */
  private void render() {
    expressionView.setText(engine.expression());
    resultView.setText(engine.display());
    clearKey.setText(engine.clearLabel());
  }
}
