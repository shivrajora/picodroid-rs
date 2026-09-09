// SPDX-License-Identifier: GPL-3.0-only
package ellipsizedemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.text.TextUtils;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Conformance for {@code TextView.setSingleLine} / {@code setEllipsize} / {@code setMaxLines}: a
 * long text in a 96-px-wide label is one line tall with an ellipsis, {@code getText()} still
 * returns the whole text (LVGL's dots overwrite its buffer), a max-lines label is that many lines
 * tall, lifting the limit restores the wrapped height, and a padding change keeps the limit. The
 * checks run a frame later, once the layout — and with it the dots — has been applied. Logs {@code
 * === ALL PASSED ===} or one {@code FAIL} line per check.
 */
public class EllipsizeDemoActivity extends Activity {
  private static final String TAG = "EllipsizeDemo";

  /** Wraps over several 96-px lines; well under the label's 127-byte text cap. */
  private static final String LONG = "The quick brown fox jumps over the lazy dog again and again";

  private static final int WIDTH = 96;

  private LinearLayout root;
  private TextView reference;
  private TextView single;
  private TextView wrapped;
  private TextView twoLines;
  private Button button;
  private int failures;

  @Override
  public void onCreate() {
    getDisplay();
    root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(4, 4, 4, 4);

    // One line, content-sized: the height every other label is measured against.
    reference = label("x");
    single = label(LONG);
    single.setSize(WIDTH, View.WRAP_CONTENT);
    single.setSingleLine();
    single.setEllipsize(TextUtils.TruncateAt.END);
    wrapped = label(LONG);
    wrapped.setSize(WIDTH, View.WRAP_CONTENT);
    twoLines = label(LONG);
    twoLines.setSize(WIDTH, View.WRAP_CONTENT);
    twoLines.setMaxLines(2);
    twoLines.setEllipsize(TextUtils.TruncateAt.END);
    // A Button is a TextView over a button with a child label: the setters must reach the label.
    button = new Button(LONG);
    button.setSize(WIDTH, View.WRAP_CONTENT);
    button.setSingleLine();
    button.setEllipsize(TextUtils.TruncateAt.END);
    root.addView(button);
    setContentView(root);

    // A frame later the layout, and with it the dots, have been applied.
    root.animate().alpha(1f).setDuration(100).withEndAction(() -> check()).start();
  }

  private TextView label(String text) {
    TextView t = new TextView();
    t.setText(text);
    t.setTextColor(Color.WHITE);
    root.addView(t);
    return t;
  }

  private void expect(boolean ok, String what) {
    if (!ok) {
      failures++;
      Log.i(TAG, "FAIL " + what);
    }
  }

  private void check() {
    // getHeight forces a layout pass, so sizes and the dots are current from here on.
    int line = reference.getHeight();
    expect(line > 0, "reference height " + line);
    expect(single.getHeight() == line, "single-line height " + single.getHeight() + " vs " + line);
    expect(wrapped.getHeight() > line, "wrapped height " + wrapped.getHeight() + " vs " + line);
    int two = twoLines.getHeight();
    expect(two > line && two < wrapped.getHeight(), "two-line height " + two);

    expect(LONG.equals(single.getText().toString()), "getText under dots: " + single.getText());
    expect(LONG.equals(twoLines.getText().toString()), "getText under two-line dots");
    expect(LONG.equals(button.getText().toString()), "button getText: " + button.getText());

    expect(single.getEllipsize() == TextUtils.TruncateAt.END, "getEllipsize END");
    expect(wrapped.getEllipsize() == null, "getEllipsize default null");
    expect(single.getMaxLines() == 1, "getMaxLines under single line " + single.getMaxLines());
    expect(twoLines.getMaxLines() == 2, "getMaxLines 2");
    expect(wrapped.getMaxLines() == Integer.MAX_VALUE, "getMaxLines default");

    // Lifting the limit re-applies the mode: the label grows back to its wrapped height.
    single.setSingleLine(false);
    single.setEllipsize(null);
    expect(
        single.getHeight() == wrapped.getHeight(),
        "lifted height " + single.getHeight() + " vs " + wrapped.getHeight());

    // A padding change keeps the limit: one line, taller by the padding alone.
    twoLines.setMaxLines(1);
    twoLines.setPadding(0, 6, 0, 6);
    expect(
        twoLines.getHeight() == line + 12,
        "padded single line " + twoLines.getHeight() + " vs " + (line + 12));

    Log.i(TAG, failures == 0 ? "=== ALL PASSED ===" : failures + " FAILED");
  }
}
