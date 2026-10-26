// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picodroid.content.Context;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.text.TextUtils;
import picodroid.view.Gravity;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.RelativeLayout;
import picodroid.widget.Space;
import picodroid.widget.TextView;

/**
 * The one look every screen wears, and the sizes that make it tappable.
 *
 * <p>Everything here is sized for the EP-0172's 320x480 panel driven by a finger. The panel has a
 * touch controller and two buttons, and board.toml gives both buttons to the system (BACK and
 * HOME), so nothing in this app is reached by focus navigation — {@link #TAP_HEIGHT} is the floor
 * for anything a user has to hit, and it is set from the roughly 9 mm a fingertip covers at this
 * panel's 165 dpi.
 *
 * <p>Every screen is a vertical {@link LinearLayout}: the header, the content, a {@link #fill} that
 * takes the slack, and the controls along the bottom edge.
 */
public final class Ui {
  // ── Palette ────────────────────────────────────────────────────────────────

  public static final int BACKGROUND = 0xFF0B0F14;
  public static final int SURFACE = 0xFF18242E;
  public static final int SURFACE_HIGH = 0xFF243645;
  public static final int ACCENT = 0xFF35D6C8;
  public static final int TEXT = 0xFFE8EEF2;
  public static final int MUTED = 0xFF7E93A3;
  public static final int DANGER = 0xFFE0574B;

  /**
   * A lit segment, and the same segment dark: a ghost, not a hole, so the digit reads as a digit.
   */
  public static final int SEGMENT_ON = 0xFF35D6C8;

  public static final int SEGMENT_OFF = 0xFF16222C;

  // ── Metrics ────────────────────────────────────────────────────────────────

  /** The header band across the top of every screen. */
  public static final int HEADER_HEIGHT = 44;

  /** The smallest a control may be and still be hit reliably with a fingertip. */
  public static final int TAP_HEIGHT = 56;

  /** Gap between stacked controls. */
  public static final int GAP = 8;

  /** Side margin. */
  public static final int MARGIN = 12;

  private static final int MATCH = ViewGroup.LayoutParams.MATCH_PARENT;
  private static final int WRAP = ViewGroup.LayoutParams.WRAP_CONTENT;

  private Ui() {}

  // ── Building blocks ────────────────────────────────────────────────────────

  /** A screen's content root: a column on the app's background, filling the window. */
  public static LinearLayout screen(Context context) {
    LinearLayout root = new LinearLayout(context);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setBackgroundColor(BACKGROUND);
    return root;
  }

  /** The id of the header's back chevron, which the caption's rule names. */
  private static final int ID_BACK = 1;

  /**
   * Adds the header band to {@code screen}: a title, and a back chevron when {@code onBack} is
   * given. The chevron is a tappable target of its own, since the carrier's BACK button is
   * reachable only by feel. A RelativeLayout places the title: right of the chevron and centred on
   * the band's height.
   */
  public static void header(LinearLayout screen, String title, View.OnClickListener onBack) {
    Context context = screen.getContext();
    RelativeLayout bar = new RelativeLayout(context);
    bar.setBackgroundColor(SURFACE);

    if (onBack != null) {
      Button back = new Button(context);
      back.setText("<");
      back.setId(ID_BACK);
      back.setTextColor(ACCENT);
      back.setBackground(rounded(SURFACE, 0));
      back.setOnClickListener(onBack);
      RelativeLayout.LayoutParams lp = new RelativeLayout.LayoutParams(48, HEADER_HEIGHT);
      lp.addRule(RelativeLayout.ALIGN_PARENT_LEFT);
      bar.addView(back, lp);
    }

    TextView caption = line(context, title, TEXT);
    RelativeLayout.LayoutParams lp = new RelativeLayout.LayoutParams(WRAP, WRAP);
    lp.addRule(RelativeLayout.CENTER_VERTICAL);
    if (onBack != null) {
      lp.addRule(RelativeLayout.RIGHT_OF, ID_BACK);
      lp.setMargins(GAP, 0, 0, 0);
    } else {
      lp.setMargins(MARGIN, 0, 0, 0);
    }
    bar.addView(caption, lp);
    screen.addView(bar, new LinearLayout.LayoutParams(MATCH, HEADER_HEIGHT));
  }

  /** A line of body text, cut with an ellipsis when it does not fit. */
  public static TextView line(Context context, String text, int color) {
    TextView t = new TextView(context);
    t.setText(text);
    t.setTextColor(color);
    t.setSingleLine();
    t.setEllipsize(TextUtils.TruncateAt.END);
    return t;
  }

  /**
   * Adds a line of body text to {@code column}, inset by the side margin, {@code top} pixels below
   * the view above it.
   */
  public static TextView label(LinearLayout column, String text, int top, int color) {
    TextView t = line(column.getContext(), text, color);
    LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(MATCH, WRAP);
    lp.setMargins(MARGIN, top, MARGIN, 0);
    column.addView(t, lp);
    return t;
  }

  /** {@link #label}, centred across the column: for a screen that retexts it later. */
  public static TextView centred(LinearLayout column, String text, int top, int color) {
    TextView t = label(column, text, top, color);
    t.setGravity(Gravity.CENTER_HORIZONTAL);
    return t;
  }

  /** Adds an empty view to {@code column} that takes whatever height the rest leaves. */
  public static void fill(LinearLayout column) {
    column.addView(new Space(column.getContext()), new LinearLayout.LayoutParams(MATCH, 0, 1f));
  }

  /**
   * A filled, finger-sized button. The label takes whichever of black or the body colour reads
   * against {@code fill} — {@link #ACCENT} is the only fill here bright enough for black, and
   * getting that backwards leaves a button that looks empty rather than merely low-contrast.
   */
  public static Button button(Context context, String text, int fill) {
    Button b = new Button(context);
    b.setText(text);
    b.setTextColor(fill == ACCENT ? Color.BLACK : TEXT);
    b.setBackground(rounded(fill, 10));
    return b;
  }

  /**
   * Adds a full-width row of {@code buttons}, {@code height} tall, to {@code column}: equal widths
   * with a {@link #GAP} between them, inset by the side margin, {@code bottom} pixels above what
   * follows.
   */
  public static void row(LinearLayout column, int height, int bottom, View... buttons) {
    LinearLayout row = new LinearLayout(column.getContext());
    row.setOrientation(LinearLayout.HORIZONTAL);
    for (int i = 0; i < buttons.length; i++) {
      LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(0, MATCH, 1f);
      if (i > 0) {
        lp.leftMargin = GAP;
      }
      row.addView(buttons[i], lp);
    }
    LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(MATCH, height);
    lp.setMargins(MARGIN, 0, MARGIN, bottom);
    column.addView(row, lp);
  }

  /** A rounded fill, the surface of a button or a card. */
  public static GradientDrawable rounded(int color, int radius) {
    GradientDrawable d = new GradientDrawable();
    d.setColor(color);
    d.setCornerRadius(radius);
    return d;
  }
}
