// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.UsageService;
import picodroid.content.Context;
import picodroid.widget.FrameLayout;

/**
 * One screen's content, between the header and the footer.
 *
 * <p>A page is built a few views at a time: each view costs the RP2350 several milliseconds of LVGL
 * work, and a whole screen inside one UI tick would stall input and trip the slow-handler watchdog.
 * {@link MainActivity} calls {@link #buildNext} once per tick until it returns false, keeps the
 * page invisible meanwhile, then fades it in.
 */
abstract class Page {
  protected final Context ctx;
  protected final Palette p;
  final FrameLayout root;

  protected int step;

  Page(Context ctx, Palette p) {
    this.ctx = ctx;
    this.p = p;
    root = Ui.group(ctx, 0, 0, Ui.WIDTH, Ui.PAGE_HEIGHT, p.background);
  }

  /** The header title's string resource. */
  abstract int titleRes();

  /** Adds the next few views. Returns true while there is more to build. */
  abstract boolean buildNext();

  /** Repaint from the service. Called once built, then every second; must diff. */
  abstract void update(UsageService repo, long nowMs);
}
