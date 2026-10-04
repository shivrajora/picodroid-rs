// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.util.TimeFormat;
import picodroid.content.Context;
import picodroid.view.View;
import picodroid.widget.TextView;

/**
 * One usage window, over an inflated {@code res/layout/limit_card.xml}: a ring gauge with the reset
 * countdown inside it, the big percentage in the ring's open bottom, and a pace line under that.
 * Two of these sit side by side on the Limits page.
 */
final class LimitCard {
  private final Context ctx;
  private final Palette palette;
  private final long windowSeconds;
  private final RingView ring;
  private final TextView resetsLabel;
  private final TextView countdown;
  private final TextView detail;
  private final TextView number;

  /**
   * @param card the card's root view
   * @param captionRes what the window is called
   * @param windowSeconds how long the window is: the pace tick's scale
   */
  LimitCard(Context ctx, View card, int captionRes, long windowSeconds) {
    this.ctx = ctx;
    this.palette = Palette.of(ctx.getResources());
    this.windowSeconds = windowSeconds;
    TextView caption = card.findViewById(R.id.caption);
    caption.setText(ctx.getString(captionRes));
    ring = card.findViewById(R.id.ring);
    resetsLabel = card.findViewById(R.id.resets_label);
    countdown = card.findViewById(R.id.countdown);
    detail = card.findViewById(R.id.detail);
    number = card.findViewById(R.id.number);
  }

  /** The big percentage, dimmed while stale. */
  private void showNumber(String text, boolean dim) {
    number.setText(text);
    number.setAlpha(dim ? Ui.DIM : 1f);
  }

  private void showDetail(int textRes, int color) {
    detail.setText(ctx.getString(textRes));
    detail.setTextColor(color);
  }

  void show(int pct, long resetEpochS, long nowMs, boolean stale) {
    long leftMs = resetEpochS > 0 ? resetEpochS * 1000L - nowMs : -1;
    // Offline across a reset: the old percentage is now known to be wrong, so stop showing it.
    boolean expired = stale && resetEpochS > 0 && leftMs <= 0;
    if (pct < 0 || expired) {
      showNumber("--%", stale);
      ring.show(-1, palette.good, -1, false);
      resetsLabel.setText("");
      countdown.setText("");
      showDetail(expired ? R.string.window_has_reset : R.string.not_reported, palette.faint);
      return;
    }
    int elapsed = -1;
    if (leftMs >= 0) {
      long gone = windowSeconds - leftMs / 1000L;
      elapsed = gone <= 0 ? 0 : (gone >= windowSeconds ? 100 : (int) (gone * 100L / windowSeconds));
    }
    showNumber(pct + "%", stale);
    ring.show(pct, palette.severity(pct), elapsed, stale);
    if (leftMs > 0) {
      resetsLabel.setText(ctx.getString(R.string.resets_in_label));
      countdown.setText(TimeFormat.duration(leftMs));
    } else {
      resetsLabel.setText("");
      countdown.setText(ctx.getString(R.string.resetting));
    }
    if (elapsed < 0) {
      detail.setText("");
    } else if (pct > elapsed + 10 && pct >= palette.warnFrom) {
      showDetail(R.string.ahead_of_pace, stale ? palette.faint : palette.severity(pct));
    } else {
      detail.setText(ctx.getString(R.string.window_gone, elapsed));
      detail.setTextColor(palette.faint);
    }
  }
}
