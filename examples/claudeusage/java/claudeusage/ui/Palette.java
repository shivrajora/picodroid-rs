// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import picodroid.content.res.Resources;

/**
 * The colours and thresholds the code picks between at run time, resolved once from {@code
 * res/values}. What a layout can say for itself, it says in XML.
 */
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

  private static Palette cached;

  /**
   * The palette, resolved on the first call: the resources do not change while the app runs, and a
   * page is a new fragment on every turn, which would otherwise read fifteen of them again.
   */
  static Palette of(Resources res) {
    if (cached == null) {
      cached = new Palette(res);
    }
    return cached;
  }

  private Palette(Resources res) {
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

  int severity(int pct) {
    return pct >= badFrom ? bad : (pct >= warnFrom ? warn : good);
  }
}
