// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageSnapshot;
import picodroid.view.View;

/** The home screen: the 5-hour session and the weekly cap, as two ring gauges side by side. */
final class LimitsPage extends UsagePage {
  private static final long SESSION_SECONDS = 5L * 3600L;
  private static final long WEEK_SECONDS = 7L * 86_400L;

  private LimitCard session;
  private LimitCard weekly;

  @Override
  int titleRes() {
    return R.string.page_limits;
  }

  @Override
  int layoutRes() {
    return R.layout.page_limits;
  }

  @Override
  void onBind(View page) {
    session =
        new LimitCard(ctx, page.findViewById(R.id.session), R.string.card_session, SESSION_SECONDS);
    weekly = new LimitCard(ctx, page.findViewById(R.id.weekly), R.string.card_weekly, WEEK_SECONDS);
  }

  @Override
  void update(UsageUiState state, long nowMs) {
    UsageSnapshot s = state.snapshot;
    boolean stale = !state.fresh;
    session.show(s.sessionPct, s.sessionReset, nowMs, stale);
    weekly.show(s.weeklyPct, s.weeklyReset, nowMs, stale);
  }
}
