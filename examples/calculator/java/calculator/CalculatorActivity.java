// SPDX-License-Identifier: GPL-3.0-only
package calculator;

import picodroid.app.Activity;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.os.Bundle;
import picodroid.text.TextUtils;
import picodroid.util.Log;
import picodroid.view.Gravity;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.GridLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * A four-function calculator for a touch panel, built for {@code pico_touch_kit} — a 320x480 screen
 * a finger drives. {@link CalculatorEngine} does the arithmetic; this class is only the keypad and
 * the two display lines.
 *
 * <p>The layout is the one every pocket calculator uses, four columns by five rows, with the result
 * right-aligned above it: a {@link GridLayout} of equal tracks under the display panel, {@code 0}
 * spanning two columns. Nothing here is 320x480 specific: the grid takes whatever the panel leaves,
 * so the same app fills a 320x240 testbench or a 240x240 Enviro screen, with smaller keys.
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

  /**
   * Each key carries half the gap as a margin on every side, so neighbours sit a whole gap apart
   * and every key keeps the same size, the outer ones included.
   */
  private static final int HALF_GAP = GAP / 2;

  /** Margin around the whole screen, outside the outer keys' own half gap. */
  private static final int PAD = 6 - HALF_GAP;

  private static final int COLUMNS = 4;

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

  private TextView expressionView;
  private TextView resultView;
  private Button clearKey;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    int width = getDisplay().getWidth();
    int height = getDisplay().getHeight();

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(PAD, PAD, PAD, PAD);

    // The panel lines up with the keys' outer edges, one gap above them.
    LinearLayout.LayoutParams panelParams =
        new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, panelHeight(height));
    panelParams.setMargins(HALF_GAP, HALF_GAP, HALF_GAP, HALF_GAP);
    root.addView(buildPanel(), panelParams);

    // The keypad: every column and row weighted alike, so the keys share the room below the
    // panel equally; each key fills its cell.
    GridLayout keys = new GridLayout(this);
    keys.setColumnCount(COLUMNS);
    Button sample = null;
    for (String[] row : KEYS) {
      for (String label : row) {
        Button key = buildKey(label);
        int span = label.equals(WIDE_KEY) ? 2 : 1;
        GridLayout.LayoutParams cell =
            new GridLayout.LayoutParams(
                GridLayout.spec(GridLayout.UNDEFINED, 1f),
                GridLayout.spec(GridLayout.UNDEFINED, span, GridLayout.FILL, 1f));
        cell.width = ViewGroup.LayoutParams.MATCH_PARENT;
        cell.height = ViewGroup.LayoutParams.MATCH_PARENT;
        cell.setMargins(HALF_GAP, HALF_GAP, HALF_GAP, HALF_GAP);
        keys.addView(key, cell);
        if (sample == null && span == 1) {
          sample = key;
        }
      }
    }
    root.addView(keys, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

    setContentView(root);
    render();
    Log.i(
        TAG,
        "ready: " + width + "x" + height + " keys " + sample.getWidth() + "x" + sample.getHeight());
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
  private LinearLayout buildPanel() {
    LinearLayout panel = new LinearLayout(this);
    panel.setOrientation(LinearLayout.VERTICAL);
    panel.setPadding(GAP, GAP, GAP, GAP);
    // BOTTOM keeps both lines against the keypad, so the display grows upwards
    // as a calculator's does rather than drifting away from the keys.
    panel.setGravity(Gravity.BOTTOM);

    expressionView = displayLine(Theme.colorTextSecondary);
    panel.addView(expressionView, lineParams(0));

    resultView = displayLine(Theme.colorText);
    panel.addView(resultView, lineParams(GAP));

    return panel;
  }

  /**
   * One display line: the panel's width, its text against the right edge, and a long expression
   * ellipsized rather than wrapped.
   */
  private TextView displayLine(int color) {
    TextView text = new TextView(this);
    text.setTextColor(color);
    text.setSingleLine();
    text.setEllipsize(TextUtils.TruncateAt.END);
    text.setGravity(Gravity.RIGHT);
    return text;
  }

  private static LinearLayout.LayoutParams lineParams(int topMargin) {
    LinearLayout.LayoutParams lp =
        new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
    lp.topMargin = topMargin;
    return lp;
  }

  private Button buildKey(String label) {
    Button key = new Button(this);
    key.setText(label);
    key.setTextColor(isOperator(label) ? Theme.colorOnPrimary : Theme.colorText);
    GradientDrawable face = new GradientDrawable();
    face.setColor(keyColor(label));
    face.setCornerRadius(8);
    key.setBackground(face);
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
