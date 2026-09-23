// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.util.TimeFormat;
import picodroid.content.Context;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;
import picodroid.widget.TextView;

/**
 * One usage window: a ring gauge with the reset countdown inside it, the big percentage in the
 * ring's open bottom, and a pace line under that. Two of these sit side by side on the Limits page.
 */
final class LimitCard {
  static final int GAP = 4;
  static final int WIDTH = (Ui.CARD_WIDTH - GAP) / 2;
  static final int HEIGHT = Ui.PAGE_HEIGHT - 4;

  // Card-relative geometry, pixel art for the 320x240 panel. The ring's rounded caps end 4 px
  // above the top of the big number's digits.
  private static final int CAPTION_Y = 4;
  private static final int RING_X = 23;
  private static final int RING_Y = 24;
  private static final int RING_DIAMETER = 104;
  private static final int RING_STROKE = 10;
  private static final int INNER_X = 33;
  private static final int INNER_WIDTH = 84;
  private static final int LINE1_Y = 57;
  private static final int LINE2_Y = 77;
  private static final int NUMBER_Y = 118;
  private static final int DETAIL_X = 4;
  private static final int DETAIL_WIDTH = WIDTH - 2 * DETAIL_X;
  private static final int DETAIL_Y = 165;

  private final Context ctx;
  private final Palette p;
  private final FrameLayout card;
  private final long windowSeconds;
  private Line resetsLabel;
  private Line countdown;
  private Line detail;
  private TextView number;
  private String numberText = "";
  private boolean numberDim;
  private RingView ring;

  // Resolved once: show() runs every second.
  private final String resetsIn;
  private final String resetting;
  private final String windowHasReset;
  private final String notReported;
  private final String aheadOfPace;
  private final String windowGone;

  /** Build step one: the card and its caption. */
  LimitCard(Context ctx, Palette p, ViewGroup parent, int x, int captionRes, long windowSeconds) {
    this.ctx = ctx;
    this.p = p;
    this.windowSeconds = windowSeconds;
    card = Ui.card(ctx, parent, x, 2, WIDTH, HEIGHT, p.card);
    Ui.labelCentred(ctx, card, ctx.getString(captionRes), 0, CAPTION_Y, WIDTH, p.muted);
    resetsIn = ctx.getString(R.string.resets_in_label);
    resetting = ctx.getString(R.string.resetting);
    windowHasReset = ctx.getString(R.string.window_has_reset);
    notReported = ctx.getString(R.string.not_reported);
    aheadOfPace = ctx.getString(R.string.ahead_of_pace);
    windowGone = ctx.getString(R.string.window_gone);
  }

  /** Build step two: the gauge and its pace tick. */
  void fillRing() {
    ring = new RingView(ctx, p, card, RING_X, RING_Y, RING_DIAMETER, RING_STROKE, true);
  }

  /** Build step three: the two lines inside the ring. */
  void fillCentre() {
    resetsLabel =
        new Line(
            Ui.labelCentred(ctx, card, "", INNER_X, LINE1_Y, INNER_WIDTH, p.faint), "", p.faint);
    countdown =
        new Line(
            Ui.labelCentred(ctx, card, "", INNER_X, LINE2_Y, INNER_WIDTH, p.muted), "", p.muted);
  }

  /** Build step four: the pace line and the big number. */
  void fillFooter() {
    detail =
        new Line(
            Ui.labelCentred(ctx, card, "", DETAIL_X, DETAIL_Y, DETAIL_WIDTH, p.faint), "", p.faint);
    number = Ui.labelCentred(ctx, card, "", 0, NUMBER_Y, WIDTH, Ui.DISPLAY_BOX, p.text);
    number.setTextSize(Ui.DISPLAY_SIZE);
    number.setIncludeFontPadding(false);
  }

  /** The big percentage: re-set only when it changes, dimmed while stale. */
  private void showNumber(String text, boolean dim) {
    if (!text.equals(numberText)) {
      number.setText(text);
      numberText = text;
    }
    if (dim != numberDim) {
      number.setAlpha(dim ? Ui.DIM : 1f);
      numberDim = dim;
    }
  }

  void show(int pct, long resetEpochS, long nowMs, boolean stale) {
    long leftMs = resetEpochS > 0 ? resetEpochS * 1000L - nowMs : -1;
    // Offline across a reset: the old percentage is now known to be wrong, so stop showing it.
    boolean expired = stale && resetEpochS > 0 && leftMs <= 0;
    if (pct < 0 || expired) {
      showNumber("--%", stale);
      ring.show(-1, p.good, -1, false);
      resetsLabel.show("", p.faint);
      countdown.show("", p.muted);
      detail.show(expired ? windowHasReset : notReported, p.faint);
      return;
    }
    int elapsed = -1;
    if (leftMs >= 0) {
      long gone = windowSeconds - leftMs / 1000L;
      elapsed = gone <= 0 ? 0 : (gone >= windowSeconds ? 100 : (int) (gone * 100L / windowSeconds));
    }
    showNumber(pct + "%", stale);
    ring.show(pct, p.severity(pct), elapsed, stale);
    if (leftMs > 0) {
      resetsLabel.show(resetsIn, p.faint);
      countdown.show(TimeFormat.duration(leftMs), p.muted);
    } else {
      resetsLabel.show("", p.faint);
      countdown.show(resetting, p.muted);
    }
    if (elapsed < 0) {
      detail.show("", p.faint);
    } else if (pct > elapsed + 10 && pct >= p.warnFrom) {
      detail.show(aheadOfPace, stale ? p.faint : p.severity(pct));
    } else {
      detail.show(String.format(windowGone, elapsed), p.faint);
    }
  }
}
