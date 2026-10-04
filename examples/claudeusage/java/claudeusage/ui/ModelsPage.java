// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageSnapshot;
import claudeusage.util.TimeFormat;
import picodroid.view.View;
import picodroid.widget.TextView;

/** Per-model weekly caps, where the plan has them, and which models the week's tokens went to. */
final class ModelsPage extends UsagePage {
  private static final int[] CAP_ROWS = {R.id.cap_0, R.id.cap_1, R.id.cap_2};
  private static final int[] MIX_ROWS = {R.id.mix_0, R.id.mix_1, R.id.mix_2};

  private TextView capsNote;
  private TextView mixNote;
  private final MeterRow[] caps = new MeterRow[UsageSnapshot.MAX_MODELS];
  private final MeterRow[] mix = new MeterRow[UsageSnapshot.MAX_MODELS];

  @Override
  int titleRes() {
    return R.string.page_models;
  }

  @Override
  int layoutRes() {
    return R.layout.page_models;
  }

  @Override
  void onBind(View page) {
    capsNote = page.findViewById(R.id.caps_note);
    mixNote = page.findViewById(R.id.mix_note);
    for (int i = 0; i < caps.length; i++) {
      caps[i] = new MeterRow(ctx, page.findViewById(CAP_ROWS[i]));
      mix[i] = new MeterRow(ctx, page.findViewById(MIX_ROWS[i]));
    }
  }

  @Override
  void update(UsageUiState state, long nowMs) {
    updateCaps(state.snapshot, !state.fresh, nowMs);
    updateMix(state.snapshot);
  }

  private void updateCaps(UsageSnapshot s, boolean stale, long nowMs) {
    // Row 0 is always the all-models cap, so the card is never empty on a plan without per-model
    // caps; the rest are whatever the account reports.
    caps[0].show(getString(R.string.models_all), s.weeklyPct, palette.severity(s.weeklyPct), stale);
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
      capsNote.setText(getString(R.string.models_no_caps));
    } else {
      capsNote.setText(
          leftMs > 0 ? getString(R.string.resets_in, TimeFormat.duration(leftMs)) : "");
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
    mixNote.setText(getString(s.mixCount == 0 ? R.string.no_transcripts : R.string.models_share));
  }
}
