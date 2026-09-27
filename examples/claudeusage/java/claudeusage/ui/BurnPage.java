// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageService;
import claudeusage.data.UsageSnapshot;
import claudeusage.util.TimeFormat;
import picodroid.os.Bundle;
import picodroid.view.Gravity;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/** How fast the session is filling, when it would run out, and the last hour's trend. */
final class BurnPage extends UsagePage {
  private static final int CARD_HEIGHT = 92;
  private static final int BARS = UsageService.TREND_SLOTS;

  /** The trend chart's left edge and the bars' baseline, in the trend card. */
  private static final int CHART_X = Ui.CARD_PAD + 8;

  private static final int BASELINE = 84;

  /** Below this many minutes to the limit the projection turns red. */
  private static final int ETA_URGENT_MIN = 30;

  /** The rate row: the big number and its unit, bottom-aligned so the unit follows the number. */
  private static final int RATE_Y = 30;

  private static final int UNIT_GAP = 8;

  /**
   * Lifts the 14 px unit so its baseline meets the number's: the display face's box ends 11 px
   * below the baseline (12 px of descent, 1 trimmed), the unit's 3 px.
   */
  private static final int UNIT_BASELINE_LIFT = 8;

  private FrameLayout rateCard;
  private FrameLayout trendCard;
  private Line rate;
  private Line eta;
  private Line reset;
  private Line now;
  private TrendChart chart;

  private String noLiveData;
  private String limitReached;
  private String idle;
  private String resetsFirst;
  private String limitIn;
  private String resetsIn;
  private String nowFmt;

  @Override
  public void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    noLiveData = getString(R.string.burn_no_live_data);
    limitReached = getString(R.string.burn_limit_reached);
    idle = getString(R.string.burn_idle);
    resetsFirst = getString(R.string.burn_resets_first);
    limitIn = getString(R.string.burn_limit_in);
    resetsIn = getString(R.string.resets_in);
    nowFmt = getString(R.string.burn_now);
  }

  @Override
  int titleRes() {
    return R.string.page_burn;
  }

  @Override
  boolean buildNext() {
    int s = step++;
    switch (s) {
      case 0:
        rateCard = Ui.card(ctx, root, 2, CARD_HEIGHT, palette.card);
        Ui.label(ctx, rateCard, ctx.getString(R.string.burn_title), Ui.CARD_PAD, 7, palette.muted);
        return true;
      case 1:
        // The number and its unit share a row, so the unit follows the number's width.
        LinearLayout rateRow =
            Ui.row(
                ctx,
                Ui.CARD_PAD,
                RATE_Y,
                Ui.CARD_WIDTH - 2 * Ui.CARD_PAD,
                Ui.DISPLAY_BOX,
                Gravity.LEFT | Gravity.BOTTOM);
        rateRow.setSpacing(UNIT_GAP);
        TextView number = new TextView(ctx);
        number.setTextColor(palette.text);
        number.setSingleLine();
        number.setTextSize(Ui.DISPLAY_SIZE);
        number.setIncludeFontPadding(false);
        rateRow.addView(number);
        rate = new Line(number, "", palette.text);
        TextView unit = new TextView(ctx);
        unit.setText(ctx.getString(R.string.burn_unit));
        unit.setTextColor(palette.muted);
        unit.setSingleLine();
        unit.setPadding(0, 0, 0, UNIT_BASELINE_LIFT);
        rateRow.addView(unit);
        rateCard.addView(rateRow);
        return true;
      case 2:
        // Each right-aligned line is a row and a label: two of them are a step of their own.
        eta = right(rateCard, 32);
        reset = right(rateCard, 54);
        return true;
      case 3:
        trendCard = Ui.card(ctx, root, 2 + CARD_HEIGHT + 4, CARD_HEIGHT, palette.card);
        Ui.label(ctx, trendCard, ctx.getString(R.string.burn_trend), Ui.CARD_PAD, 7, palette.muted);
        now = right(trendCard, 7);
        return true;
      default:
        // One view draws every bar, in one step; a view per bar took six steps of four.
        chart = new TrendChart(ctx, palette, BARS);
        chart.setPosition(CHART_X, BASELINE - TrendChart.HEIGHT);
        trendCard.addView(chart);
        return false;
    }
  }

  private Line right(FrameLayout card, int y) {
    return new Line(
        Ui.labelRight(ctx, card, "", 140, y, Ui.CARD_WIDTH - 140 - Ui.CARD_PAD, palette.muted),
        "",
        palette.muted);
  }

  @Override
  void update(UsageService repo, long nowMs) {
    UsageSnapshot s = repo.snapshot();
    if (s == null) {
      return;
    }
    boolean stale = !repo.isFresh();

    int perHour = stale ? -1 : s.ratePerHour;
    rate.show(perHour < 0 ? "--" : "+" + perHour, palette.text);

    if (stale) {
      eta.show(noLiveData, palette.faint);
    } else if (s.sessionPct >= 100) {
      eta.show(limitReached, palette.bad);
    } else if (s.ratePerHour <= 0 || s.etaMinutes < 0) {
      eta.show(idle, palette.good);
    } else {
      long leftMin = s.sessionReset > 0 ? (s.sessionReset * 1000L - nowMs) / 60_000L : -1;
      if (leftMin >= 0 && s.etaMinutes > leftMin) {
        eta.show(resetsFirst, palette.good);
      } else {
        eta.show(
            String.format(limitIn, TimeFormat.duration(s.etaMinutes * 60_000L)),
            s.etaMinutes < ETA_URGENT_MIN ? palette.bad : palette.warn);
      }
    }
    long leftMs = s.sessionReset > 0 ? s.sessionReset * 1000L - nowMs : -1;
    reset.show(
        leftMs > 0 ? String.format(resetsIn, TimeFormat.duration(leftMs)) : "", palette.muted);
    now.show(stale || s.sessionPct < 0 ? "" : String.format(nowFmt, s.sessionPct), palette.muted);

    // Newest sample at the right edge; slots not yet filled stay as stubs on the left.
    int count = repo.trendCount();
    boolean changed = false;
    for (int i = 0; i < BARS; i++) {
      int sample = i - (BARS - count);
      changed |= chart.set(i, sample >= 0 ? repo.trendAt(sample) : -1);
    }
    if (changed) {
      chart.invalidate();
    }
  }
}
