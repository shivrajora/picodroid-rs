// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.UsageRepository;
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
  final FrameLayout root = Ui.group(0, Ui.PAGE_Y, Ui.WIDTH, Ui.PAGE_HEIGHT, Palette.BACKGROUND);

  protected int step;

  abstract String title();

  /** Adds the next few views. Returns true while there is more to build. */
  abstract boolean buildNext();

  /** Repaint from the repository. Called once built, then every second; must diff. */
  abstract void update(UsageRepository repo, long nowMs);
}
