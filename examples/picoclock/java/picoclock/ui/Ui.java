// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.text.TextUtils;
import picodroid.view.Gravity;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * The one look every screen wears, and the sizes that make it tappable.
 *
 * <p>Everything here is sized for the EP-0172's 320x480 panel driven by a finger. The panel has a
 * touch controller and two buttons, and board.toml gives both buttons to the system (BACK and
 * HOME), so nothing in this app is reached by focus navigation — {@link #TAP_HEIGHT} is the floor
 * for anything a user has to hit, and it is set from the roughly 9 mm a fingertip covers at this
 * panel's 165 dpi.
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

  /** Panel width, from board.toml. Every screen is exactly this wide. */
  public static final int WIDTH = 320;

  /** Panel height, from board.toml. */
  public static final int HEIGHT = 480;

  /** The header band across the top of every screen. */
  public static final int HEADER_HEIGHT = 44;

  /** The smallest a control may be and still be hit reliably with a fingertip. */
  public static final int TAP_HEIGHT = 56;

  /** Gap between stacked controls. */
  public static final int GAP = 8;

  /** Side margin. */
  public static final int MARGIN = 12;

  /** The height of one line of body text. */
  public static final int LINE_HEIGHT = 22;

  /**
   * One shared drawable for every container meant to disappear into the background. An LVGL object
   * arrives with a one-pixel border, and only a Drawable clears it — {@code setBackgroundColor}
   * paints the fill and leaves the outline, which shows up as a box around every centred line. One
   * instance because a Drawable here is a description that is read at apply time, not state the
   * view keeps.
   */
  private static final GradientDrawable INVISIBLE =
      new GradientDrawable().setColor(BACKGROUND).setCornerRadius(0).setStroke(0, BACKGROUND);

  /** The drawable behind {@link #group}, for a view this class does not build itself. */
  public static GradientDrawable invisible() {
    return INVISIBLE;
  }

  private Ui() {}

  // ── Building blocks ────────────────────────────────────────────────────────

  /**
   * A container that paints nothing and lays nothing out: children keep the position they are
   * given. An LVGL object arrives with a surface and a border of its own, which is visible wherever
   * a group of views is meant to read as one thing rather than as a panel.
   */
  public static FrameLayout group(int x, int y, int width, int height) {
    FrameLayout f = new FrameLayout();
    f.setSize(width, height);
    f.setPosition(x, y);
    f.setPadding(0, 0, 0, 0);
    f.setBackground(INVISIBLE);
    return f;
  }

  /** A full-screen absolutely-positioned root. A LinearLayout would re-lay-out and clobber it. */
  public static FrameLayout screen() {
    FrameLayout root = new FrameLayout();
    root.setSize(WIDTH, HEIGHT);
    root.setPosition(0, 0);
    root.setPadding(0, 0, 0, 0);
    root.setBackground(INVISIBLE);
    return root;
  }

  /**
   * The header band: a title, and a back chevron when {@code onBack} is given. The chevron is a
   * tappable target of its own, since the carrier's BACK button is reachable only by feel.
   */
  public static View header(Activity a, String title, View.OnClickListener onBack) {
    FrameLayout bar = new FrameLayout();
    bar.setSize(WIDTH, HEADER_HEIGHT);
    bar.setPosition(0, 0);
    bar.setPadding(0, 0, 0, 0);
    bar.setBackgroundColor(SURFACE);

    TextView caption = new TextView();
    caption.setText(title);
    caption.setTextColor(TEXT);
    caption.setSingleLine();
    caption.setEllipsize(TextUtils.TruncateAt.END);
    caption.setPosition(onBack == null ? MARGIN : 56, 14);
    bar.addView(caption);

    if (onBack != null) {
      Button back = new Button("<");
      back.setSize(48, HEADER_HEIGHT);
      back.setPosition(0, 0);
      back.setTextColor(ACCENT);
      back.setBackground(new GradientDrawable().setColor(SURFACE).setCornerRadius(0));
      back.setOnClickListener(onBack);
      bar.addView(back);
    }
    return bar;
  }

  /** A line of body text at {@code (x, y)}, clipped to one line within the panel. */
  public static TextView label(String text, int x, int y, int color) {
    TextView t = new TextView();
    t.setText(text);
    t.setTextColor(color);
    t.setSingleLine();
    t.setEllipsize(TextUtils.TruncateAt.END);
    t.setPosition(x, y);
    return t;
  }

  /**
   * Adds a line of body text centred across the panel to {@code parent} and returns the label, for
   * a screen that has to retext it later. A label has no alignment of its own — the SDK's TextView
   * is the glyphs and nothing more — so the centring is a row with a centring gravity, which is
   * what Android would end up doing anyway.
   */
  public static TextView centred(ViewGroup parent, String text, int y, int color) {
    return centred(parent, text, 0, y, WIDTH, color);
  }

  /**
   * {@link #centred} within {@code width} pixels from {@code x} rather than across the panel. The
   * row paints a background, so it must not be made wider than the space it is meant to fill: a
   * full-width row dropped between two buttons covers them.
   */
  public static TextView centred(
      ViewGroup parent, String text, int x, int y, int width, int color) {
    LinearLayout row = new LinearLayout();
    row.setOrientation(LinearLayout.HORIZONTAL);
    row.setSize(width, LINE_HEIGHT);
    row.setPosition(x, y);
    row.setPadding(0, 0, 0, 0);
    row.setSpacing(0);
    row.setGravity(Gravity.CENTER);
    row.setBackground(INVISIBLE);
    TextView t = label(text, 0, 0, color);
    row.addView(t);
    parent.addView(row);
    return t;
  }

  /**
   * A filled, finger-sized button. The label takes whichever of black or the body colour reads
   * against {@code fill} — {@link #ACCENT} is the only fill here bright enough for black, and
   * getting that backwards leaves a button that looks empty rather than merely low-contrast.
   */
  public static Button button(String text, int x, int y, int width, int fill) {
    Button b = new Button(text);
    b.setSize(width, TAP_HEIGHT);
    b.setPosition(x, y);
    b.setTextColor(fill == ACCENT ? Color.BLACK : TEXT);
    b.setBackground(new GradientDrawable().setColor(fill).setCornerRadius(10));
    return b;
  }

  /** A card: the surface a row or a group of controls sits on. */
  public static FrameLayout card(int x, int y, int width, int height) {
    FrameLayout f = new FrameLayout();
    f.setSize(width, height);
    f.setPosition(x, y);
    f.setPadding(0, 0, 0, 0);
    f.setBackground(new GradientDrawable().setColor(SURFACE).setCornerRadius(10));
    return f;
  }

  /** A horizontal strip of evenly spaced controls: the widths for {@code n} across the panel. */
  public static int columnWidth(int n) {
    return (WIDTH - 2 * MARGIN - (n - 1) * GAP) / n;
  }

  /** The x of column {@code i} of {@code n}. */
  public static int columnX(int i, int n) {
    return MARGIN + i * (columnWidth(n) + GAP);
  }

  /** A vertical list with no padding, for a ScrollView's content. */
  public static LinearLayout list() {
    LinearLayout l = new LinearLayout();
    l.setOrientation(LinearLayout.VERTICAL);
    l.setPadding(0, 0, 0, 0);
    l.setSpacing(0);
    return l;
  }
}
