// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageSnapshot;
import claudeusage.util.TimeFormat;
import picodroid.view.View;
import picodroid.widget.TextView;

/** How fast the session is filling, when it would run out, and the last hour's trend. */
final class BurnPage extends UsagePage {
  /** Below this many minutes to the limit the projection turns red. */
  private static final int ETA_URGENT_MIN = 30;

  private TextView rate;
  private TextView eta;
  private TextView reset;
  private TextView now;
  private TrendChart chart;

  @Override
  int titleRes() {
    return R.string.page_burn;
  }

  @Override
  int layoutRes() {
    return R.layout.page_burn;
  }

  @Override
  void onBind(View page) {
    rate = page.findViewById(R.id.rate);
    eta = page.findViewById(R.id.eta);
    reset = page.findViewById(R.id.reset);
    now = page.findViewById(R.id.now);
    chart = page.findViewById(R.id.trend);
  }

  private void showEta(int textRes, int color) {
    eta.setText(getString(textRes));
    eta.setTextColor(color);
  }

  @Override
  void update(UsageUiState state, long nowMs) {
    UsageSnapshot s = state.snapshot;
    boolean stale = !state.fresh;

    int perHour = stale ? -1 : s.ratePerHour;
    rate.setText(perHour < 0 ? "--" : "+" + perHour);

    if (stale) {
      showEta(R.string.burn_no_live_data, palette.faint);
    } else if (s.sessionPct >= 100) {
      showEta(R.string.burn_limit_reached, palette.bad);
    } else if (s.ratePerHour <= 0 || s.etaMinutes < 0) {
      showEta(R.string.burn_idle, palette.good);
    } else {
      long leftMin = s.sessionReset > 0 ? (s.sessionReset * 1000L - nowMs) / 60_000L : -1;
      if (leftMin >= 0 && s.etaMinutes > leftMin) {
        showEta(R.string.burn_resets_first, palette.good);
      } else {
        eta.setText(getString(R.string.burn_limit_in, TimeFormat.duration(s.etaMinutes * 60_000L)));
        eta.setTextColor(s.etaMinutes < ETA_URGENT_MIN ? palette.bad : palette.warn);
      }
    }
    long leftMs = s.sessionReset > 0 ? s.sessionReset * 1000L - nowMs : -1;
    reset.setText(leftMs > 0 ? getString(R.string.resets_in, TimeFormat.duration(leftMs)) : "");
    now.setText(stale || s.sessionPct < 0 ? "" : getString(R.string.burn_now, s.sessionPct));

    // Newest sample at the right edge; slots not yet filled stay as stubs on the left.
    int[] trend = state.trend;
    int bars = chart.bars();
    boolean changed = false;
    for (int i = 0; i < bars; i++) {
      int sample = i - (bars - trend.length);
      changed |= chart.set(i, sample >= 0 ? trend[sample] : -1);
    }
    if (changed) {
      chart.invalidate();
    }
  }
}
