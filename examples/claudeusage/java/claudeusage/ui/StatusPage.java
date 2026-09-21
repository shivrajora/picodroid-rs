// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.LinkState;
import claudeusage.data.UsageFetcher;
import claudeusage.data.UsageRepository;
import picodroid.widget.FrameLayout;

/**
 * Shown instead of the data screens while there has never been any data: says what is wrong, where
 * the bridge is expected, and when the next attempt is. It doubles as the setup screen, since a
 * wrong address is the likeliest first-boot problem.
 */
final class StatusPage extends Page {
  private FrameLayout card;
  private FrameLayout dot;
  private Line headline;
  private Line advice;
  private Line retry;
  private int shownDot;

  @Override
  String title() {
    return "Claude usage";
  }

  @Override
  boolean buildNext() {
    int inner = Ui.CARD_WIDTH;
    switch (step++) {
      case 0:
        card = Ui.card(root, 2, Ui.PAGE_HEIGHT - 4);
        dot = Ui.box(Ui.CARD_WIDTH / 2 - 6, 24, 12, 12, Palette.CLAY, 6);
        shownDot = Palette.CLAY;
        card.addView(dot);
        return true;
      case 1:
        headline = centred(48, Palette.TEXT, inner);
        advice = centred(70, Palette.MUTED, inner);
        return true;
      default:
        Ui.labelCentred(card, "Bridge  " + UsageFetcher.ADDRESS, 0, 108, inner, Palette.MUTED);
        retry = centred(130, Palette.CLAY, inner);
        Ui.labelCentred(card, "X  retry now", 0, 156, inner, Palette.FAINT);
        return false;
    }
  }

  private Line centred(int y, int color, int width) {
    return new Line(Ui.labelCentred(card, "", 0, y, width, color), "", color);
  }

  @Override
  void update(UsageRepository repo, long nowMs) {
    int state = repo.linkState();
    String err = repo.linkErr();
    headline.show(LinkState.shortText(state, err), Palette.TEXT);
    advice.show(LinkState.advice(state, err), Palette.MUTED);
    if (repo.isSyncing()) {
      retry.show("Contacting bridge", Palette.CLAY);
    } else if (state == LinkState.JOINING || state == LinkState.NO_WIFI) {
      retry.show("", Palette.CLAY);
    } else {
      int wait = repo.secondsToNextAttempt();
      retry.show(wait > 0 ? "Retrying in " + wait + "s" : "Retrying", Palette.CLAY);
    }
    int color = state == LinkState.JOINING ? Palette.CLAY : Palette.BAD;
    if (color != shownDot) {
      Ui.fill(dot, color, 6);
      shownDot = color;
    }
  }
}
