// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.LinkState;
import claudeusage.data.UsageService;
import picodroid.content.Context;
import picodroid.widget.FrameLayout;

/**
 * Shown instead of the data screens while there has never been any data: says what is wrong, where
 * the bridge is expected, and when the next attempt is. It doubles as the setup screen, since a
 * wrong address is the likeliest first-boot problem.
 */
final class StatusPage extends Page {
  private final String bridgeAddress;
  private FrameLayout card;
  private FrameLayout dot;
  private Line headline;
  private Line advice;
  private Line retry;
  private int shownDot;

  private final String contacting;
  private final String retryingIn;
  private final String retrying;

  StatusPage(Context ctx, Palette palette, String bridgeAddress) {
    super(ctx, palette);
    this.bridgeAddress = bridgeAddress;
    contacting = ctx.getString(R.string.status_contacting);
    retryingIn = ctx.getString(R.string.status_retrying_in);
    retrying = ctx.getString(R.string.status_retrying);
  }

  @Override
  int titleRes() {
    return R.string.page_status;
  }

  @Override
  boolean buildNext() {
    int inner = Ui.CARD_WIDTH;
    switch (step++) {
      case 0:
        card = Ui.card(ctx, root, 2, Ui.PAGE_HEIGHT - 4, palette.card);
        dot = Ui.box(ctx, Ui.CARD_WIDTH / 2 - 6, 24, 12, 12, palette.clay, 6);
        shownDot = palette.clay;
        card.addView(dot);
        return true;
      case 1:
        headline = centred(48, palette.text, inner);
        advice = centred(70, palette.muted, inner);
        return true;
      default:
        Ui.labelCentred(
            ctx,
            card,
            String.format(ctx.getString(R.string.status_bridge), bridgeAddress),
            0,
            108,
            inner,
            palette.muted);
        retry = centred(130, palette.clay, inner);
        Ui.labelCentred(
            ctx, card, ctx.getString(R.string.status_retry_hint), 0, 156, inner, palette.faint);
        return false;
    }
  }

  private Line centred(int y, int color, int width) {
    return new Line(Ui.labelCentred(ctx, card, "", 0, y, width, color), "", color);
  }

  @Override
  void update(UsageService repo, long nowMs) {
    LinkState state = repo.linkState();
    String err = repo.linkErr();
    headline.show(ctx.getString(state.shortText(err)), palette.text);
    int adviceRes = state.advice(err);
    advice.show(adviceRes == 0 ? "" : ctx.getString(adviceRes), palette.muted);
    if (repo.isSyncing()) {
      retry.show(contacting, palette.clay);
    } else if (state == LinkState.JOINING || state == LinkState.NO_WIFI) {
      retry.show("", palette.clay);
    } else {
      int wait = repo.secondsToNextAttempt();
      retry.show(wait > 0 ? String.format(retryingIn, wait) : retrying, palette.clay);
    }
    int color = state == LinkState.JOINING ? palette.clay : palette.bad;
    if (color != shownDot) {
      Ui.fill(dot, color, 6);
      shownDot = color;
    }
  }
}
