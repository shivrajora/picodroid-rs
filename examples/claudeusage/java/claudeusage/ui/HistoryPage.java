// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageSnapshot;
import claudeusage.util.TimeFormat;
import picodroid.view.View;
import picodroid.widget.TextView;

/** Today's totals and a week of daily token counts, from the PC's local transcripts. */
final class HistoryPage extends UsagePage {
  private TextView tokensToday;
  private TextView apiValue;
  private TextView messages;
  private TextView peak;
  private WeekChart week;

  @Override
  int titleRes() {
    return R.string.page_history;
  }

  @Override
  int layoutRes() {
    return R.layout.page_history;
  }

  @Override
  void onBind(View page) {
    tokensToday = page.findViewById(R.id.tokens_today);
    apiValue = page.findViewById(R.id.api_value);
    messages = page.findViewById(R.id.messages);
    peak = page.findViewById(R.id.peak);
    week = page.findViewById(R.id.week);
  }

  @Override
  void update(UsageUiState state, long nowMs) {
    UsageSnapshot s = state.snapshot;
    int ink = state.fresh ? palette.text : palette.muted;
    tokensToday.setText(TimeFormat.tokens(s.todayTokensK));
    tokensToday.setTextColor(ink);
    apiValue.setText(
        s.todayCents < 0
            ? getString(R.string.dash)
            : getString(R.string.history_estimate, TimeFormat.dollars(s.todayCents)));
    apiValue.setTextColor(ink);
    messages.setText(String.valueOf(s.todayMessages));
    messages.setTextColor(ink);

    if (!s.hasHistory) {
      peak.setText(getString(R.string.no_transcripts));
      return;
    }
    int max = 0;
    for (int d = 0; d < s.dayTokensK.length; d++) {
      if (s.dayTokensK[d] > max) {
        max = s.dayTokensK[d];
      }
    }
    peak.setText(max > 0 ? getString(R.string.history_peak, TimeFormat.tokens(max)) : "");
    if (week.set(s.dayTokensK, s.dayLetters)) {
      week.invalidate();
    }
  }
}
