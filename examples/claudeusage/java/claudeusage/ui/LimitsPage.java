// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageService;
import claudeusage.data.UsageSnapshot;
import picodroid.content.Context;

/** The home screen: the 5-hour session and the weekly cap, as two ring gauges side by side. */
final class LimitsPage extends Page {
  private static final long SESSION_SECONDS = 5L * 3600L;
  private static final long WEEK_SECONDS = 7L * 86_400L;

  private LimitCard session;
  private LimitCard weekly;

  LimitsPage(Context ctx, Palette p) {
    super(ctx, p);
  }

  @Override
  int titleRes() {
    return R.string.page_limits;
  }

  @Override
  boolean buildNext() {
    switch (step++) {
      case 0:
        session = new LimitCard(ctx, p, root, Ui.MARGIN, R.string.card_session, SESSION_SECONDS);
        return true;
      case 1:
        session.fillRing();
        return true;
      case 2:
        session.fillCentre();
        return true;
      case 3:
        session.fillFooter();
        return true;
      case 4:
        weekly =
            new LimitCard(
                ctx,
                p,
                root,
                Ui.MARGIN + LimitCard.WIDTH + LimitCard.GAP,
                R.string.card_weekly,
                WEEK_SECONDS);
        return true;
      case 5:
        weekly.fillRing();
        return true;
      case 6:
        weekly.fillCentre();
        return true;
      default:
        weekly.fillFooter();
        return false;
    }
  }

  @Override
  void update(UsageService repo, long nowMs) {
    UsageSnapshot s = repo.snapshot();
    if (s == null) {
      return;
    }
    boolean stale = !repo.isFresh();
    session.show(s.sessionPct, s.sessionReset, nowMs, stale);
    weekly.show(s.weeklyPct, s.weeklyReset, nowMs, stale);
  }
}
