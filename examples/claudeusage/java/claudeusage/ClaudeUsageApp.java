// SPDX-License-Identifier: GPL-3.0-only
package claudeusage;

import claudeusage.data.UsageRepository;
import claudeusage.ui.MainActivity;
import claudeusage.ui.Palette;
import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.graphics.Theme;

/**
 * A desk display for Claude usage limits: the 5-hour session, the weekly cap, burn rate and token
 * history, fetched from a small bridge on the LAN (see {@code bridge/claude_usage_bridge.py}).
 */
public class ClaudeUsageApp extends Application {
  public static final String TAG = "ClaudeUsage";

  @Override
  public void onCreate() {
    Theme.colorBackground = Palette.BACKGROUND;
    Theme.colorSurface = Palette.CARD;
    Theme.colorPrimary = Palette.CLAY;
    Theme.colorOnPrimary = Palette.BACKGROUND;
    Theme.colorText = Palette.TEXT;
    Theme.colorTextSecondary = Palette.MUTED;
    Theme.colorOutline = Palette.TRACK;

    UsageRepository.get().start();
    startActivity(new Intent(MainActivity.class));
  }
}
