// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import picodroid.content.res.Resources;
import picodroid.graphics.Theme;

/** The app's colours and thresholds, resolved once from {@code res/values}. */
final class Palette {
  final int background;
  final int card;
  final int track;
  final int text;
  final int muted;
  final int faint;
  final int clay;
  final int barPast;
  final int good;
  final int warn;
  final int bad;

  /** LED colours as 0xRRGGBB; off while usage is comfortable or the data is stale. */
  final int ledWarn;

  final int ledBad;

  /** Below this a limit is comfortable; from {@link #badFrom} it is nearly gone. */
  final int warnFrom;

  final int badFrom;

  Palette(Resources res) {
    background = res.getColor(R.color.background);
    card = res.getColor(R.color.card);
    track = res.getColor(R.color.track);
    text = res.getColor(R.color.text);
    muted = res.getColor(R.color.muted);
    faint = res.getColor(R.color.faint);
    clay = res.getColor(R.color.clay);
    barPast = res.getColor(R.color.bar_past);
    good = res.getColor(R.color.good);
    warn = res.getColor(R.color.warn);
    bad = res.getColor(R.color.bad);
    ledWarn = res.getColor(R.color.led_warn) & 0xFFFFFF;
    ledBad = res.getColor(R.color.led_bad) & 0xFFFFFF;
    warnFrom = res.getInteger(R.integer.warn_from);
    badFrom = res.getInteger(R.integer.bad_from);
  }

  /** The framework widgets' defaults, so a Button or a border matches the app. */
  void applyTheme() {
    Theme.colorBackground = background;
    Theme.colorSurface = card;
    Theme.colorPrimary = clay;
    Theme.colorOnPrimary = background;
    Theme.colorText = text;
    Theme.colorTextSecondary = muted;
    Theme.colorOutline = track;
  }

  int severity(int pct) {
    return pct >= badFrom ? bad : (pct >= warnFrom ? warn : good);
  }
}
