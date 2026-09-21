// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.UsageRepository;
import claudeusage.data.UsageSnapshot;

/** The home screen: the 5-hour session and the weekly cap. */
final class LimitsPage extends Page {
  private static final long SESSION_SECONDS = 5L * 3600L;
  private static final long WEEK_SECONDS = 7L * 86_400L;

  private LimitCard session;
  private LimitCard weekly;

  @Override
  String title() {
    return "Limits";
  }

  @Override
  boolean buildNext() {
    switch (step++) {
      case 0:
        session = new LimitCard(root, 2, "5-hour session", SESSION_SECONDS);
        return true;
      case 1:
        session.fill();
        return true;
      case 2:
        weekly = new LimitCard(root, 2 + LimitCard.HEIGHT + 4, "Weekly, all models", WEEK_SECONDS);
        return true;
      default:
        weekly.fill();
        return false;
    }
  }

  @Override
  void update(UsageRepository repo, long nowMs) {
    UsageSnapshot s = repo.snapshot();
    if (s == null) {
      return;
    }
    boolean stale = !repo.isFresh();
    session.show(s.sessionPct, s.sessionReset, nowMs, stale);
    weekly.show(s.weeklyPct, s.weeklyReset, nowMs, stale);
  }
}
