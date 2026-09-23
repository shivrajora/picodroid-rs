// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.util.TimeFormat;
import picodroid.content.Context;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;

/** One usage window: big percentage, reset countdown, and a bar with a pace marker. */
final class LimitCard {
  static final int HEIGHT = 92;

  private final Context ctx;
  private final Palette p;
  private final FrameLayout card;
  private final int captionRes;
  private final long windowSeconds;
  private Line reset;
  private Line detail;
  private BigNumber number;
  private BarView bar;

  // Resolved once: show() runs every second.
  private final String resetsIn;
  private final String resetting;
  private final String windowHasReset;
  private final String waitingForSync;
  private final String notReported;
  private final String aheadOfPace;
  private final String windowGone;

  LimitCard(Context ctx, Palette p, ViewGroup parent, int y, int captionRes, long windowSeconds) {
    this.ctx = ctx;
    this.p = p;
    this.captionRes = captionRes;
    this.windowSeconds = windowSeconds;
    card = Ui.card(ctx, parent, y, HEIGHT, p.card);
    resetsIn = ctx.getString(R.string.resets_in);
    resetting = ctx.getString(R.string.resetting);
    windowHasReset = ctx.getString(R.string.window_has_reset);
    waitingForSync = ctx.getString(R.string.waiting_for_sync);
    notReported = ctx.getString(R.string.not_reported);
    aheadOfPace = ctx.getString(R.string.ahead_of_pace);
    windowGone = ctx.getString(R.string.window_gone);
  }

  /** Build steps two and three: everything inside the card, in two halves. */
  void fillText() {
    int inner = Ui.CARD_WIDTH - 2 * Ui.CARD_PAD;
    Ui.label(ctx, card, ctx.getString(captionRes), Ui.CARD_PAD, 7, p.muted);
    reset = new Line(Ui.labelRight(ctx, card, "", 120, 7, inner - 108, p.muted), "", p.muted);
    detail = new Line(Ui.labelRight(ctx, card, "", 120, 46, inner - 108, p.faint), "", p.faint);
  }

  void fillGauge() {
    int inner = Ui.CARD_WIDTH - 2 * Ui.CARD_PAD;
    number = new BigNumber(ctx, card, Ui.CARD_PAD, 26);
    bar = new BarView(ctx, p, card, Ui.CARD_PAD, 75, inner, 9, true);
  }

  void show(int pct, long resetEpochS, long nowMs, boolean stale) {
    long leftMs = resetEpochS > 0 ? resetEpochS * 1000L - nowMs : -1;
    // Offline across a reset: the old percentage is now known to be wrong, so stop showing it.
    boolean expired = stale && resetEpochS > 0 && leftMs <= 0;
    if (pct < 0 || expired) {
      number.showPercent(-1, stale);
      bar.show(-1, p.good, p.goodDeep, -1, false);
      reset.show(expired ? windowHasReset : "", p.muted);
      detail.show(expired ? waitingForSync : notReported, p.faint);
      return;
    }
    int elapsed = -1;
    if (leftMs >= 0) {
      long gone = windowSeconds - leftMs / 1000L;
      elapsed = gone <= 0 ? 0 : (gone >= windowSeconds ? 100 : (int) (gone * 100L / windowSeconds));
    }
    number.showPercent(pct, stale);
    bar.show(pct, p.severity(pct), p.severityDeep(pct), elapsed, stale);
    reset.show(
        leftMs > 0 ? String.format(resetsIn, TimeFormat.duration(leftMs)) : resetting, p.muted);
    if (elapsed < 0) {
      detail.show("", p.faint);
    } else if (pct > elapsed + 10 && pct >= p.warnFrom) {
      detail.show(aheadOfPace, stale ? p.faint : p.severity(pct));
    } else {
      detail.show(String.format(windowGone, elapsed), p.faint);
    }
  }
}
