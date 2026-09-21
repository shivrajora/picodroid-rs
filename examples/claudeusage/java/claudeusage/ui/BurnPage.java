// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.UsageRepository;
import claudeusage.data.UsageSnapshot;
import claudeusage.util.TimeFormat;
import picodroid.view.View;
import picodroid.widget.FrameLayout;

/** How fast the session is filling, when it would run out, and the last hour's trend. */
final class BurnPage extends Page {
  private static final int CARD_HEIGHT = 92;
  private static final int BARS = UsageRepository.TREND_SLOTS;
  private static final int BAR_WIDTH = 8;
  private static final int BAR_PITCH = 11;
  private static final int CHART_HEIGHT = 50;
  private static final int BASELINE = 84;
  private static final int BARS_PER_STEP = 8;

  /** Below this many minutes to the limit the projection turns red. */
  private static final int ETA_URGENT_MIN = 30;

  private FrameLayout rateCard;
  private FrameLayout trendCard;
  private BigNumber rate;
  private FrameLayout unitHolder;
  private Line eta;
  private Line reset;
  private Line now;
  private final FrameLayout[] bars = new FrameLayout[BARS];
  private final int[] shown = new int[BARS];
  private int unitX = -1;

  @Override
  String title() {
    return "Burn rate";
  }

  @Override
  boolean buildNext() {
    int s = step++;
    switch (s) {
      case 0:
        rateCard = Ui.card(root, 2, CARD_HEIGHT);
        Ui.label(rateCard, "Session burn rate", Ui.CARD_PAD, 7, Palette.MUTED);
        rate = new BigNumber(rateCard, Ui.CARD_PAD, 30);
        return true;
      case 1:
        // The unit follows the number, whose width changes, so it lives in a movable holder.
        unitHolder = Ui.group(Ui.CARD_PAD + 90, 50, 70, Ui.LINE_HEIGHT, Palette.CARD);
        rateCard.addView(unitHolder);
        Ui.label(unitHolder, "% / hour", 0, 0, Palette.MUTED);
        eta = right(rateCard, 32);
        reset = right(rateCard, 54);
        return true;
      case 2:
        trendCard = Ui.card(root, 2 + CARD_HEIGHT + 4, CARD_HEIGHT);
        Ui.label(trendCard, "Session, last hour", Ui.CARD_PAD, 7, Palette.MUTED);
        now = right(trendCard, 7);
        return true;
      default:
        int from = (s - 3) * BARS_PER_STEP;
        int to = from + BARS_PER_STEP > BARS ? BARS : from + BARS_PER_STEP;
        for (int i = from; i < to; i++) {
          bars[i] = Ui.box(barX(i), BASELINE - 2, BAR_WIDTH, 2, Palette.TRACK, 1);
          shown[i] = -1;
          trendCard.addView(bars[i]);
        }
        return to < BARS;
    }
  }

  private static Line right(FrameLayout card, int y) {
    return new Line(
        Ui.labelRight(card, "", 140, y, Ui.CARD_WIDTH - 140 - Ui.CARD_PAD, Palette.MUTED),
        "",
        Palette.MUTED);
  }

  private static int barX(int i) {
    return Ui.CARD_PAD + 8 + i * BAR_PITCH;
  }

  @Override
  void update(UsageRepository repo, long nowMs) {
    UsageSnapshot s = repo.snapshot();
    if (s == null) {
      return;
    }
    boolean stale = !repo.isFresh();

    int width = rate.showSigned(stale ? -1 : s.ratePerHour, false);
    int x = Ui.CARD_PAD + width + 8;
    if (x != unitX) {
      unitHolder.setPosition(x, 50);
      unitX = x;
    }

    if (stale) {
      eta.show("no live data", Palette.FAINT);
    } else if (s.sessionPct >= 100) {
      eta.show("limit reached", Palette.BAD);
    } else if (s.ratePerHour <= 0 || s.etaMinutes < 0) {
      eta.show("idle", Palette.GOOD);
    } else {
      long leftMin = s.sessionReset > 0 ? (s.sessionReset * 1000L - nowMs) / 60_000L : -1;
      if (leftMin >= 0 && s.etaMinutes > leftMin) {
        eta.show("resets before limit", Palette.GOOD);
      } else {
        eta.show(
            "limit in ~" + TimeFormat.duration(s.etaMinutes * 60_000L),
            s.etaMinutes < ETA_URGENT_MIN ? Palette.BAD : Palette.WARN);
      }
    }
    long leftMs = s.sessionReset > 0 ? s.sessionReset * 1000L - nowMs : -1;
    reset.show(leftMs > 0 ? "resets in " + TimeFormat.duration(leftMs) : "", Palette.MUTED);
    now.show(stale || s.sessionPct < 0 ? "" : "now " + s.sessionPct + "%", Palette.MUTED);

    // Newest sample at the right edge; slots not yet filled stay as stubs on the left.
    int count = repo.trendCount();
    for (int i = 0; i < BARS; i++) {
      int sample = i - (BARS - count);
      int value = sample >= 0 ? repo.trendAt(sample) : -1;
      if (value == shown[i]) {
        continue;
      }
      shown[i] = value;
      if (value < 0) {
        bars[i].setSize(BAR_WIDTH, 2);
        bars[i].setPosition(barX(i), BASELINE - 2);
        Ui.fill(bars[i], Palette.TRACK, 1);
      } else {
        int h = 3 + value * (CHART_HEIGHT - 3) / 100;
        bars[i].setSize(BAR_WIDTH, h);
        bars[i].setPosition(barX(i), BASELINE - h);
        Ui.fill(bars[i], Palette.severity(value), 2);
      }
      bars[i].setVisibility(View.VISIBLE);
    }
  }
}
