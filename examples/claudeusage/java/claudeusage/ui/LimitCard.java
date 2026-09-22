// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.util.TimeFormat;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;

/** One usage window: big percentage, reset countdown, and a bar with a pace marker. */
final class LimitCard {
  static final int HEIGHT = 92;

  private final FrameLayout card;
  private final String caption;
  private final long windowSeconds;
  private Line reset;
  private Line detail;
  private BigNumber number;
  private BarView bar;

  LimitCard(ViewGroup parent, int y, String caption, long windowSeconds) {
    this.caption = caption;
    this.windowSeconds = windowSeconds;
    card = Ui.card(parent, y, HEIGHT);
  }

  /** Build steps two and three: everything inside the card, in two halves. */
  void fillText() {
    int inner = Ui.CARD_WIDTH - 2 * Ui.CARD_PAD;
    Ui.label(card, caption, Ui.CARD_PAD, 7, Palette.MUTED);
    reset =
        new Line(Ui.labelRight(card, "", 120, 7, inner - 108, Palette.MUTED), "", Palette.MUTED);
    detail =
        new Line(Ui.labelRight(card, "", 120, 46, inner - 108, Palette.FAINT), "", Palette.FAINT);
  }

  void fillGauge() {
    int inner = Ui.CARD_WIDTH - 2 * Ui.CARD_PAD;
    number = new BigNumber(card, Ui.CARD_PAD, 26);
    bar = new BarView(card, Ui.CARD_PAD, 75, inner, 9, true);
  }

  void show(int pct, long resetEpochS, long nowMs, boolean stale) {
    long leftMs = resetEpochS > 0 ? resetEpochS * 1000L - nowMs : -1;
    // Offline across a reset: the old percentage is now known to be wrong, so stop showing it.
    boolean expired = stale && resetEpochS > 0 && leftMs <= 0;
    if (pct < 0 || expired) {
      number.showPercent(-1, stale);
      bar.show(-1, Palette.GOOD, Palette.GOOD_DEEP, -1, false);
      reset.show(expired ? "window has reset" : "", Palette.MUTED);
      detail.show(expired ? "waiting for sync" : "not reported", Palette.FAINT);
      return;
    }
    int elapsed = -1;
    if (leftMs >= 0) {
      long gone = windowSeconds - leftMs / 1000L;
      elapsed = gone <= 0 ? 0 : (gone >= windowSeconds ? 100 : (int) (gone * 100L / windowSeconds));
    }
    number.showPercent(pct, stale);
    bar.show(pct, Palette.severity(pct), Palette.severityDeep(pct), elapsed, stale);
    reset.show(
        leftMs > 0 ? "resets in " + TimeFormat.duration(leftMs) : "resetting", Palette.MUTED);
    if (elapsed < 0) {
      detail.show("", Palette.FAINT);
    } else if (pct > elapsed + 10 && pct >= Palette.WARN_FROM) {
      detail.show("ahead of pace", stale ? Palette.FAINT : Palette.severity(pct));
    } else {
      detail.show(elapsed + "% of window gone", Palette.FAINT);
    }
  }
}
