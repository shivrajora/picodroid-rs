// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.view.ViewGroup;

/** "Opus [===== ] 61%": a name, a bar and a figure on one line. */
final class MeterRow {
  static final int HEIGHT = 21;

  private final Line name;
  private final Line figure;
  private final BarView bar;

  MeterRow(ViewGroup card, int y) {
    name = new Line(Ui.label(card, "", Ui.CARD_PAD, y, Palette.TEXT), "", Palette.TEXT);
    bar = new BarView(card, 84, y + 5, 156, 8, false);
    figure = new Line(Ui.labelRight(card, "", 246, y, 46, Palette.TEXT), "", Palette.TEXT);
  }

  void show(String label, int pct, int color, int deep, boolean stale) {
    name.show(label, stale ? Palette.MUTED : Palette.TEXT);
    bar.setVisible(true);
    bar.show(pct, color, deep, -1, stale);
    figure.show(pct < 0 ? "--" : pct + "%", stale ? Palette.MUTED : Palette.TEXT);
  }

  void clear() {
    name.show("", Palette.TEXT);
    bar.show(-1, Palette.GOOD, Palette.GOOD_DEEP, -1, false);
    bar.setVisible(false);
    figure.show("", Palette.TEXT);
  }
}
