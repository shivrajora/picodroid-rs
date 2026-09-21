// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.UsageRepository;
import claudeusage.data.UsageSnapshot;
import claudeusage.util.TimeFormat;
import picodroid.widget.FrameLayout;

/** Today's totals and a week of daily token counts, from the PC's local transcripts. */
final class HistoryPage extends Page {
  private static final int STATS_HEIGHT = 60;
  private static final int CHART_Y = 2 + STATS_HEIGHT + 4;
  private static final int CHART_CARD_HEIGHT = Ui.PAGE_HEIGHT - CHART_Y - 2;
  private static final int DAYS = UsageSnapshot.DAYS;
  private static final int COLUMN = 40;
  private static final int BAR_WIDTH = 26;
  private static final int BAR_MAX = 58;
  private static final int BASELINE = 98;

  /** Column origins: the first caption is the widest, so the columns are not equal. */
  private static final int[] STAT_X = {Ui.CARD_PAD, 124, 214};

  private FrameLayout stats;
  private FrameLayout chart;
  private final Line[] statValue = new Line[3];
  private Line peak;
  private final FrameLayout[] bars = new FrameLayout[DAYS];
  private final Line[] letters = new Line[DAYS];
  private final int[] shownHeight = new int[DAYS];
  private static final String[] CAPTIONS = {"tokens today", "API value", "messages"};

  @Override
  String title() {
    return "History";
  }

  @Override
  boolean buildNext() {
    int s = step++;
    switch (s) {
      case 0:
        stats = Ui.card(root, 2, STATS_HEIGHT);
        return true;
      case 1:
      case 2:
      case 3:
        int i = s - 1;
        int x = STAT_X[i];
        statValue[i] = new Line(Ui.label(stats, "--", x, 11, Palette.TEXT), "--", Palette.TEXT);
        Ui.label(stats, CAPTIONS[i], x, 31, Palette.MUTED);
        return true;
      case 4:
        chart = Ui.card(root, CHART_Y, CHART_CARD_HEIGHT);
        Ui.label(chart, "Last 7 days", Ui.CARD_PAD, 7, Palette.MUTED);
        peak =
            new Line(
                Ui.labelRight(chart, "", 150, 7, Ui.CARD_WIDTH - 150 - Ui.CARD_PAD, Palette.FAINT),
                "",
                Palette.FAINT);
        return true;
      case 5:
        for (int d = 0; d < DAYS; d++) {
          bars[d] = Ui.box(barX(d), BASELINE - 2, BAR_WIDTH, 2, Palette.TRACK, 1);
          shownHeight[d] = -1;
          chart.addView(bars[d]);
        }
        return true;
      default:
        int from = (s - 6) * 4;
        int to = from + 4 > DAYS ? DAYS : from + 4;
        for (int d = from; d < to; d++) {
          letters[d] =
              new Line(
                  Ui.labelCentred(chart, "", columnX(d), BASELINE + 3, COLUMN, Palette.MUTED),
                  "",
                  Palette.MUTED);
        }
        return to < DAYS;
    }
  }

  private static int columnX(int day) {
    return Ui.CARD_PAD + day * COLUMN;
  }

  private static int barX(int day) {
    return columnX(day) + (COLUMN - BAR_WIDTH) / 2;
  }

  @Override
  void update(UsageRepository repo, long nowMs) {
    UsageSnapshot s = repo.snapshot();
    if (s == null) {
      return;
    }
    int ink = repo.isFresh() ? Palette.TEXT : Palette.MUTED;
    statValue[0].show(TimeFormat.tokens(s.todayTokensK), ink);
    statValue[1].show(s.todayCents < 0 ? "--" : "~" + TimeFormat.dollars(s.todayCents), ink);
    statValue[2].show(String.valueOf(s.todayMessages), ink);

    if (!s.hasHistory) {
      peak.show("no transcripts yet", Palette.FAINT);
      return;
    }
    int max = 0;
    for (int d = 0; d < DAYS; d++) {
      if (s.dayTokensK[d] > max) {
        max = s.dayTokensK[d];
      }
    }
    peak.show(max > 0 ? "peak " + TimeFormat.tokens(max) : "", Palette.FAINT);
    for (int d = 0; d < DAYS; d++) {
      boolean today = d == DAYS - 1;
      int h =
          max == 0 || s.dayTokensK[d] == 0
              ? 2
              : 4 + (int) ((long) s.dayTokensK[d] * (BAR_MAX - 4) / max);
      if (h != shownHeight[d]) {
        shownHeight[d] = h;
        bars[d].setSize(BAR_WIDTH, h);
        bars[d].setPosition(barX(d), BASELINE - h);
        Ui.fill(
            bars[d],
            h <= 2 ? Palette.TRACK : (today ? Palette.CLAY : Palette.BAR_PAST),
            h <= 2 ? 1 : 4);
      }
      String letter = d < s.dayLetters.length() ? s.dayLetters.substring(d, d + 1) : "";
      letters[d].show(letter, today ? Palette.TEXT : Palette.MUTED);
    }
  }
}
