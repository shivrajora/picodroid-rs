// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageService;
import claudeusage.data.UsageSnapshot;
import claudeusage.util.TimeFormat;
import picodroid.content.Context;
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

  /** The first paint takes one card per tick: both at once cost the RP2350 57 ms. */
  private int paintStep;

  private final String all;
  private final String noCaps;
  private final String resetsIn;
  private final String noTranscripts;
  private final String share;

  ModelsPage(Context ctx, Palette palette) {
    super(ctx, palette);
    all = ctx.getString(R.string.models_all);
    noCaps = ctx.getString(R.string.models_no_caps);
    resetsIn = ctx.getString(R.string.resets_in);
    noTranscripts = ctx.getString(R.string.no_transcripts);
    share = ctx.getString(R.string.models_share);
  }

  @Override
  int titleRes() {
    return R.string.page_models;
  }

  /**
   * A card's title row and note, then one meter row per step: three rows at once cost the RP2350
   * 120 ms.
   */
  private static final int STEPS_PER_CARD = 1 + UsageSnapshot.MAX_MODELS;

  @Override
  boolean buildNext() {
    int s = step++;
    if (s == 0) {
      capsCard = Ui.card(ctx, root, 2, CARD_HEIGHT, palette.card);
      Ui.label(
          ctx,
          capsCard,
          ctx.getString(R.string.models_weekly_limit),
          Ui.CARD_PAD,
          7,
          palette.muted);
      capsNote = note(capsCard);
      return true;
    }
    if (s < STEPS_PER_CARD) {
      row(capsCard, caps, s - 1);
      return true;
    }
    if (s == STEPS_PER_CARD) {
      mixCard = Ui.card(ctx, root, 2 + CARD_HEIGHT + 4, CARD_HEIGHT, palette.card);
      Ui.label(
          ctx, mixCard, ctx.getString(R.string.models_tokens_week), Ui.CARD_PAD, 7, palette.muted);
      mixNote = note(mixCard);
      return true;
    }
    int i = s - STEPS_PER_CARD - 1;
    row(mixCard, mix, i);
    return i + 1 < mix.length;
  }

  private Line note(FrameLayout card) {
    return new Line(
        Ui.labelRight(ctx, card, "", 150, 7, Ui.CARD_WIDTH - 150 - Ui.CARD_PAD, palette.faint),
        "",
        palette.faint);
  }

  private void row(FrameLayout card, MeterRow[] into, int i) {
    into[i] = new MeterRow(ctx, palette, card, FIRST_ROW_Y + i * MeterRow.HEIGHT);
  }

  @Override
  boolean paintNext(UsageService repo, long nowMs) {
    UsageSnapshot s = repo.snapshot();
    if (s == null) {
      return false;
    }
    boolean stale = !repo.isFresh();
    if (paintStep++ == 0) {
      updateCaps(s, stale, nowMs);
      return true;
    }
    updateMix(s);
    return false;
  }

  @Override
  void update(UsageService repo, long nowMs) {
    UsageSnapshot s = repo.snapshot();
    if (s == null) {
      return;
    }
    boolean stale = !repo.isFresh();
    updateCaps(s, stale, nowMs);
    updateMix(s);
  }

  private void updateCaps(UsageSnapshot s, boolean stale, long nowMs) {
    // Row 0 is always the all-models cap, so the card is never empty on a plan without per-model
    // caps; the rest are whatever the account reports.
    caps[0].show(all, s.weeklyPct, palette.severity(s.weeklyPct), stale);
    for (int i = 1; i < caps.length; i++) {
      int m = i - 1;
      if (m < s.modelCount) {
        int pct = s.modelPct[m];
        caps[i].show(s.modelName[m], pct, palette.severity(pct), stale);
      } else {
        caps[i].clear();
      }
    }
    long leftMs = s.weeklyReset > 0 ? s.weeklyReset * 1000L - nowMs : -1;
    if (s.modelCount == 0) {
      capsNote.show(noCaps, palette.faint);
    } else {
      capsNote.show(
          leftMs > 0 ? String.format(resetsIn, TimeFormat.duration(leftMs)) : "", palette.faint);
    }
  }

  private void updateMix(UsageSnapshot s) {
    for (int i = 0; i < mix.length; i++) {
      if (i < s.mixCount) {
        mix[i].show(s.mixName[i], s.mixPct[i], palette.clay, false);
      } else {
        mix[i].clear();
      }
    }
    mixNote.show(s.mixCount == 0 ? noTranscripts : share, palette.faint);
  }
}
