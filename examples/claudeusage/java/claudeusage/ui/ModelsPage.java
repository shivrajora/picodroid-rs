// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.UsageRepository;
import claudeusage.data.UsageSnapshot;
import claudeusage.util.TimeFormat;
import picodroid.widget.FrameLayout;

/** Per-model weekly caps, where the plan has them, and which models the week's tokens went to. */
final class ModelsPage extends Page {
  private static final int CARD_HEIGHT = 92;
  private static final int FIRST_ROW_Y = 27;

  private FrameLayout capsCard;
  private FrameLayout mixCard;
  private Line capsNote;
  private Line mixNote;
  private final MeterRow[] caps = new MeterRow[UsageSnapshot.MAX_MODELS];
  private final MeterRow[] mix = new MeterRow[UsageSnapshot.MAX_MODELS];

  @Override
  String title() {
    return "Models";
  }

  @Override
  boolean buildNext() {
    switch (step++) {
      case 0:
        capsCard = Ui.card(root, 2, CARD_HEIGHT);
        Ui.label(capsCard, "Weekly limit", Ui.CARD_PAD, 7, Palette.MUTED);
        capsNote = note(capsCard);
        return true;
      case 1:
        rows(capsCard, caps);
        return true;
      case 2:
        mixCard = Ui.card(root, 2 + CARD_HEIGHT + 4, CARD_HEIGHT);
        Ui.label(mixCard, "Tokens, last 7 days", Ui.CARD_PAD, 7, Palette.MUTED);
        mixNote = note(mixCard);
        return true;
      default:
        rows(mixCard, mix);
        return false;
    }
  }

  private static Line note(FrameLayout card) {
    return new Line(
        Ui.labelRight(card, "", 150, 7, Ui.CARD_WIDTH - 150 - Ui.CARD_PAD, Palette.FAINT),
        "",
        Palette.FAINT);
  }

  private static void rows(FrameLayout card, MeterRow[] into) {
    for (int i = 0; i < into.length; i++) {
      into[i] = new MeterRow(card, FIRST_ROW_Y + i * MeterRow.HEIGHT);
    }
  }

  @Override
  void update(UsageRepository repo, long nowMs) {
    UsageSnapshot s = repo.snapshot();
    if (s == null) {
      return;
    }
    boolean stale = !repo.isFresh();

    // Row 0 is always the all-models cap, so the card is never empty on a plan without per-model
    // caps; the rest are whatever the account reports.
    caps[0].show(
        "All",
        s.weeklyPct,
        Palette.severity(s.weeklyPct),
        Palette.severityDeep(s.weeklyPct),
        stale);
    for (int i = 1; i < caps.length; i++) {
      int m = i - 1;
      if (m < s.modelCount) {
        int pct = s.modelPct[m];
        caps[i].show(s.modelName[m], pct, Palette.severity(pct), Palette.severityDeep(pct), stale);
      } else {
        caps[i].clear();
      }
    }
    long leftMs = s.weeklyReset > 0 ? s.weeklyReset * 1000L - nowMs : -1;
    if (s.modelCount == 0) {
      capsNote.show("no per-model caps", Palette.FAINT);
    } else {
      capsNote.show(leftMs > 0 ? "resets in " + TimeFormat.duration(leftMs) : "", Palette.FAINT);
    }

    for (int i = 0; i < mix.length; i++) {
      if (i < s.mixCount) {
        mix[i].show(s.mixName[i], s.mixPct[i], Palette.CLAY, Palette.CLAY_DEEP, false);
      } else {
        mix[i].clear();
      }
    }
    mixNote.show(s.mixCount == 0 ? "no transcripts yet" : "share of tokens", Palette.FAINT);
  }
}
