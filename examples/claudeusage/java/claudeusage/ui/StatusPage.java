// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.LinkState;
import picodroid.view.View;
import picodroid.widget.TextView;

/**
 * Shown instead of the data screens while there has never been any data: says what is wrong, where
 * the bridge is expected, and when the next attempt is. It doubles as the setup screen, since a
 * wrong address is the likeliest first-boot problem.
 */
final class StatusPage extends UsagePage {
  private View dot;
  private TextView headline;
  private TextView advice;
  private TextView bridge;
  private TextView retry;

  @Override
  int titleRes() {
    return R.string.page_status;
  }

  @Override
  int layoutRes() {
    return R.layout.page_status;
  }

  /** Paints at once: it is what shows while there is no data. */
  @Override
  boolean needsData() {
    return false;
  }

  @Override
  void onBind(View page) {
    dot = page.findViewById(R.id.link_dot);
    headline = page.findViewById(R.id.headline);
    advice = page.findViewById(R.id.advice);
    bridge = page.findViewById(R.id.bridge);
    retry = page.findViewById(R.id.retry);
  }

  @Override
  void update(UsageUiState state, long nowMs) {
    LinkState link = state.link;
    String err = state.linkErr;
    headline.setText(getString(link.shortText(err)));
    int adviceRes = link.advice(err);
    advice.setText(adviceRes == 0 ? "" : getString(adviceRes));
    bridge.setText(
        state.address == null
            ? getString(R.string.status_searching)
            : getString(R.string.status_bridge, state.address));
    if (state.syncing) {
      retry.setText(getString(R.string.status_contacting));
    } else if (link == LinkState.JOINING || link == LinkState.NO_WIFI) {
      retry.setText("");
    } else {
      int wait = state.retrySeconds;
      retry.setText(
          wait > 0
              ? getString(R.string.status_retrying_in, wait)
              : getString(R.string.status_retrying));
    }
    Ui.tint(dot, link == LinkState.JOINING ? palette.clay : palette.bad);
  }
}
